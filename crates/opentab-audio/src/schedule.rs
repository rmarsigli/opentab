use opentab_playback::{Channel, MidiMessage, Performance, ScoreTime, Speed, TempoMap};

use crate::timeline::{step_of, unit_at};
use crate::{AudioError, SampleRate};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SampledEvent {
    pub frame: u64,
    pub track: usize,
    pub channel: Channel,
    pub message: MidiMessage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CachedFrame {
    at: ScoreTime,
    frame: u64,
}

pub fn frame_at(
    tempo: &TempoMap,
    at: ScoreTime,
    speed: Speed,
    rate: SampleRate,
) -> Result<u64, AudioError> {
    Ok(unit_at(tempo, at, rate)? / step_of(speed))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scheduler {
    events: Vec<SampledEvent>,
    next: usize,
}

impl Scheduler {
    pub fn new(
        performance: &Performance,
        speed: Speed,
        rate: SampleRate,
    ) -> Result<Scheduler, AudioError> {
        let mut events = Vec::with_capacity(performance.events.len());
        let mut cached: Option<CachedFrame> = None;
        for event in &performance.events {
            let frame = match cached {
                Some(hit) if hit.at == event.at => hit.frame,
                _ => {
                    let frame = frame_at(&performance.tempo, event.at, speed, rate)?;
                    cached = Some(CachedFrame {
                        at: event.at,
                        frame,
                    });
                    frame
                }
            };
            events.push(SampledEvent {
                frame,
                track: event.track,
                channel: event.channel,
                message: event.message,
            });
        }
        Ok(Scheduler { events, next: 0 })
    }

    pub fn events(&self) -> &[SampledEvent] {
        &self.events
    }

    pub fn peek(&self) -> Option<&SampledEvent> {
        self.events.get(self.next)
    }
}

impl Iterator for Scheduler {
    type Item = SampledEvent;

    fn next(&mut self) -> Option<SampledEvent> {
        let event = self.events.get(self.next).copied()?;
        self.next += 1;
        Some(event)
    }
}
