use crate::SampleRate;
use crate::rate::millis_to_frames;

pub const BUS_FRACTION_BITS: u32 = 20;
const BUS_UNITY: i64 = 1 << BUS_FRACTION_BITS;
const GAIN_BITS: u32 = 16;
const GAIN_UNITY: i64 = 1 << GAIN_BITS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LimiterSettings {
    pub gain_q16: i64,
    pub knee: i64,
    pub ceiling: i64,
    pub attack_ms: u32,
    pub release_ms: u32,
}

impl LimiterSettings {
    pub const DEFAULT: LimiterSettings = LimiterSettings {
        gain_q16: GAIN_UNITY,
        knee: BUS_UNITY / 2,
        ceiling: BUS_UNITY * 31 / 32,
        attack_ms: 2,
        release_ms: 150,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Limiter {
    settings: LimiterSettings,
    attack_frames: i64,
    release_frames: i64,
    envelope: i64,
}

fn round_shift(value: i64, bits: u32) -> i64 {
    (value + (1 << (bits - 1))) >> bits
}

fn frames(rate: SampleRate, millis: u32) -> i64 {
    i64::try_from(millis_to_frames(rate.hz(), millis).max(1)).unwrap_or(i64::MAX)
}

impl Limiter {
    pub fn new(settings: LimiterSettings, rate: SampleRate) -> Limiter {
        Limiter {
            settings,
            attack_frames: frames(rate, settings.attack_ms),
            release_frames: frames(rate, settings.release_ms),
            envelope: 0,
        }
    }

    pub fn curve(&self, level: i64) -> i64 {
        let knee = self.settings.knee;
        if level <= knee {
            return level;
        }
        let over = level - knee;
        let span = (self.settings.ceiling - knee).max(1);
        knee + over * span / (over + span)
    }

    fn clip(&self, sample: i64) -> i64 {
        if sample < 0 {
            -self.curve(-sample)
        } else {
            self.curve(sample)
        }
    }

    fn follow(&mut self, level: i64) {
        if level > self.envelope {
            let rise = level - self.envelope;
            self.envelope += (rise + self.attack_frames - 1) / self.attack_frames;
        } else {
            self.envelope -= (self.envelope - level) / self.release_frames;
        }
    }

    pub fn process(&mut self, left: &mut [i64], right: &mut [i64]) {
        for (left, right) in left.iter_mut().zip(right.iter_mut()) {
            let boosted_left = round_shift(*left * self.settings.gain_q16, GAIN_BITS);
            let boosted_right = round_shift(*right * self.settings.gain_q16, GAIN_BITS);
            self.follow(boosted_left.abs().max(boosted_right.abs()));
            let gain = if self.envelope > self.settings.knee {
                self.curve(self.envelope) * GAIN_UNITY / self.envelope
            } else {
                GAIN_UNITY
            };
            *left = self.clip(round_shift(boosted_left * gain, GAIN_BITS));
            *right = self.clip(round_shift(boosted_right * gain, GAIN_BITS));
        }
    }
}
