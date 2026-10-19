use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};

use claxon::{FlacReader, FlacReaderOptions};
use hound::{SampleFormat, WavReader};
use lewton::audio::{PreviousWindowRight, read_audio_packet_generic};
use lewton::header::{read_header_comment, read_header_ident, read_header_setup};
use lewton::samples::{InterleavedSamples, Samples};
use ogg::PacketReader;

use crate::sfz::limits::{MIB, SfzLimits};
use crate::sfz::vorbis::{self, VorbisSetupCost};
use crate::{BankError, UnsafeBank};

const GROWTH_FLOOR_VALUES: usize = 16_384;
const BYTES_PER_VALUE: u64 = 4;
const FLOAT_BITS: u16 = 32;
const UNKNOWN_PANIC: &str = "the sample decoder stopped without a message";

#[derive(Debug, Clone, PartialEq)]
pub struct DecodedSample {
    pub rate: u32,
    pub channels: u32,
    pub frames: usize,
    pub data: Vec<f32>,
}

impl DecodedSample {
    pub fn bytes(&self) -> u64 {
        self.data.capacity() as u64 * BYTES_PER_VALUE
    }

    pub fn peak(&self) -> f32 {
        self.data
            .iter()
            .fold(0.0f32, |peak, value| peak.max(value.abs()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioKind {
    Wav,
    Flac,
    Ogg,
}

pub fn audio_kind(bytes: &[u8]) -> Option<AudioKind> {
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        Some(AudioKind::Wav)
    } else if bytes.starts_with(b"fLaC") {
        Some(AudioKind::Flac)
    } else if bytes.starts_with(b"OggS") {
        Some(AudioKind::Ogg)
    } else {
        None
    }
}

fn invalid(reason: impl Into<String>) -> BankError {
    BankError::Invalid {
        reason: reason.into(),
    }
}

fn refuse(reason: UnsafeBank) -> BankError {
    BankError::Unsafe { reason }
}

struct Collector<'a> {
    data: Vec<f32>,
    channels: u32,
    limits: &'a SfzLimits,
    budget: u64,
}

impl Collector<'_> {
    fn new(
        channels: u32,
        rate: u32,
        limits: &SfzLimits,
        budget: u64,
    ) -> Result<Collector<'_>, BankError> {
        if channels == 0 || channels > limits.sample_channels {
            return Err(refuse(UnsafeBank::SampleChannels { channels }));
        }
        if rate < limits.min_sample_rate || rate > limits.max_sample_rate {
            return Err(refuse(UnsafeBank::SampleRate { hz: rate }));
        }
        Ok(Collector {
            data: Vec::new(),
            channels,
            limits,
            budget,
        })
    }

    fn grow(&mut self) -> Result<(), BankError> {
        let frame_cap = self
            .limits
            .sample_frames
            .saturating_mul(u64::from(self.channels));
        let budget_cap = self.budget / BYTES_PER_VALUE;
        let cap = usize::try_from(frame_cap.min(budget_cap)).unwrap_or(usize::MAX);
        let current = self.data.capacity();
        if current >= cap {
            return Err(if frame_cap <= budget_cap {
                refuse(UnsafeBank::SampleTooLong {
                    limit: self.limits.sample_frames,
                })
            } else {
                refuse(UnsafeBank::DecodedAudioBudget {
                    limit_mib: self.limits.decoded_bytes / MIB,
                })
            });
        }
        let wanted = current.saturating_mul(2).max(GROWTH_FLOOR_VALUES).min(cap);
        self.data
            .try_reserve_exact(wanted - self.data.len())
            .map_err(|_| {
                refuse(UnsafeBank::DecodedAudioBudget {
                    limit_mib: self.limits.decoded_bytes / MIB,
                })
            })
    }

    fn push(&mut self, value: f32) -> Result<(), BankError> {
        if self.data.len() == self.data.capacity() {
            self.grow()?;
        }
        self.data.push(value);
        Ok(())
    }

    fn finish(mut self, rate: u32) -> Result<DecodedSample, BankError> {
        let channels = usize::try_from(self.channels).unwrap_or(1);
        if !self.data.len().is_multiple_of(channels) {
            return Err(invalid("a sample ends in the middle of a frame"));
        }
        self.data.shrink_to_fit();
        Ok(DecodedSample {
            rate,
            channels: self.channels,
            frames: self.data.len() / channels,
            data: self.data,
        })
    }
}

fn integer_scale(bits: u32) -> f32 {
    let shift = bits.clamp(1, 32) - 1;
    (1u64 << shift) as f32
}

fn decode_wav(bytes: &[u8], limits: &SfzLimits, budget: u64) -> Result<DecodedSample, BankError> {
    let reader = WavReader::new(Cursor::new(bytes)).map_err(|error| invalid(error.to_string()))?;
    let spec = reader.spec();
    let mut collector = Collector::new(u32::from(spec.channels), spec.sample_rate, limits, budget)?;
    match spec.sample_format {
        SampleFormat::Int => {
            let scale = integer_scale(u32::from(spec.bits_per_sample));
            for sample in reader.into_samples::<i32>() {
                let value = sample.map_err(|error| invalid(error.to_string()))?;
                collector.push(value as f32 / scale)?;
            }
        }
        SampleFormat::Float if spec.bits_per_sample == FLOAT_BITS => {
            for sample in reader.into_samples::<f32>() {
                let value = sample.map_err(|error| invalid(error.to_string()))?;
                collector.push(if value.is_finite() { value } else { 0.0 })?;
            }
        }
        SampleFormat::Float => return Err(invalid("a float WAV sample is not 32-bit")),
    }
    collector.finish(spec.sample_rate)
}

fn decode_flac(bytes: &[u8], limits: &SfzLimits, budget: u64) -> Result<DecodedSample, BankError> {
    let options = FlacReaderOptions {
        metadata_only: false,
        read_vorbis_comment: false,
    };
    let mut reader = FlacReader::new_ext(Cursor::new(bytes), options)
        .map_err(|error| invalid(error.to_string()))?;
    let info = reader.streaminfo();
    let mut collector = Collector::new(info.channels, info.sample_rate, limits, budget)?;
    let scale = integer_scale(info.bits_per_sample);
    let mut blocks = reader.blocks();
    let mut buffer = Vec::new();
    loop {
        match blocks.read_next_or_eof(buffer) {
            Ok(Some(block)) => {
                if block.channels() != info.channels {
                    return Err(invalid("a FLAC block changes the channel count"));
                }
                for index in 0..block.duration() {
                    for channel in 0..info.channels {
                        collector.push(block.sample(channel, index) as f32 / scale)?;
                    }
                }
                buffer = block.into_buffer();
            }
            Ok(None) => break,
            Err(error) => return Err(invalid(error.to_string())),
        }
    }
    collector.finish(info.sample_rate)
}

fn ogg_error(error: impl ToString) -> BankError {
    invalid(error.to_string())
}

struct VorbisHeaders<'a> {
    packets: PacketReader<Cursor<&'a [u8]>>,
    ident: ogg::Packet,
    comment: ogg::Packet,
    setup: ogg::Packet,
    cost: VorbisSetupCost,
}

fn vorbis_headers<'a>(bytes: &'a [u8], limits: &SfzLimits) -> Result<VorbisHeaders<'a>, BankError> {
    let mut packets = PacketReader::new(Cursor::new(bytes));
    let ident = packets.read_packet_expected().map_err(ogg_error)?;
    let serial = ident.stream_serial();
    let shape = vorbis::check_ident(&ident.data)?;
    Collector::new(shape.channels, shape.rate, limits, 0)?;
    let mut next_header = || loop {
        let packet = packets.read_packet_expected().map_err(ogg_error)?;
        if packet.stream_serial() == serial {
            return Ok::<ogg::Packet, BankError>(packet);
        }
    };
    let comment = next_header()?;
    vorbis::check_comment(&comment.data)?;
    let setup = next_header()?;
    let cost = vorbis::check_setup(&setup.data, limits)?;
    Ok(VorbisHeaders {
        packets,
        ident,
        comment,
        setup,
        cost,
    })
}

pub fn vorbis_setup_cost(bytes: &[u8], limits: &SfzLimits) -> Result<VorbisSetupCost, BankError> {
    vorbis_headers(bytes, limits).map(|headers| headers.cost)
}

fn decode_ogg(bytes: &[u8], limits: &SfzLimits, budget: u64) -> Result<DecodedSample, BankError> {
    let VorbisHeaders {
        mut packets,
        ident,
        comment,
        setup,
        ..
    } = vorbis_headers(bytes, limits)?;
    let serial = ident.stream_serial();
    let shape = vorbis::check_ident(&ident.data)?;
    let mut collector = Collector::new(shape.channels, shape.rate, limits, budget)?;
    let ident_header = read_header_ident(&ident.data).map_err(ogg_error)?;
    read_header_comment(&comment.data).map_err(ogg_error)?;
    let setup_header = read_header_setup(
        &setup.data,
        ident_header.audio_channels,
        (ident_header.blocksize_0, ident_header.blocksize_1),
    )
    .map_err(ogg_error)?;
    let mut window = PreviousWindowRight::new();
    let mut position: Option<u64> = None;
    while let Some(packet) = packets.read_packet().map_err(ogg_error)? {
        if packet.stream_serial() != serial {
            continue;
        }
        let mut decoded: InterleavedSamples<f32> =
            read_audio_packet_generic(&ident_header, &setup_header, &packet.data, &mut window)
                .map_err(ogg_error)?;
        if let (Some(start), true) = (position, packet.last_in_stream()) {
            let keep = packet.absgp_page().saturating_sub(start);
            decoded.truncate(usize::try_from(keep).unwrap_or(usize::MAX));
        }
        if packet.last_in_page() {
            position = Some(packet.absgp_page());
        } else if let Some(start) = position.as_mut() {
            *start += decoded.num_samples() as u64;
        }
        for value in decoded.samples {
            collector.push(if value.is_finite() { value } else { 0.0 })?;
        }
        if packet.last_in_stream() {
            break;
        }
    }
    collector.finish(shape.rate)
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        UNKNOWN_PANIC.to_owned()
    }
}

pub(crate) fn decode_within(
    bytes: &[u8],
    limits: &SfzLimits,
    budget: u64,
) -> Result<DecodedSample, BankError> {
    if bytes.len() as u64 > limits.audio_file_bytes {
        return Err(refuse(UnsafeBank::AudioFileTooLarge {
            limit_mib: limits.audio_file_bytes / MIB,
        }));
    }
    let Some(kind) = audio_kind(bytes) else {
        return Err(BankError::Unsupported {
            message: "OpenTab decodes WAV, FLAC and Ogg Vorbis samples only".to_owned(),
        });
    };
    let decoded = catch_unwind(AssertUnwindSafe(|| match kind {
        AudioKind::Wav => decode_wav(bytes, limits, budget),
        AudioKind::Flac => decode_flac(bytes, limits, budget),
        AudioKind::Ogg => decode_ogg(bytes, limits, budget),
    }));
    decoded.unwrap_or_else(|payload| {
        Err(BankError::Panicked {
            message: panic_text(payload.as_ref()),
        })
    })
}

pub fn decode_audio(bytes: &[u8], limits: &SfzLimits) -> Result<DecodedSample, BankError> {
    decode_within(bytes, limits, limits.decoded_bytes)
}
