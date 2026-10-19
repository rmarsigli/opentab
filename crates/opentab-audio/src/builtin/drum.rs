use super::dsp::{Noise, Q16, Q30, UNITY_Q30, decay_q30, frames_of, lowpass, sine};
use super::pitch::{hertz_increment, increment, key_cents, period_q16};
use super::voicing::{
    DRUM_LONGEST_MS, KIT_LEVEL_Q16, NoiseRecipe, ToneRecipe, UNMAPPED_DECAY_MS, UNMAPPED_LEVEL_Q16,
    UNMAPPED_SWEEP_MS, drum_recipe, tone,
};

const SILENT_Q30: i64 = UNITY_Q30 >> 10;
const TONE_TO_Q23: u32 = 8;
const LOW_TONE_KEY: u8 = 35;
const HIGH_TONE_KEY: u8 = 81;
const OTHER_TONE_SHIFT: u32 = 1;

fn pitched(key: u8) -> bool {
    !(LOW_TONE_KEY..=HIGH_TONE_KEY).contains(&key)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DrumTone {
    phase: u32,
    start: u32,
    end: u32,
    sweep: i64,
    sweep_k: i64,
    envelope: i64,
    envelope_k: i64,
    level_q16: i64,
}

impl DrumTone {
    fn new(recipe: ToneRecipe, rate: u32) -> DrumTone {
        DrumTone {
            phase: 0,
            start: hertz_increment(rate, recipe.start_hz),
            end: hertz_increment(rate, recipe.end_hz),
            sweep: UNITY_Q30,
            sweep_k: decay_q30(rate, recipe.sweep_ms),
            envelope: UNITY_Q30,
            envelope_k: decay_q30(rate, recipe.decay_ms),
            level_q16: recipe.level_q16,
        }
    }

    fn next(&mut self) -> i64 {
        let span = i64::from(self.start) - i64::from(self.end);
        let step = i64::from(self.end) + ((span * self.sweep) >> Q30);
        self.phase = self.phase.wrapping_add(u32::try_from(step).unwrap_or(0));
        let sample = i64::from(sine(self.phase)) << TONE_TO_Q23;
        let out = (((sample * self.envelope) >> Q30) * self.level_q16) >> Q16;
        self.sweep = (self.sweep * self.sweep_k) >> Q30;
        self.envelope = (self.envelope * self.envelope_k) >> Q30;
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DrumNoise {
    noise: Noise,
    top: i64,
    bottom: i64,
    top_q16: i64,
    bottom_q16: i64,
    envelope: i64,
    envelope_k: i64,
    level_q16: i64,
}

impl DrumNoise {
    fn new(recipe: NoiseRecipe, rate: u32, noise: Noise) -> DrumNoise {
        DrumNoise {
            noise,
            top: 0,
            bottom: 0,
            top_q16: recipe.top_q16,
            bottom_q16: recipe.bottom_q16,
            envelope: UNITY_Q30,
            envelope_k: decay_q30(rate, recipe.decay_ms),
            level_q16: recipe.level_q16,
        }
    }

    fn next(&mut self) -> i64 {
        lowpass(&mut self.top, i64::from(self.noise.next()), self.top_q16);
        lowpass(&mut self.bottom, self.top, self.bottom_q16);
        let band = self.top - self.bottom;
        let out = (((band * self.envelope) >> Q30) * self.level_q16) >> Q16;
        self.envelope = (self.envelope * self.envelope_k) >> Q30;
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Drum {
    tone: Option<DrumTone>,
    noise: Option<DrumNoise>,
    age: u32,
    longest: u32,
}

impl Drum {
    pub(crate) fn strike(key: u8, rate: u32, noise: Noise) -> Drum {
        let mut found = drum_recipe(key);
        if pitched(key) {
            let period = period_q16(rate, key_cents(key));
            let hertz = (u64::from(increment(period)) * u64::from(rate)) >> 32;
            let hertz = u32::try_from(hertz >> OTHER_TONE_SHIFT).unwrap_or(1).max(1);
            found.tone = Some(tone(
                hertz,
                hertz,
                UNMAPPED_SWEEP_MS,
                UNMAPPED_DECAY_MS,
                UNMAPPED_LEVEL_Q16,
            ));
        }
        Drum {
            tone: found.tone.map(|recipe| DrumTone::new(recipe, rate)),
            noise: found
                .noise
                .map(|recipe| DrumNoise::new(recipe, rate, noise)),
            age: 0,
            longest: frames_of(rate, DRUM_LONGEST_MS),
        }
    }

    pub(crate) fn next(&mut self) -> i64 {
        self.age = self.age.saturating_add(1);
        let tone = self.tone.as_mut().map_or(0, DrumTone::next);
        let noise = self.noise.as_mut().map_or(0, DrumNoise::next);
        ((tone + noise) * KIT_LEVEL_Q16) >> Q16
    }

    pub(crate) fn finished(&self) -> bool {
        let tone_quiet = self.tone.is_none_or(|tone| tone.envelope < SILENT_Q30);
        let noise_quiet = self.noise.is_none_or(|noise| noise.envelope < SILENT_Q30);
        (tone_quiet && noise_quiet) || self.age >= self.longest
    }
}
