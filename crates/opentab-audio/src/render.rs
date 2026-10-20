use opentab_playback::{Performance, Speed};

use crate::{AudioError, BUS_FRACTION_BITS, SampleRate, frame_at};

pub const BLOCK_FRAMES: usize = 512;
pub const RENDER_TAIL_MS: u32 = 3000;
pub const MAX_FRAMES: u64 = (u32::MAX as u64 - 36) / 4;
const BUS_UNITY: f32 = (1u32 << BUS_FRACTION_BITS) as f32;
pub const TRACK_HEADROOM: f32 = 8.0;

pub(crate) fn to_bus(sample: f32) -> i64 {
    if !sample.is_finite() {
        return 0;
    }
    let scaled = sample.clamp(-TRACK_HEADROOM, TRACK_HEADROOM) * BUS_UNITY;
    let truncated = scaled as i64;
    if (truncated as f32) > scaled {
        truncated.saturating_sub(1)
    } else {
        truncated
    }
}

pub(crate) fn from_bus(sample: i64) -> f32 {
    sample as f32 / BUS_UNITY
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderedAudio {
    pub rate: SampleRate,
    pub frames: u64,
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

pub fn full_length(performance: &Performance, rate: SampleRate) -> Result<u64, AudioError> {
    let end = frame_at(
        &performance.tempo,
        performance.order.end(),
        Speed::NORMAL,
        rate,
    )?;
    let last = match performance.events.last() {
        Some(event) => frame_at(&performance.tempo, event.at, Speed::NORMAL, rate)?,
        None => 0,
    };
    end.max(last)
        .checked_add(rate.frames_of_millis(RENDER_TAIL_MS))
        .ok_or(AudioError::FrameOverflow)
}
