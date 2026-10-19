use crate::{MAX_FRAMES, RenderedAudio};

pub const WAV_HEADER_BYTES: usize = 44;
const CHANNELS: u16 = 2;
const BITS: u16 = 16;
const BYTES_PER_FRAME: u32 = 4;
const PCM_FORMAT: u16 = 1;
const FORMAT_CHUNK_BYTES: u32 = 16;
const RIFF_EXTRA_BYTES: u32 = 36;
const PCM_SCALE: f32 = 32_768.0;

pub fn pcm16(sample: f32) -> i16 {
    let scaled = sample * PCM_SCALE;
    let truncated = scaled as i32;
    let floored = if (truncated as f32) > scaled {
        truncated.saturating_sub(1)
    } else {
        truncated
    };
    let clamped = floored.clamp(i32::from(i16::MIN), i32::from(i16::MAX));
    i16::try_from(clamped).unwrap_or(0)
}

pub fn encode_wav(audio: &RenderedAudio) -> Vec<u8> {
    let frames = audio
        .frames
        .min(audio.left.len() as u64)
        .min(audio.right.len() as u64)
        .min(MAX_FRAMES);
    let data_bytes = u32::try_from(frames)
        .unwrap_or(0)
        .saturating_mul(BYTES_PER_FRAME);
    let rate = audio.rate.hz();
    let mut out = Vec::with_capacity(WAV_HEADER_BYTES + data_bytes as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(RIFF_EXTRA_BYTES + data_bytes).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&FORMAT_CHUNK_BYTES.to_le_bytes());
    out.extend_from_slice(&PCM_FORMAT.to_le_bytes());
    out.extend_from_slice(&CHANNELS.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * BYTES_PER_FRAME).to_le_bytes());
    out.extend_from_slice(&(CHANNELS * BITS / 8).to_le_bytes());
    out.extend_from_slice(&BITS.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_bytes.to_le_bytes());
    for (left, right) in audio
        .left
        .iter()
        .zip(&audio.right)
        .take(usize::try_from(frames).unwrap_or(0))
    {
        out.extend_from_slice(&pcm16(*left).to_le_bytes());
        out.extend_from_slice(&pcm16(*right).to_le_bytes());
    }
    out
}
