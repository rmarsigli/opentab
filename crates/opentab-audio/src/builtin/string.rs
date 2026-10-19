use super::dsp::{Noise, Q16, UNITY_Q16, lowpass, round_shift, soft_clip};
use super::pitch::{MIN_PERIOD_Q16, period_q16};
use super::voicing::{
    EXCITATION_PEAK_Q23, HIGH_NOTE_BOOST_Q16, HIGH_NOTE_MIN_BLEND_PERCENT, HIGH_NOTE_REFERENCE_HZ,
    STRING_MAKEUP_Q16, StringFamily,
};
use super::{LINE_LENGTH, LINE_MASK};

const ALLPASS_MIN_FRACTION_Q16: u32 = 6_554;
const VELOCITY_BRIGHTNESS_BASE: i64 = 40;
const VELOCITY_BRIGHTNESS_SPAN: i64 = 167;
const FILL_MARGIN: u32 = 2;
const QUIET_Q23: i64 = 128;
const NOISE_TO_PEAK: u32 = 1;
const PERCENT: i64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PluckedString {
    family: StringFamily,
    rate: u32,
    write: u32,
    delay_q16: u32,
    last: i64,
    quiet_run: u32,
    damping_q16: i64,
    blend_q16: i64,
    gain_q16: i64,
    whole: u32,
    allpass_q16: i64,
    allpass_in: i64,
    allpass_out: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Compensation {
    damping_q16: i64,
    blend_q16: i64,
    gain_q16: i64,
}

fn compensation(family: StringFamily, rate: u32, period_q16: u32) -> Compensation {
    let reference = (i64::from(rate) << Q16) / i64::from(HIGH_NOTE_REFERENCE_HZ);
    let ratio = (i64::from(period_q16) * UNITY_Q16 / reference.max(1)).min(UNITY_Q16);
    let loss = round_shift((UNITY_Q16 - family.damping_q16) * ratio, Q16);
    let floor = family.blend_q16 * HIGH_NOTE_MIN_BLEND_PERCENT / PERCENT;
    Compensation {
        damping_q16: UNITY_Q16 - loss,
        blend_q16: round_shift(round_shift(family.blend_q16 * ratio, Q16) * ratio, Q16).max(floor),
        gain_q16: UNITY_Q16 + round_shift(HIGH_NOTE_BOOST_Q16 * (UNITY_Q16 - ratio), Q16),
    }
}

fn triangle(index: i64, apex: i64, length: i64) -> i64 {
    if index < apex {
        EXCITATION_PEAK_Q23 * index / apex.max(1)
    } else {
        EXCITATION_PEAK_Q23 * (length - index) / (length - apex).max(1)
    }
}

impl PluckedString {
    pub(crate) fn pluck(
        line: &mut [i32],
        family: StringFamily,
        rate: u32,
        cents: i32,
        velocity: u8,
        noise: &mut Noise,
    ) -> PluckedString {
        let mut string = PluckedString {
            family,
            rate,
            write: 0,
            delay_q16: MIN_PERIOD_Q16,
            last: 0,
            quiet_run: 0,
            damping_q16: family.damping_q16,
            blend_q16: family.blend_q16,
            gain_q16: UNITY_Q16,
            whole: 1,
            allpass_q16: 0,
            allpass_in: 0,
            allpass_out: 0,
        };
        string.tune(cents);
        let brightness = ((family.pluck_q16
            * (VELOCITY_BRIGHTNESS_BASE + i64::from(velocity.min(127))))
            / VELOCITY_BRIGHTNESS_SPAN)
            .min(UNITY_Q16);
        let fill = (string.delay_q16 >> Q16) + FILL_MARGIN;
        let length = i64::from(fill);
        let apex = length / i64::from(family.position_divisor.max(2));
        line.fill(0);
        let mut filtered: i64 = 0;
        let mut sum: i64 = 0;
        for step in 0..fill {
            let shape = triangle(i64::from(step), apex, length);
            let grain = i64::from(noise.next() >> NOISE_TO_PEAK);
            let mixed = shape + round_shift((grain - shape) * family.noise_q16, Q16);
            lowpass(&mut filtered, mixed, brightness);
            let index = (LINE_LENGTH - fill + step) & LINE_MASK;
            if let Some(cell) = line.get_mut(index as usize) {
                *cell = i32::try_from(filtered).unwrap_or(0);
                sum += filtered;
            }
        }
        let mean = i32::try_from(sum / length.max(1)).unwrap_or(0);
        for step in 0..fill {
            let index = (LINE_LENGTH - fill + step) & LINE_MASK;
            if let Some(cell) = line.get_mut(index as usize) {
                *cell -= mean;
            }
        }
        string
    }

    pub(crate) fn tune(&mut self, cents: i32) {
        let period = period_q16(self.rate, cents);
        let compensated = compensation(self.family, self.rate, period);
        self.damping_q16 = compensated.damping_q16;
        self.blend_q16 = compensated.blend_q16;
        self.gain_q16 = compensated.gain_q16;
        let blend = u32::try_from(self.blend_q16).unwrap_or(0);
        let delay = period.saturating_sub(blend).max(MIN_PERIOD_Q16 >> 1);
        let whole = (delay - ALLPASS_MIN_FRACTION_Q16) >> Q16;
        let fraction = i64::from(delay - (whole << Q16));
        self.delay_q16 = delay;
        self.whole = whole;
        self.allpass_q16 = (UNITY_Q16 - fraction) * UNITY_Q16 / (UNITY_Q16 + fraction);
    }

    pub(crate) fn next(&mut self, line: &mut [i32]) -> i32 {
        let index = self.write.wrapping_sub(self.whole) & LINE_MASK;
        let input = i64::from(line.get(index as usize).copied().unwrap_or(0));
        let current =
            round_shift(self.allpass_q16 * (input - self.allpass_out), Q16) + self.allpass_in;
        self.allpass_in = input;
        self.allpass_out = current;
        let blended = current + round_shift((self.last - current) * self.blend_q16, Q16);
        let damped = round_shift(blended * self.damping_q16, Q16);
        self.last = current;
        if damped.abs() <= QUIET_Q23 {
            self.quiet_run = self.quiet_run.saturating_add(1);
        } else {
            self.quiet_run = 0;
        }
        let sample = i32::try_from(damped).unwrap_or(0);
        if let Some(cell) = line.get_mut(self.write as usize) {
            *cell = sample;
        }
        self.write = (self.write + 1) & LINE_MASK;
        let voiced = i64::from(soft_clip(sample, self.family.drive_q16));
        let level = round_shift(
            round_shift(self.family.level_q16 * STRING_MAKEUP_Q16, Q16) * self.gain_q16,
            Q16,
        );
        i32::try_from(round_shift(voiced * level, Q16)).unwrap_or(0)
    }

    pub(crate) fn finished(&self) -> bool {
        self.quiet_run >= LINE_LENGTH
    }
}
