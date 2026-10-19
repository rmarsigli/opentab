use crate::rate::millis_to_frames;

pub(crate) const Q16: u32 = 16;
pub(crate) const Q30: u32 = 30;
pub(crate) const UNITY_Q16: i64 = 1 << Q16;
pub(crate) const UNITY_Q30: i64 = 1 << Q30;
pub(crate) const FULL_SCALE: i32 = 1 << 23;
const LN2_Q30: i64 = 744_261_118;
const MILLIS_PER_SECOND: i64 = 1000;
const NOISE_SEED: u32 = 0x2545_F491;
const SEED_MIX: u32 = 0x9E37_79B9;
const NOISE_TO_Q23: u32 = 8;
const PHASE_TO_Q15: u32 = 16;
const HALF_TURN_Q15: i32 = 32_768;
const SINE_SHIFT: u32 = 13;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Noise(u32);

impl Noise {
    pub(crate) fn seeded(serial: u32, key: u8) -> Noise {
        let seed = NOISE_SEED ^ serial.wrapping_mul(SEED_MIX) ^ u32::from(key).rotate_left(Q16);
        Noise(if seed == 0 { NOISE_SEED } else { seed })
    }

    pub(crate) fn next(&mut self) -> i32 {
        let mut state = self.0;
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        self.0 = state;
        i32::from_ne_bytes(state.to_ne_bytes()) >> NOISE_TO_Q23
    }
}

pub(crate) fn sine(phase: u32) -> i32 {
    let x = i32::from_ne_bytes(phase.to_ne_bytes()) >> PHASE_TO_Q15;
    (x * (HALF_TURN_Q15 - x.abs())) >> SINE_SHIFT
}

pub(crate) fn decay_q30(rate: u32, half_life_ms: u32) -> i64 {
    let samples = i64::from(rate) * i64::from(half_life_ms.max(1));
    UNITY_Q30 - LN2_Q30 * MILLIS_PER_SECOND / samples
}

pub(crate) fn frames_of(rate: u32, millis: u32) -> u32 {
    u32::try_from(millis_to_frames(rate, millis).max(1)).unwrap_or(u32::MAX)
}

pub(crate) fn round_shift(value: i64, bits: u32) -> i64 {
    (value + (1 << (bits - 1))) >> bits
}

pub(crate) fn lowpass(state: &mut i64, input: i64, coefficient_q16: i64) {
    *state += ((input - *state) * coefficient_q16) >> Q16;
}

pub(crate) fn clamp_q23(sample: i64) -> i32 {
    let clamped = sample.clamp(-i64::from(FULL_SCALE), i64::from(FULL_SCALE - 1));
    i32::try_from(clamped).unwrap_or(0)
}

pub(crate) fn soft_clip(sample: i32, drive_q16: i64) -> i32 {
    if drive_q16 == 0 {
        return sample;
    }
    let full = i64::from(FULL_SCALE);
    let driven = ((i64::from(sample) * drive_q16) >> Q16).clamp(-full, full);
    let cube = (((driven * driven) >> 23) * driven) >> 24;
    clamp_q23(((3 * driven) >> 1) - cube)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Release {
    left: u32,
    total: u32,
}

impl Release {
    pub(crate) fn new(frames: u32) -> Release {
        Release {
            left: frames.max(1),
            total: frames.max(1),
        }
    }

    pub(crate) fn apply(&mut self, sample: i32) -> i32 {
        let scaled = i64::from(sample) * i64::from(self.left) / i64::from(self.total);
        self.left = self.left.saturating_sub(1);
        i32::try_from(scaled).unwrap_or(0)
    }

    pub(crate) fn done(self) -> bool {
        self.left == 0
    }
}
