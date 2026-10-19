use opentab_playback::{Performance, Speed};

use crate::{
    AudioError, BUS_FRACTION_BITS, Limiter, LimiterSettings, SampleRate, Scheduler, Synth, frame_at,
};

pub const BLOCK_FRAMES: usize = 512;
pub const RENDER_TAIL_MS: u32 = 3000;
pub const MAX_FRAMES: u64 = (u32::MAX as u64 - 36) / 4;
const BUS_UNITY: f32 = (1u32 << BUS_FRACTION_BITS) as f32;
pub const TRACK_HEADROOM: f32 = 8.0;

fn to_bus(sample: f32) -> i64 {
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

fn from_bus(sample: i64) -> f32 {
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

pub fn render_all(
    performance: &Performance,
    voices: Vec<Box<dyn Synth>>,
    rate: SampleRate,
) -> Result<RenderedAudio, AudioError> {
    let frames = full_length(performance, rate)?;
    render_frames(performance, voices, rate, frames)
}

pub fn render_frames(
    performance: &Performance,
    mut voices: Vec<Box<dyn Synth>>,
    rate: SampleRate,
    frames: u64,
) -> Result<RenderedAudio, AudioError> {
    if voices.len() != performance.tracks.len() {
        return Err(AudioError::VoiceCount {
            voices: voices.len(),
            tracks: performance.tracks.len(),
        });
    }
    if frames > MAX_FRAMES {
        return Err(AudioError::TooLong { frames });
    }
    let length = usize::try_from(frames).map_err(|_| AudioError::TooLong { frames })?;
    let mut events = Scheduler::new(performance, Speed::NORMAL, rate)?.peekable();
    for voice in &mut voices {
        voice.reset();
    }
    let mut left = vec![0.0; length];
    let mut right = vec![0.0; length];
    let mut scratch_left = vec![0.0; BLOCK_FRAMES];
    let mut scratch_right = vec![0.0; BLOCK_FRAMES];
    let mut bus_left = vec![0i64; BLOCK_FRAMES];
    let mut bus_right = vec![0i64; BLOCK_FRAMES];
    let mut limiter = Limiter::new(LimiterSettings::DEFAULT, rate);
    let mut cursor: usize = 0;
    loop {
        while let Some(event) = events.next_if(|event| event.frame <= cursor as u64) {
            if let Some(voice) = voices.get_mut(event.track) {
                voice.handle(event.channel, &event.message);
            }
        }
        if cursor >= length {
            break;
        }
        let next_event = events
            .peek()
            .and_then(|event| usize::try_from(event.frame).ok())
            .map_or(length, |frame| frame.min(length));
        let until = next_event.min(cursor + BLOCK_FRAMES);
        let count = until - cursor;
        let (bus_left, bus_right) = (&mut bus_left[..count], &mut bus_right[..count]);
        bus_left.fill(0);
        bus_right.fill(0);
        for voice in &mut voices {
            voice.render(&mut scratch_left[..count], &mut scratch_right[..count]);
            for (target, sample) in bus_left.iter_mut().zip(&scratch_left[..count]) {
                *target = target.saturating_add(to_bus(*sample));
            }
            for (target, sample) in bus_right.iter_mut().zip(&scratch_right[..count]) {
                *target = target.saturating_add(to_bus(*sample));
            }
        }
        limiter.process(bus_left, bus_right);
        for (target, sample) in left[cursor..until].iter_mut().zip(bus_left.iter()) {
            *target = from_bus(*sample);
        }
        for (target, sample) in right[cursor..until].iter_mut().zip(bus_right.iter()) {
            *target = from_bus(*sample);
        }
        cursor = until;
    }
    Ok(RenderedAudio {
        rate,
        frames,
        left,
        right,
    })
}
