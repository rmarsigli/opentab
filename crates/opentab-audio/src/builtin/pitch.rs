use super::LINE_LENGTH;

const Q30: u32 = 30;
const CENTS_PER_OCTAVE: i32 = 1200;
const CENTS_PER_SEMITONE: i32 = 100;
const CENTS_PER_TEN: i32 = 10;
const MAX_CENTS: i32 = 12_700 + 2_400;
const KEY_ZERO_PERIOD_Q40: i64 = 134_483_692_557;
const PERIOD_FROM_Q40: u32 = 24;
const INCREMENT_NUMERATOR: u64 = 1 << 48;
const BEND_CENTRE: i32 = 8192;
const MAX_BEND_RANGE: i32 = 24;
pub(crate) const MIN_PERIOD_Q16: u32 = 4 << 16;
pub(crate) const MAX_PERIOD_Q16: u32 = (LINE_LENGTH - 8) << 16;

const NEG_SEMITONE_Q30: [i64; 12] = [
    1_073_741_824,
    1_013_477_326,
    956_595_215,
    902_905_651,
    852_229_450,
    804_397_487,
    759_250_125,
    716_636_690,
    676_414_963,
    638_450_708,
    602_617_224,
    568_794_918,
];

const NEG_TEN_CENTS_Q30: [i64; 10] = [
    1_073_741_824,
    1_067_557_526,
    1_061_408_847,
    1_055_295_582,
    1_049_217_527,
    1_043_174_479,
    1_037_166_236,
    1_031_192_598,
    1_025_253_365,
    1_019_348_341,
];

const NEG_CENT_Q30: [i64; 10] = [
    1_073_741_824,
    1_073_121_785,
    1_072_502_105,
    1_071_882_782,
    1_071_263_817,
    1_070_645_210,
    1_070_026_959,
    1_069_409_066,
    1_068_791_530,
    1_068_174_350,
];

fn table(values: &[i64], index: i32) -> i64 {
    usize::try_from(index)
        .ok()
        .and_then(|index| values.get(index))
        .copied()
        .unwrap_or(1 << Q30)
}

pub(crate) fn period_q16(rate: u32, cents: i32) -> u32 {
    let cents = cents.clamp(0, MAX_CENTS);
    let octave = cents / CENTS_PER_OCTAVE;
    let within = cents % CENTS_PER_OCTAVE;
    let rest = within % CENTS_PER_SEMITONE;
    let mut period = (i64::from(rate) * KEY_ZERO_PERIOD_Q40) >> PERIOD_FROM_Q40;
    period = (period * table(&NEG_SEMITONE_Q30, within / CENTS_PER_SEMITONE)) >> Q30;
    period = (period * table(&NEG_TEN_CENTS_Q30, rest / CENTS_PER_TEN)) >> Q30;
    period = (period * table(&NEG_CENT_Q30, rest % CENTS_PER_TEN)) >> Q30;
    period >>= octave;
    let clamped = period.clamp(i64::from(MIN_PERIOD_Q16), i64::from(MAX_PERIOD_Q16));
    u32::try_from(clamped).unwrap_or(MAX_PERIOD_Q16)
}

pub(crate) fn increment(period_q16: u32) -> u32 {
    let period = u64::from(period_q16.max(MIN_PERIOD_Q16));
    u32::try_from(INCREMENT_NUMERATOR / period).unwrap_or(u32::MAX)
}

pub(crate) fn hertz_increment(rate: u32, hertz: u32) -> u32 {
    let increment = (u64::from(hertz) << 32) / u64::from(rate.max(1));
    u32::try_from(increment).unwrap_or(u32::MAX)
}

pub(crate) fn bend_cents(bend: u16, range: u8) -> i32 {
    let offset = i32::from(bend) - BEND_CENTRE;
    let range = i32::from(range).min(MAX_BEND_RANGE);
    offset * range * CENTS_PER_SEMITONE / BEND_CENTRE
}

pub(crate) fn key_cents(key: u8) -> i32 {
    i32::from(key) * CENTS_PER_SEMITONE
}
