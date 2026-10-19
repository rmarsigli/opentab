use super::dsp::{Q16, Q30, UNITY_Q30, decay_q30, frames_of, sine};
use super::pitch::{increment, period_q16};

const SILENT_Q30: i64 = UNITY_Q30 >> 12;
const SINE_TO_Q23: u32 = 8;
const PHASE_PER_RADIAN: i64 = 683_565_276;
const SINE_Q15: u32 = 15;
const PROGRAMS_PER_FAMILY: u8 = 8;
const PERCENT: i64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ToneFamily {
    ratio_numerator: u32,
    ratio_denominator: u32,
    index_low_q16: i64,
    index_high_q16: i64,
    attack_ms: u32,
    decay_ms: u32,
    sustain_percent: i64,
    pub(crate) release_ms: u32,
    level_q16: i64,
}

const fn family(
    (ratio_numerator, ratio_denominator): (u32, u32),
    (index_low_q16, index_high_q16): (i64, i64),
    attack_ms: u32,
    decay_ms: u32,
    sustain_percent: i64,
    release_ms: u32,
) -> ToneFamily {
    ToneFamily {
        ratio_numerator,
        ratio_denominator,
        index_low_q16,
        index_high_q16,
        attack_ms,
        decay_ms,
        sustain_percent,
        release_ms,
        level_q16: 22_000,
    }
}

const FAMILIES: [ToneFamily; 16] = [
    family((1, 1), (20_000, 131_072), 3, 350, 0, 150),
    family((7, 2), (32_768, 196_608), 1, 250, 0, 150),
    family((1, 1), (65_536, 65_536), 8, 100, 90, 60),
    family((1, 1), (20_000, 131_072), 3, 350, 0, 150),
    family((1, 1), (20_000, 131_072), 3, 300, 0, 120),
    family((1, 1), (65_536, 104_858), 90, 400, 80, 200),
    family((1, 1), (65_536, 104_858), 120, 400, 80, 200),
    family((1, 1), (52_429, 196_608), 35, 200, 75, 100),
    family((2, 1), (78_643, 131_072), 30, 200, 80, 90),
    family((1, 1), (13_107, 39_322), 40, 200, 85, 90),
    family((1, 1), (163_840, 262_144), 4, 300, 85, 80),
    family((1, 2), (65_536, 98_304), 250, 500, 90, 200),
    family((3, 2), (65_536, 163_840), 50, 300, 70, 200),
    family((1, 1), (65_536, 163_840), 20, 300, 70, 100),
    family((7, 5), (65_536, 262_144), 1, 120, 0, 100),
    family((11, 3), (131_072, 393_216), 10, 300, 50, 150),
];

pub(crate) const LONGEST_TONE_RELEASE_MS: u32 = 200;

pub(crate) fn tone_family(program: u8) -> ToneFamily {
    FAMILIES
        .get(usize::from(program / PROGRAMS_PER_FAMILY))
        .copied()
        .unwrap_or(FAMILIES[0])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tone {
    family: ToneFamily,
    rate: u32,
    carrier: u32,
    modulator: u32,
    step: u32,
    envelope: i64,
    attack_step: i64,
    attacking: bool,
    decay_k: i64,
    sustain: i64,
}

impl Tone {
    pub(crate) fn start(family: ToneFamily, rate: u32, cents: i32) -> Tone {
        let mut tone = Tone {
            family,
            rate,
            carrier: 0,
            modulator: 0,
            step: 0,
            envelope: 0,
            attack_step: UNITY_Q30 / i64::from(frames_of(rate, family.attack_ms)),
            attacking: true,
            decay_k: decay_q30(rate, family.decay_ms),
            sustain: UNITY_Q30 / PERCENT * family.sustain_percent,
        };
        tone.tune(cents);
        tone
    }

    pub(crate) fn tune(&mut self, cents: i32) {
        self.step = increment(period_q16(self.rate, cents));
    }

    pub(crate) fn next(&mut self) -> i64 {
        if self.attacking {
            self.envelope += self.attack_step;
            if self.envelope >= UNITY_Q30 {
                self.envelope = UNITY_Q30;
                self.attacking = false;
            }
        } else {
            self.envelope = self.sustain + (((self.envelope - self.sustain) * self.decay_k) >> Q30);
        }
        let modulator_step = u64::from(self.step) * u64::from(self.family.ratio_numerator)
            / u64::from(self.family.ratio_denominator.max(1));
        self.modulator = self
            .modulator
            .wrapping_add(u32::try_from(modulator_step & u64::from(u32::MAX)).unwrap_or(0));
        self.carrier = self.carrier.wrapping_add(self.step);
        let span = self.family.index_high_q16 - self.family.index_low_q16;
        let index = self.family.index_low_q16 + ((span * self.envelope) >> Q30);
        let swing =
            (((i64::from(sine(self.modulator)) * index) >> Q16) * PHASE_PER_RADIAN) >> SINE_Q15;
        let offset = u32::try_from(swing & i64::from(u32::MAX)).unwrap_or(0);
        let sample = i64::from(sine(self.carrier.wrapping_add(offset))) << SINE_TO_Q23;
        (((sample * self.envelope) >> Q30) * self.family.level_q16) >> Q16
    }

    pub(crate) fn finished(&self) -> bool {
        !self.attacking && self.sustain == 0 && self.envelope < SILENT_Q30
    }
}
