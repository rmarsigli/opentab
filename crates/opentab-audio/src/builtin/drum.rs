use super::dsp::{Noise, Q16, Q30, UNITY_Q30, decay_q30, frames_of, lowpass, sine};
use super::pitch::{hertz_increment, increment, key_cents, period_q16};

const SILENT_Q30: i64 = UNITY_Q30 >> 10;
const TONE_TO_Q23: u32 = 8;
const LONGEST_DECAY_MS: u32 = 2000;
const LOW_TONE_KEY: u8 = 35;
const HIGH_TONE_KEY: u8 = 81;
const OTHER_TONE_SHIFT: u32 = 1;
const KIT_LEVEL_Q16: i64 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ToneRecipe {
    start_hz: u32,
    end_hz: u32,
    sweep_ms: u32,
    decay_ms: u32,
    level_q16: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NoiseRecipe {
    top_q16: i64,
    bottom_q16: i64,
    decay_ms: u32,
    level_q16: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DrumRecipe {
    tone: Option<ToneRecipe>,
    noise: Option<NoiseRecipe>,
}

const fn tone(
    start_hz: u32,
    end_hz: u32,
    sweep_ms: u32,
    decay_ms: u32,
    level_q16: i64,
) -> ToneRecipe {
    ToneRecipe {
        start_hz,
        end_hz,
        sweep_ms,
        decay_ms,
        level_q16,
    }
}

const fn noise(top_q16: i64, bottom_q16: i64, decay_ms: u32, level_q16: i64) -> NoiseRecipe {
    NoiseRecipe {
        top_q16,
        bottom_q16,
        decay_ms,
        level_q16,
    }
}

fn tom(end_hz: u32) -> DrumRecipe {
    DrumRecipe {
        tone: Some(tone(end_hz * 3 / 2, end_hz, 40, 140, 70_000)),
        noise: Some(noise(30_000, 2_000, 20, 8_000)),
    }
}

fn hand(hertz: u32, decay_ms: u32) -> DrumRecipe {
    DrumRecipe {
        tone: Some(tone(hertz * 5 / 4, hertz, 8, decay_ms, 50_000)),
        noise: Some(noise(40_000, 8_000, 8, 8_000)),
    }
}

fn recipe(key: u8) -> DrumRecipe {
    match key {
        35 | 36 => DrumRecipe {
            tone: Some(tone(170, 50, 22, 110, 90_000)),
            noise: Some(noise(40_000, 6_000, 4, 12_000)),
        },
        37 => DrumRecipe {
            tone: Some(tone(900, 700, 10, 12, 30_000)),
            noise: Some(noise(50_000, 12_000, 10, 30_000)),
        },
        38 | 40 => DrumRecipe {
            tone: Some(tone(240, 185, 20, 40, 40_000)),
            noise: Some(noise(42_000, 9_000, 75, 52_000)),
        },
        39 => DrumRecipe {
            tone: None,
            noise: Some(noise(36_000, 10_000, 45, 56_000)),
        },
        41 => tom(82),
        43 => tom(98),
        45 => tom(110),
        47 => tom(131),
        48 => tom(147),
        50 => tom(165),
        42 | 44 => DrumRecipe {
            tone: None,
            noise: Some(noise(62_000, 40_000, 22, 30_000)),
        },
        46 => DrumRecipe {
            tone: None,
            noise: Some(noise(62_000, 38_000, 150, 26_000)),
        },
        49 | 52 | 55 | 57 => DrumRecipe {
            tone: None,
            noise: Some(noise(60_000, 26_000, 190, 30_000)),
        },
        51 | 53 | 59 => DrumRecipe {
            tone: Some(tone(3_200, 3_100, 50, 120, 6_000)),
            noise: Some(noise(58_000, 34_000, 120, 16_000)),
        },
        54 | 69 | 70 => DrumRecipe {
            tone: None,
            noise: Some(noise(60_000, 42_000, 35, 24_000)),
        },
        56 => DrumRecipe {
            tone: Some(tone(560, 540, 20, 60, 40_000)),
            noise: None,
        },
        60 => hand(400, 60),
        61 => hand(300, 70),
        62 => hand(330, 40),
        63 => hand(260, 80),
        64 => hand(200, 90),
        65 => hand(320, 90),
        66 => hand(220, 100),
        67 => hand(650, 50),
        68 => hand(470, 50),
        75 => hand(2_500, 20),
        76 => hand(1_300, 25),
        77 => hand(900, 25),
        80 => hand(4_400, 40),
        81 => hand(4_400, 180),
        _ => DrumRecipe {
            tone: None,
            noise: Some(noise(48_000, 16_000, 40, 30_000)),
        },
    }
}

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
        let mut found = recipe(key);
        if pitched(key) {
            let period = period_q16(rate, key_cents(key));
            let hertz = (u64::from(increment(period)) * u64::from(rate)) >> 32;
            let hertz = u32::try_from(hertz >> OTHER_TONE_SHIFT).unwrap_or(1).max(1);
            found.tone = Some(tone(hertz, hertz, 10, 50, 30_000));
        }
        Drum {
            tone: found.tone.map(|recipe| DrumTone::new(recipe, rate)),
            noise: found
                .noise
                .map(|recipe| DrumNoise::new(recipe, rate, noise)),
            age: 0,
            longest: frames_of(rate, LONGEST_DECAY_MS),
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

pub(crate) const DRUM_LONGEST_MS: u32 = LONGEST_DECAY_MS;
