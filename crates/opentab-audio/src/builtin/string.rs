use super::dsp::{Noise, Q16, UNITY_Q16, lowpass, soft_clip};
use super::pitch::{MIN_PERIOD_Q16, period_q16};
use super::{LINE_LENGTH, LINE_MASK};

const POSITION_MASK: u32 = (LINE_LENGTH << Q16) - 1;
const FRACTION_MASK: u32 = (1 << Q16) - 1;
const EXCITATION_PEAK_SHIFT: u32 = 1;
const VELOCITY_BRIGHTNESS_BASE: i64 = 40;
const VELOCITY_BRIGHTNESS_SPAN: i64 = 167;
const FILL_MARGIN: u32 = 2;
const MAKEUP_GAIN: i64 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StringFamily {
    pub(crate) damping_q16: i64,
    pub(crate) blend_q16: i64,
    pub(crate) pluck_q16: i64,
    pub(crate) drive_q16: i64,
    pub(crate) level_q16: i64,
}

const NYLON: StringFamily = StringFamily {
    damping_q16: 64_620,
    blend_q16: 32_768,
    pluck_q16: 22_000,
    drive_q16: 0,
    level_q16: 65_536,
};
const STEEL: StringFamily = StringFamily {
    damping_q16: 64_880,
    blend_q16: 22_000,
    pluck_q16: 46_000,
    drive_q16: 0,
    level_q16: 58_000,
};
const JAZZ: StringFamily = StringFamily {
    damping_q16: 64_772,
    blend_q16: 32_768,
    pluck_q16: 26_000,
    drive_q16: 0,
    level_q16: 62_000,
};
const CLEAN: StringFamily = StringFamily {
    damping_q16: 64_962,
    blend_q16: 24_000,
    pluck_q16: 38_000,
    drive_q16: 0,
    level_q16: 60_000,
};
const MUTED: StringFamily = StringFamily {
    damping_q16: 60_017,
    blend_q16: 32_768,
    pluck_q16: 26_000,
    drive_q16: 0,
    level_q16: 70_000,
};
const OVERDRIVE: StringFamily = StringFamily {
    damping_q16: 65_152,
    blend_q16: 20_000,
    pluck_q16: 40_000,
    drive_q16: 4 << Q16,
    level_q16: 14_000,
};
const DISTORTION: StringFamily = StringFamily {
    damping_q16: 65_208,
    blend_q16: 18_000,
    pluck_q16: 44_000,
    drive_q16: 9 << Q16,
    level_q16: 11_000,
};
const HARMONICS: StringFamily = StringFamily {
    damping_q16: 64_772,
    blend_q16: 10_000,
    pluck_q16: 60_000,
    drive_q16: 0,
    level_q16: 50_000,
};
const BASS: StringFamily = StringFamily {
    damping_q16: 63_911,
    blend_q16: 32_768,
    pluck_q16: 20_000,
    drive_q16: 0,
    level_q16: 80_000,
};
const SLAP: StringFamily = StringFamily {
    damping_q16: 63_911,
    blend_q16: 20_000,
    pluck_q16: 44_000,
    drive_q16: 0,
    level_q16: 70_000,
};
const SYNTH_BASS: StringFamily = StringFamily {
    damping_q16: 64_400,
    blend_q16: 14_000,
    pluck_q16: 50_000,
    drive_q16: 2 << Q16,
    level_q16: 30_000,
};
const SITAR: StringFamily = StringFamily {
    damping_q16: 64_880,
    blend_q16: 12_000,
    pluck_q16: 52_000,
    drive_q16: 0,
    level_q16: 52_000,
};
const BANJO: StringFamily = StringFamily {
    damping_q16: 64_225,
    blend_q16: 16_000,
    pluck_q16: 56_000,
    drive_q16: 0,
    level_q16: 56_000,
};
const KOTO: StringFamily = StringFamily {
    damping_q16: 64_620,
    blend_q16: 20_000,
    pluck_q16: 40_000,
    drive_q16: 0,
    level_q16: 60_000,
};

pub(crate) fn string_family(program: u8) -> Option<StringFamily> {
    let family = match program {
        24 => NYLON,
        25 => STEEL,
        26 => JAZZ,
        27 => CLEAN,
        28 => MUTED,
        29 => OVERDRIVE,
        30 => DISTORTION,
        31 => HARMONICS,
        32..=35 => BASS,
        36 | 37 => SLAP,
        38 | 39 => SYNTH_BASS,
        104 => SITAR,
        105 | 106 => BANJO,
        107 => KOTO,
        _ => return None,
    };
    Some(family)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PluckedString {
    family: StringFamily,
    rate: u32,
    write: u32,
    delay_q16: u32,
    last: i64,
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
        };
        string.tune(cents);
        let brightness = (family.pluck_q16
            * (VELOCITY_BRIGHTNESS_BASE + i64::from(velocity.min(127))))
            / VELOCITY_BRIGHTNESS_SPAN;
        let fill = (string.delay_q16 >> Q16) + FILL_MARGIN;
        line.fill(0);
        let mut filtered: i64 = 0;
        let mut sum: i64 = 0;
        let mut count: i64 = 0;
        for back in 1..=fill {
            let index = (LINE_LENGTH - back) & LINE_MASK;
            lowpass(
                &mut filtered,
                i64::from(noise.next() >> EXCITATION_PEAK_SHIFT),
                brightness.min(UNITY_Q16),
            );
            if let Some(cell) = line.get_mut(index as usize) {
                *cell = i32::try_from(filtered).unwrap_or(0);
                sum += filtered;
                count += 1;
            }
        }
        let mean = i32::try_from(sum / count.max(1)).unwrap_or(0);
        for back in 1..=fill {
            let index = (LINE_LENGTH - back) & LINE_MASK;
            if let Some(cell) = line.get_mut(index as usize) {
                *cell -= mean;
            }
        }
        string
    }

    pub(crate) fn tune(&mut self, cents: i32) {
        let period = period_q16(self.rate, cents);
        let blend = u32::try_from(self.family.blend_q16).unwrap_or(0);
        self.delay_q16 = period.saturating_sub(blend).max(MIN_PERIOD_Q16 >> 1);
    }

    pub(crate) fn next(&mut self, line: &mut [i32]) -> i32 {
        let read = (self.write << Q16).wrapping_sub(self.delay_q16) & POSITION_MASK;
        let index = read >> Q16;
        let fraction = i64::from(read & FRACTION_MASK);
        let older = i64::from(line.get(index as usize).copied().unwrap_or(0));
        let newer = i64::from(
            line.get(((index + 1) & LINE_MASK) as usize)
                .copied()
                .unwrap_or(0),
        );
        let current = older + (((newer - older) * fraction) >> Q16);
        let blended = current + (((self.last - current) * self.family.blend_q16) >> Q16);
        let damped = (blended * self.family.damping_q16) >> Q16;
        self.last = current;
        let sample = i32::try_from(damped).unwrap_or(0);
        if let Some(cell) = line.get_mut(self.write as usize) {
            *cell = sample;
        }
        self.write = (self.write + 1) & LINE_MASK;
        let voiced = soft_clip(sample, self.family.drive_q16);
        i32::try_from((i64::from(voiced) * self.family.level_q16 * MAKEUP_GAIN) >> Q16).unwrap_or(0)
    }
}
