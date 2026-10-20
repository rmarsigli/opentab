use std::fmt;
use std::sync::Arc;

use opentab_playback::{BEND_CENTRE, Channel, ChannelState, MidiMessage};

use crate::sf2::{SynthFault, SynthStage};
use crate::sfz::decode::DecodedSample;
use crate::sfz::region::{LoopMode, OffMode, SfzRegion, Trigger};
use crate::sfz::{SfzInstrument, SfzShared};
use crate::{SampleRate, SoundUse, Synth, TRACK_HEADROOM};

const CHANNELS: usize = 16;
const CONTROLLERS: usize = 128;
const DRUM_CHANNEL: u8 = 9;
const DRUM_BANK_MSB: u8 = 120;
const ALL_SOUND_OFF: u8 = 120;
const ALL_NOTES_OFF: u8 = 123;
const FIRST_MODE_CONTROLLER: u8 = 120;
const RPN_CONTROLLERS: [u8; 6] = [6, 38, 98, 99, 100, 101];
const FAST_OFF_SECONDS: f32 = 0.006;
const MIN_FADE_SECONDS: f32 = 0.001;
const RANDOM_SEED: u32 = 0x5EED_5F2A;
const RANDOM_SCALE: f32 = 16_777_216.0;
const CENTS_PER_OCTAVE: f64 = 1200.0;
const BEND_SPAN: f64 = 8192.0;
const MAX_LEVEL: f32 = 127.0;
const PAN_CENTRE: f32 = 64.0;
const PAN_LEFT_SPAN: f32 = 63.0;
pub const SFZ_MASTER_GAIN: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct ChannelSfz {
    state: ChannelState,
    cc: [u8; CONTROLLERS],
    cc_set: [bool; CONTROLLERS],
    held: [bool; CONTROLLERS],
}

impl Default for ChannelSfz {
    fn default() -> ChannelSfz {
        ChannelSfz {
            state: ChannelState::default(),
            cc: [0; CONTROLLERS],
            cc_set: [false; CONTROLLERS],
            held: [false; CONTROLLERS],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Attack,
    Hold,
    Decay,
    Sustain,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Shape {
    attack: f32,
    hold: f32,
    decay: f32,
    sustain: f32,
    release: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Fade {
    from: f32,
    frames: f32,
    done: f32,
}

#[derive(Debug, Clone)]
struct Voice {
    channel: u8,
    key: u8,
    event: u64,
    sample: Arc<DecodedSample>,
    position: f64,
    base_step: f64,
    step: f64,
    static_cents: f64,
    end: f64,
    loop_range: Option<(f64, f64)>,
    loop_mode: LoopMode,
    gain_left: f32,
    gain_right: f32,
    shape: Shape,
    stage: Stage,
    stage_done: f32,
    level: f32,
    fade: Option<Fade>,
    off_by: u32,
    off_mode: OffMode,
    off_frames: f32,
    min_fade: f32,
}

impl Voice {
    fn looping(&self) -> bool {
        match self.loop_mode {
            LoopMode::Continuous => true,
            LoopMode::Sustain => self.fade.is_none(),
            LoopMode::NoLoop | LoopMode::OneShot => false,
        }
    }

    fn envelope(&mut self) -> f32 {
        loop {
            let (length, next) = match self.stage {
                Stage::Attack => (self.shape.attack, Stage::Hold),
                Stage::Hold => (self.shape.hold, Stage::Decay),
                Stage::Decay => (self.shape.decay, Stage::Sustain),
                Stage::Sustain => break,
            };
            if self.stage_done < length {
                break;
            }
            self.stage = next;
            self.stage_done = 0.0;
        }
        let along = |done: f32, length: f32| if length > 0.0 { done / length } else { 1.0 };
        self.level = match self.stage {
            Stage::Attack => along(self.stage_done, self.shape.attack),
            Stage::Hold => 1.0,
            Stage::Decay => {
                1.0 - (1.0 - self.shape.sustain) * along(self.stage_done, self.shape.decay)
            }
            Stage::Sustain => self.shape.sustain,
        };
        self.stage_done += 1.0;
        match &mut self.fade {
            Some(fade) => {
                let left = (1.0 - fade.done / fade.frames).max(0.0);
                fade.done += 1.0;
                fade.from * left
            }
            None => self.level,
        }
    }

    fn release(&mut self, frames: f32) {
        if self.fade.is_none() {
            let level = self.level;
            self.fade = Some(Fade {
                from: level,
                frames: frames.max(self.min_fade),
                done: 0.0,
            });
        }
    }

    fn faded(&self) -> bool {
        self.fade.is_some_and(|fade| fade.done >= fade.frames)
    }

    fn frame_value(&self, index: usize, channel: usize) -> f32 {
        let channels = self.sample.channels as usize;
        self.sample
            .data
            .get(index * channels + channel.min(channels.saturating_sub(1)))
            .copied()
            .unwrap_or(0.0)
    }

    fn next(&mut self) -> Option<(f32, f32)> {
        if self.position >= self.end || self.faded() {
            return None;
        }
        let index = self.position as usize;
        let fraction = (self.position - index as f64) as f32;
        let looping = self.looping();
        let following = match self.loop_range {
            Some((start, end)) if looping && index as f64 + 1.0 >= end => start as usize,
            _ => index + 1,
        };
        let read = |channel: usize| {
            let here = self.frame_value(index, channel);
            let there = if (following as f64) < self.end {
                self.frame_value(following, channel)
            } else {
                0.0
            };
            here + (there - here) * fraction
        };
        let (left, right) = (read(0), read(1));
        let mut gain = self.envelope();
        if !looping {
            let remaining = ((self.end - self.position) / self.step) as f32;
            if remaining < self.min_fade {
                gain *= remaining / self.min_fade;
            }
        }
        self.position += self.step;
        if let Some((start, end)) = self.loop_range
            && looping
            && self.position >= end
        {
            let length = end - start;
            self.position = start + (self.position - end) % length;
        }
        Some((left * gain * self.gain_left, right * gain * self.gain_right))
    }
}

struct Started<'a> {
    region: &'a SfzRegion,
    sample: Arc<DecodedSample>,
}

pub struct SfzSynth {
    shared: Arc<SfzShared>,
    rate: SampleRate,
    instance: u32,
    channels: [ChannelSfz; CHANNELS],
    voices: Vec<Voice>,
    event: u64,
    random: u32,
    sequences: Vec<Vec<u8>>,
}

impl fmt::Debug for SfzSynth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SfzSynth")
            .field("instance", &self.instance)
            .field("rate", &self.rate)
            .field("voices", &self.voices.len())
            .finish()
    }
}

fn decibels(volume: f32) -> f32 {
    10f32.powf(volume / 20.0)
}

fn pan_gains(pan: f32) -> (f32, f32) {
    let angle = (pan + 100.0) / 200.0 * std::f32::consts::FRAC_PI_2;
    (
        angle.cos() * std::f32::consts::SQRT_2,
        angle.sin() * std::f32::consts::SQRT_2,
    )
}

fn channel_gains(state: &ChannelState) -> (f32, f32) {
    let volume = f32::from(state.volume).min(MAX_LEVEL) / MAX_LEVEL;
    let base = volume * volume;
    let pan = f32::from(state.pan).min(MAX_LEVEL);
    let left = ((MAX_LEVEL - pan) / PAN_LEFT_SPAN).min(1.0);
    let right = (pan / PAN_CENTRE).min(1.0);
    (base * left, base * right)
}

fn bend_cents(state: &ChannelState) -> f64 {
    (f64::from(state.bend) - f64::from(BEND_CENTRE)) / BEND_SPAN
        * f64::from(state.bend_range)
        * 100.0
}

fn sanitize(samples: &mut [f32]) {
    for sample in samples {
        *sample = if sample.is_finite() {
            (*sample * SFZ_MASTER_GAIN).clamp(-TRACK_HEADROOM, TRACK_HEADROOM)
        } else {
            0.0
        };
    }
}

impl SfzSynth {
    pub(crate) fn new(shared: Arc<SfzShared>, rate: SampleRate, instance: u32) -> SfzSynth {
        let sequences = shared
            .instruments
            .iter()
            .map(|instrument| vec![1; instrument.regions.len()])
            .collect();
        SfzSynth {
            shared,
            rate,
            instance,
            channels: [ChannelSfz::default(); CHANNELS],
            voices: Vec::new(),
            event: 0,
            random: RANDOM_SEED,
            sequences,
        }
    }

    pub fn sounding(&self) -> usize {
        self.voices.len()
    }

    fn seconds(&self, seconds: f32) -> f32 {
        seconds * self.rate.hz() as f32
    }

    fn draw(&mut self) -> f32 {
        let mut x = self.random;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.random = x;
        (x >> 8) as f32 / RANDOM_SCALE
    }

    fn instrument_for(&self, channel: u8) -> Option<usize> {
        let state = &self.channels.get(usize::from(channel))?.state;
        self.instrument_of(channel, state)
    }

    fn instrument_of(&self, channel: u8, state: &ChannelState) -> Option<usize> {
        if channel == DRUM_CHANNEL || state.bank.msb() == DRUM_BANK_MSB {
            self.shared.map.drums()
        } else {
            self.shared.map.melodic(state.program)
        }
    }

    fn controller(&self, channel: usize, instrument: &SfzInstrument, controller: u8) -> u8 {
        let Some(state) = self.channels.get(channel) else {
            return 0;
        };
        let index = usize::from(controller);
        if state.cc_set.get(index).copied().unwrap_or(false) {
            state.cc.get(index).copied().unwrap_or(0)
        } else {
            instrument
                .set_cc
                .iter()
                .find(|pair| pair.0 == controller)
                .map_or(0, |pair| pair.1)
        }
    }

    fn eligible(
        &self,
        channel: usize,
        instrument: &SfzInstrument,
        region: &SfzRegion,
        held_other: bool,
        random: f32,
    ) -> bool {
        let trigger_ok = match region.trigger {
            Trigger::Attack => true,
            Trigger::First => !held_other,
            Trigger::Legato => held_other,
        };
        let random_ok = region.lorand <= random && (random < region.hirand || region.hirand >= 1.0);
        let switch_ok = match region.sw_last {
            Some(switch) => instrument.active_switch == Some(switch),
            None => true,
        };
        trigger_ok
            && random_ok
            && switch_ok
            && region.cc_ranges.iter().all(|range| {
                let value = self.controller(channel, instrument, range.controller);
                (range.low..=range.high).contains(&value)
            })
    }

    fn note_on(&mut self, channel: u8, key: u8, velocity: u8) {
        let slot = usize::from(channel);
        let Some(index) = self.instrument_for(channel) else {
            return;
        };
        let shared = Arc::clone(&self.shared);
        let Some(instrument) = shared.instruments.get(index) else {
            return;
        };
        let Some(state) = self.channels.get(slot).copied() else {
            return;
        };
        let held_other = state
            .held
            .iter()
            .enumerate()
            .any(|(held_key, held)| *held && held_key != usize::from(key));
        if let Some(entry) = self
            .channels
            .get_mut(slot)
            .and_then(|state| state.held.get_mut(usize::from(key)))
        {
            *entry = true;
        }
        let random = self.draw();
        self.event += 1;
        let event = self.event;
        let mut started: Vec<Started<'_>> = Vec::new();
        for (position, play) in instrument.regions.iter().enumerate() {
            let region = &play.region;
            if !region.accepts(key, velocity)
                || !self.eligible(slot, instrument, region, held_other, random)
            {
                continue;
            }
            if region.seq_length > 1 {
                let Some(counter) = self
                    .sequences
                    .get_mut(index)
                    .and_then(|list| list.get_mut(position))
                else {
                    continue;
                };
                let matches = *counter == region.seq_position;
                *counter = if *counter >= region.seq_length {
                    1
                } else {
                    *counter + 1
                };
                if !matches {
                    continue;
                }
            }
            if started.len() >= self.shared.limits.voices_per_note {
                break;
            }
            let fetched = shared.cache.fetch(play.sample);
            match fetched.sample {
                Ok(sample) => started.push(Started { region, sample }),
                Err(error) => {
                    if fetched.first_failure {
                        shared.faults.push(SynthFault {
                            instance: self.instance,
                            stage: SynthStage::Handle,
                            message: error.to_string(),
                        });
                    }
                }
            }
        }
        for start in &started {
            if start.region.group != 0 {
                self.choke(start.region.group, event);
            }
        }
        for start in started {
            self.start(channel, key, velocity, event, start, &state.state);
        }
    }

    fn choke(&mut self, group: u32, event: u64) {
        let fast = self.seconds(FAST_OFF_SECONDS);
        for voice in &mut self.voices {
            if voice.event != event && voice.off_by == group {
                let frames = match voice.off_mode {
                    OffMode::Fast => fast,
                    OffMode::Normal => voice.shape.release,
                    OffMode::Time => voice.off_frames,
                };
                voice.release(frames);
            }
        }
    }

    fn start(
        &mut self,
        channel: u8,
        key: u8,
        velocity: u8,
        event: u64,
        start: Started<'_>,
        state: &ChannelState,
    ) {
        let region = start.region;
        let sample = start.sample;
        let frames = sample.frames as f64;
        let end = match region.end {
            Some(last) => (last as f64 + 1.0).min(frames),
            None => frames,
        };
        let offset = (region.offset as f64).min(end);
        let loop_range = match (region.loop_mode, region.loop_start, region.loop_end) {
            (LoopMode::Continuous | LoopMode::Sustain, start, finish) => {
                let loop_start = start.map_or(0.0, |frame| frame as f64).min(end);
                let loop_end = finish.map_or(end, |frame| (frame as f64 + 1.0).min(end));
                (loop_end - loop_start >= 1.0).then_some((loop_start, loop_end))
            }
            _ => None,
        };
        let static_cents = (f64::from(key) - f64::from(region.pitch_keycenter))
            * f64::from(region.pitch_keytrack)
            + f64::from(region.transpose) * 100.0
            + f64::from(region.tune);
        let base_step = f64::from(sample.rate) / f64::from(self.rate.hz());
        let step = base_step * ((static_cents + bend_cents(state)) / CENTS_PER_OCTAVE).exp2();
        let gain =
            decibels(region.volume) * region.amplitude / 100.0 * region.velocity_gain(velocity);
        let (pan_left, pan_right) = pan_gains(region.pan);
        let shape = Shape {
            attack: self.seconds(region.ampeg.attack),
            hold: self.seconds(region.ampeg.hold),
            decay: self.seconds(region.ampeg.decay),
            sustain: region.ampeg.sustain / 100.0,
            release: self.seconds(region.ampeg.release),
        };
        let voice = Voice {
            channel,
            key,
            event,
            sample,
            position: offset,
            base_step,
            step,
            static_cents,
            end,
            loop_range,
            loop_mode: region.loop_mode,
            gain_left: gain * pan_left,
            gain_right: gain * pan_right,
            shape,
            stage: Stage::Attack,
            stage_done: 0.0,
            level: 0.0,
            fade: None,
            off_by: region.off_by,
            off_mode: region.off_mode,
            off_frames: self.seconds(region.off_time),
            min_fade: self.seconds(MIN_FADE_SECONDS),
        };
        if self.voices.len() >= self.shared.limits.voices {
            self.steal();
        }
        self.voices.push(voice);
    }

    fn steal(&mut self) {
        let victim = self
            .voices
            .iter()
            .position(|voice| voice.fade.is_some())
            .unwrap_or(0);
        if victim < self.voices.len() {
            self.voices.remove(victim);
        }
    }

    fn note_off(&mut self, channel: u8, key: u8) {
        if let Some(entry) = self
            .channels
            .get_mut(usize::from(channel))
            .and_then(|state| state.held.get_mut(usize::from(key)))
        {
            *entry = false;
        }
        for voice in &mut self.voices {
            if voice.channel == channel && voice.key == key && voice.loop_mode != LoopMode::OneShot
            {
                let frames = voice.shape.release;
                voice.release(frames);
            }
        }
    }

    fn retune(&mut self, channel: u8) {
        let Some(state) = self
            .channels
            .get(usize::from(channel))
            .map(|entry| entry.state)
        else {
            return;
        };
        let bend = bend_cents(&state);
        for voice in &mut self.voices {
            if voice.channel == channel {
                voice.step =
                    voice.base_step * ((voice.static_cents + bend) / CENTS_PER_OCTAVE).exp2();
            }
        }
    }

    fn control(&mut self, channel: u8, controller: u8, value: u8) {
        let slot = usize::from(channel);
        match controller {
            ALL_SOUND_OFF => self.voices.retain(|voice| voice.channel != channel),
            ALL_NOTES_OFF => {
                for voice in &mut self.voices {
                    if voice.channel == channel {
                        let frames = voice.shape.release;
                        voice.release(frames);
                    }
                }
                if let Some(state) = self.channels.get_mut(slot) {
                    state.held = [false; CONTROLLERS];
                }
            }
            _ => {}
        }
        let Some(state) = self.channels.get_mut(slot) else {
            return;
        };
        state
            .state
            .apply(MidiMessage::Control { controller, value });
        if controller < FIRST_MODE_CONTROLLER && !RPN_CONTROLLERS.contains(&controller) {
            let index = usize::from(controller);
            if let (Some(stored), Some(set)) =
                (state.cc.get_mut(index), state.cc_set.get_mut(index))
            {
                *stored = value;
                *set = true;
            }
        }
    }
}

impl Synth for SfzSynth {
    fn reset(&mut self) {
        self.channels = [ChannelSfz::default(); CHANNELS];
        self.voices.clear();
        self.event = 0;
        self.random = RANDOM_SEED;
        for counters in &mut self.sequences {
            counters.fill(1);
        }
    }

    fn handle(&mut self, channel: Channel, message: &MidiMessage) {
        let channel = channel.get();
        if usize::from(channel) >= CHANNELS {
            return;
        }
        match *message {
            MidiMessage::NoteOn { key, velocity: 0 } | MidiMessage::NoteOff { key, .. } => {
                self.note_off(channel, key);
            }
            MidiMessage::NoteOn { key, velocity } => self.note_on(channel, key, velocity),
            MidiMessage::Control { controller, value } => self.control(channel, controller, value),
            MidiMessage::PitchBend { .. } | MidiMessage::Rpn { .. } => {
                if let Some(state) = self.channels.get_mut(usize::from(channel)) {
                    state.state.apply(*message);
                }
                self.retune(channel);
            }
            MidiMessage::Program { .. } | MidiMessage::Bank { .. } => {
                if let Some(state) = self.channels.get_mut(usize::from(channel)) {
                    state.state.apply(*message);
                }
            }
        }
    }

    fn prepare(&mut self, uses: &[SoundUse]) {
        let mut decoded = vec![false; self.shared.cache.len()];
        for found in uses {
            let state = ChannelState {
                bank: found.bank,
                program: found.program,
                ..ChannelState::default()
            };
            let Some(instrument) = self
                .instrument_of(found.channel.get(), &state)
                .and_then(|index| self.shared.instruments.get(index))
            else {
                continue;
            };
            for play in &instrument.regions {
                if let Some(seen) = decoded.get_mut(play.sample)
                    && !*seen
                {
                    *seen = true;
                    let fetched = self.shared.cache.fetch(play.sample);
                    if let (Err(error), true) = (fetched.sample, fetched.first_failure) {
                        self.shared.faults.push(SynthFault {
                            instance: self.instance,
                            stage: SynthStage::Handle,
                            message: error.to_string(),
                        });
                    }
                }
            }
        }
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let frames = left.len().min(right.len());
        left.fill(0.0);
        right.fill(0.0);
        let (left, _) = left.split_at_mut(frames);
        let (right, _) = right.split_at_mut(frames);
        let channels = self.channels;
        self.voices.retain_mut(|voice| {
            let Some(state) = channels.get(usize::from(voice.channel)) else {
                return false;
            };
            let (channel_left, channel_right) = channel_gains(&state.state);
            for (out_left, out_right) in left.iter_mut().zip(right.iter_mut()) {
                let Some((sample_left, sample_right)) = voice.next() else {
                    return false;
                };
                *out_left += sample_left * channel_left;
                *out_right += sample_right * channel_right;
            }
            true
        });
        sanitize(left);
        sanitize(right);
    }
}
