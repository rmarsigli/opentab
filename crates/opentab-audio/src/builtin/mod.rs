mod drum;
mod dsp;
mod pitch;
mod string;
mod tone;
mod voicing;

use opentab_playback::{ChannelState, MidiMessage};

use drum::Drum;
use dsp::{Noise, Q16, Release, clamp_q23, frames_of, round_shift};
use pitch::{bend_cents, key_cents};
use string::PluckedString;
use tone::Tone;
use voicing::{
    CHOKE_RELEASE_MS, DRUM_LONGEST_MS, ENGINE_GAIN_Q16, LONGEST_MELODIC_RELEASE_MS,
    STRING_RELEASE_MS, VELOCITY_CURVE_OFFSET, string_family, tone_family,
};

pub(crate) const LINE_LENGTH: u32 = 4096;
pub(crate) const LINE_MASK: u32 = LINE_LENGTH - 1;
pub const MELODIC_RELEASE_MS: u32 = LONGEST_MELODIC_RELEASE_MS;
pub const DRUM_DECAY_MS: u32 = DRUM_LONGEST_MS;
pub const OUTPUT_FRACTION_BITS: u32 = 20;
const VOICE_TO_OUTPUT: u32 = 3;
const OUTPUT_LIMIT: i64 = 1 << 23;
const VOICES: usize = 32;
const CHANNELS: usize = 16;
const DRUM_CHANNEL: u8 = 9;
const DRUM_BANK_MSB: u8 = 120;
const ALL_SOUND_OFF: u8 = 120;
const ALL_NOTES_OFF: u8 = 123;
const MAX_LEVEL: i64 = 127;
const PAN_CENTRE: i64 = 64;
const PAN_LEFT_SPAN: i64 = 63;
const CLOSED_HATS: [u8; 2] = [42, 44];
const OPEN_HAT: u8 = 46;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Body {
    String(PluckedString),
    Drum(Drum),
    Tone(Tone),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Voice {
    channel: u8,
    key: u8,
    serial: u64,
    gain_q16: i64,
    release_frames: u32,
    release: Option<Release>,
    body: Body,
}

impl Voice {
    fn next(&mut self, line: &mut [i32]) -> Tick {
        let raw = match &mut self.body {
            Body::String(string) => i64::from(string.next(line)),
            Body::Drum(drum) => drum.next(),
            Body::Tone(tone) => tone.next(),
        };
        let mut sample = clamp_q23((raw * self.gain_q16) >> Q16);
        let mut finished = match &self.body {
            Body::String(string) => string.finished(),
            Body::Drum(drum) => drum.finished(),
            Body::Tone(tone) => tone.finished(),
        };
        if let Some(release) = &mut self.release {
            sample = release.apply(sample);
            finished |= release.done();
        }
        Tick { sample, finished }
    }

    fn release(&mut self, frames: u32) {
        if self.release.is_none() {
            self.release = Some(Release::new(frames));
        }
    }

    fn retune(&mut self, cents: i32) {
        match &mut self.body {
            Body::String(string) => string.tune(cents),
            Body::Tone(tone) => tone.tune(cents),
            Body::Drum(_) => {}
        }
    }

    fn is_drum(&self) -> bool {
        matches!(self.body, Body::Drum(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Slot {
    line: Vec<i32>,
    voice: Option<Voice>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Gains {
    left_q16: i64,
    right_q16: i64,
}

fn gains(state: &ChannelState) -> Gains {
    let volume = i64::from(state.volume).min(MAX_LEVEL);
    let base = volume * volume * (1 << Q16) / (MAX_LEVEL * MAX_LEVEL);
    let pan = i64::from(state.pan).min(MAX_LEVEL);
    let left = ((MAX_LEVEL - pan) * (1 << Q16) / PAN_LEFT_SPAN).min(1 << Q16);
    let right = (pan * (1 << Q16) / PAN_CENTRE).min(1 << Q16);
    Gains {
        left_q16: (base * left) >> Q16,
        right_q16: (base * right) >> Q16,
    }
}

fn velocity_gain(velocity: u8) -> i64 {
    let velocity = i64::from(velocity).min(MAX_LEVEL);
    velocity * (velocity + VELOCITY_CURVE_OFFSET) * (1 << Q16)
        / (MAX_LEVEL * (MAX_LEVEL + VELOCITY_CURVE_OFFSET))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Tick {
    sample: i32,
    finished: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SoundingVoice {
    pub slot: usize,
    pub channel: u8,
    pub key: u8,
    pub released: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Engine {
    rate: u32,
    channels: [ChannelState; CHANNELS],
    slots: Vec<Slot>,
    serial: u64,
}

impl Engine {
    pub(crate) fn new(rate: u32) -> Engine {
        let slots = (0..VOICES)
            .map(|_| Slot {
                line: vec![0; LINE_LENGTH as usize],
                voice: None,
            })
            .collect();
        Engine {
            rate,
            channels: [ChannelState::default(); CHANNELS],
            slots,
            serial: 0,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.channels = [ChannelState::default(); CHANNELS];
        for slot in &mut self.slots {
            slot.voice = None;
        }
        self.serial = 0;
    }

    pub(crate) fn handle(&mut self, channel: u8, message: MidiMessage) {
        let index = usize::from(channel);
        if index >= CHANNELS {
            return;
        }
        match message {
            MidiMessage::NoteOn { key, velocity: 0 } | MidiMessage::NoteOff { key, .. } => {
                self.note_off(channel, key);
            }
            MidiMessage::NoteOn { key, velocity } => self.note_on(channel, key, velocity),
            MidiMessage::Control {
                controller: ALL_SOUND_OFF,
                ..
            } => {
                for slot in &mut self.slots {
                    if slot.voice.is_some_and(|voice| voice.channel == channel) {
                        slot.voice = None;
                    }
                }
            }
            MidiMessage::Control {
                controller: ALL_NOTES_OFF,
                ..
            } => {
                let frames = frames_of(self.rate, STRING_RELEASE_MS);
                for voice in self.voices_on(channel) {
                    voice.release(frames);
                }
            }
            MidiMessage::PitchBend { .. } | MidiMessage::Rpn { .. } => {
                let Some(state) = self.channels.get_mut(index) else {
                    return;
                };
                state.apply(message);
                let bend = bend_cents(state.bend, state.bend_range);
                for voice in self.voices_on(channel) {
                    voice.retune(key_cents(voice.key) + bend);
                }
            }
            MidiMessage::Program { .. }
            | MidiMessage::Bank { .. }
            | MidiMessage::Control { .. } => {
                if let Some(state) = self.channels.get_mut(index) {
                    state.apply(message);
                }
            }
        }
    }

    fn voices_on(&mut self, channel: u8) -> impl Iterator<Item = &mut Voice> {
        self.slots
            .iter_mut()
            .filter_map(|slot| slot.voice.as_mut())
            .filter(move |voice| voice.channel == channel)
    }

    fn note_off(&mut self, channel: u8, key: u8) {
        for voice in self.voices_on(channel) {
            if voice.key == key && !voice.is_drum() {
                let frames = voice.release_frames;
                voice.release(frames);
            }
        }
    }

    fn choke(&mut self, channel: u8, keys: &[u8]) {
        let frames = frames_of(self.rate, CHOKE_RELEASE_MS);
        for voice in self.voices_on(channel) {
            if keys.contains(&voice.key) {
                voice.release(frames);
            }
        }
    }

    fn free_slot(&self) -> usize {
        if let Some(index) = self.slots.iter().position(|slot| slot.voice.is_none()) {
            return index;
        }
        let oldest = |released: bool| {
            self.slots
                .iter()
                .enumerate()
                .filter_map(|(index, slot)| slot.voice.map(|voice| (index, voice)))
                .filter(|(_, voice)| voice.release.is_some() == released)
                .min_by_key(|(_, voice)| voice.serial)
                .map(|(index, _)| index)
        };
        oldest(true).or_else(|| oldest(false)).unwrap_or(0)
    }

    fn note_on(&mut self, channel: u8, key: u8, velocity: u8) {
        let Some(state) = self.channels.get(usize::from(channel)).copied() else {
            return;
        };
        let drums = channel == DRUM_CHANNEL || state.bank.msb() == DRUM_BANK_MSB;
        if drums {
            self.choke(channel, &[key]);
            if CLOSED_HATS.contains(&key) {
                self.choke(channel, &[OPEN_HAT]);
            }
        } else {
            self.note_off(channel, key);
        }
        self.serial += 1;
        let serial = self.serial;
        let noise = Noise::seeded(
            u32::try_from(serial & u64::from(u32::MAX)).unwrap_or(0),
            key,
        );
        let cents = key_cents(key) + bend_cents(state.bend, state.bend_range);
        let index = self.free_slot();
        let rate = self.rate;
        let Some(slot) = self.slots.get_mut(index) else {
            return;
        };
        let (body, release_ms) = if drums {
            (Body::Drum(Drum::strike(key, rate, noise)), CHOKE_RELEASE_MS)
        } else if let Some(family) = string_family(state.program) {
            let mut noise = noise;
            (
                Body::String(PluckedString::pluck(
                    &mut slot.line,
                    family,
                    rate,
                    cents,
                    velocity,
                    &mut noise,
                )),
                STRING_RELEASE_MS,
            )
        } else {
            let family = tone_family(state.program);
            (
                Body::Tone(Tone::start(family, rate, cents)),
                family.release_ms,
            )
        };
        slot.voice = Some(Voice {
            channel,
            key,
            serial,
            gain_q16: velocity_gain(velocity),
            release_frames: frames_of(rate, release_ms),
            release: None,
            body,
        });
    }

    pub(crate) fn render(&mut self, left: &mut [i32], right: &mut [i32]) {
        left.fill(0);
        right.fill(0);
        for slot in &mut self.slots {
            let Slot { line, voice } = slot;
            let Some(active) = voice.as_mut() else {
                continue;
            };
            let Some(state) = self.channels.get(usize::from(active.channel)) else {
                continue;
            };
            let gains = gains(state);
            let mut finished = false;
            for (out_left, out_right) in left.iter_mut().zip(right.iter_mut()) {
                let tick = active.next(line);
                let sample = i64::from(tick.sample);
                *out_left += clamp_q23((sample * gains.left_q16) >> Q16);
                *out_right += clamp_q23((sample * gains.right_q16) >> Q16);
                if tick.finished {
                    finished = true;
                    break;
                }
            }
            if finished {
                *voice = None;
            }
        }
        for sample in left.iter_mut().chain(right.iter_mut()) {
            let output = round_shift(i64::from(*sample) * ENGINE_GAIN_Q16, Q16 + VOICE_TO_OUTPUT);
            *sample = i32::try_from(output.clamp(-OUTPUT_LIMIT, OUTPUT_LIMIT)).unwrap_or(0);
        }
    }

    pub(crate) fn voices(&self) -> Vec<SoundingVoice> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(slot, entry)| {
                entry.voice.map(|voice| SoundingVoice {
                    slot,
                    channel: voice.channel,
                    key: voice.key,
                    released: voice.release.is_some(),
                })
            })
            .collect()
    }

    pub(crate) fn sounding(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| slot.voice.is_some())
            .count()
    }
}
