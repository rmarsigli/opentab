use super::dsp::{Q16, Q30, UNITY_Q30, decay_q30, frames_of, sine};
use super::pitch::{increment, period_q16};
use super::voicing::ToneFamily;

const SILENT_Q30: i64 = UNITY_Q30 >> 12;
const SINE_TO_Q23: u32 = 8;
const PHASE_PER_RADIAN: i64 = 683_565_276;
const SINE_Q15: u32 = 15;
const PERCENT: i64 = 100;

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
