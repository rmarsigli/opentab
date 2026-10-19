const PCM_TAG: u16 = 1;
const EXTENSIBLE_TAG: u16 = 0xFFFE;
const PLAIN_FMT_BYTES: u32 = 16;
const EXTENSIBLE_FMT_BYTES: u32 = 40;
const EXTENSION_BYTES: u16 = 22;
const PCM_SUBFORMAT: [u8; 16] = [
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71,
];
const MONO_MASK: u32 = 0x4;
const STEREO_MASK: u32 = 0x3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavSpec {
    pub rate: u32,
    pub channels: u16,
    pub bits: u16,
    pub extensible: bool,
}

impl WavSpec {
    pub const fn mono16(rate: u32) -> WavSpec {
        WavSpec {
            rate,
            channels: 1,
            bits: 16,
            extensible: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavOverrides {
    pub declared_data_bytes: Option<u32>,
    pub truncate_to: Option<usize>,
}

pub fn write_wav(spec: &WavSpec, interleaved: &[i32]) -> Vec<u8> {
    write_wav_with(
        spec,
        interleaved,
        &WavOverrides {
            declared_data_bytes: None,
            truncate_to: None,
        },
    )
}

pub fn write_wav_with(spec: &WavSpec, interleaved: &[i32], overrides: &WavOverrides) -> Vec<u8> {
    let bytes_per_sample = usize::from(spec.bits.div_ceil(8));
    let mut data = Vec::with_capacity(interleaved.len() * bytes_per_sample);
    for sample in interleaved {
        let bytes = sample.to_le_bytes();
        data.extend_from_slice(&bytes[..bytes_per_sample]);
    }
    let block_align = spec.channels * spec.bits.div_ceil(8);
    let byte_rate = spec.rate * u32::from(block_align);
    let fmt_bytes = if spec.extensible {
        EXTENSIBLE_FMT_BYTES
    } else {
        PLAIN_FMT_BYTES
    };
    let data_len = u32::try_from(data.len()).unwrap_or(u32::MAX);
    let declared = overrides.declared_data_bytes.unwrap_or(data_len);
    let mut out = Vec::with_capacity(data.len() + 80);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(4 + 8 + fmt_bytes + 8 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&fmt_bytes.to_le_bytes());
    let tag = if spec.extensible {
        EXTENSIBLE_TAG
    } else {
        PCM_TAG
    };
    out.extend_from_slice(&tag.to_le_bytes());
    out.extend_from_slice(&spec.channels.to_le_bytes());
    out.extend_from_slice(&spec.rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&(spec.bits.div_ceil(8) * 8).to_le_bytes());
    if spec.extensible {
        out.extend_from_slice(&EXTENSION_BYTES.to_le_bytes());
        out.extend_from_slice(&spec.bits.to_le_bytes());
        let mask = if spec.channels == 1 {
            MONO_MASK
        } else {
            STEREO_MASK
        };
        out.extend_from_slice(&mask.to_le_bytes());
        out.extend_from_slice(&PCM_SUBFORMAT);
    }
    out.extend_from_slice(b"data");
    out.extend_from_slice(&declared.to_le_bytes());
    out.extend_from_slice(&data);
    if let Some(length) = overrides.truncate_to {
        out.truncate(length);
    }
    out
}
