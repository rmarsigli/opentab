use opentab_model::{Fraction, Score, Tempo, TempoChange, TempoUnit};

use crate::glyphs::{Glyph, MET_NOTE_8TH_UP, MET_NOTE_HALF_UP, MET_NOTE_QUARTER_UP};
use crate::grid::Grid;

const MANTISSA_BITS: u32 = 52;
const EXPONENT_MASK: u64 = 0x7ff;
const EXPONENT_BIAS: i32 = 1075;
const SUBNORMAL_POWER: i32 = -1074;
const MAX_SHIFT: u32 = 120;
const TENTHS: i128 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moment {
    At(Fraction),
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkAnchor {
    Column(usize),
    Start,
    End,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TempoMark {
    pub anchor: MarkAnchor,
    pub unit: TempoUnit,
    pub bpm: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingTempo {
    pub moment: Moment,
    pub unit: TempoUnit,
    pub bpm: String,
}

pub fn unit_glyph(unit: TempoUnit) -> (Glyph, bool) {
    match unit {
        TempoUnit::Eighth => (MET_NOTE_8TH_UP, false),
        TempoUnit::Quarter => (MET_NOTE_QUARTER_UP, false),
        TempoUnit::DottedQuarter => (MET_NOTE_QUARTER_UP, true),
        TempoUnit::Half => (MET_NOTE_HALF_UP, false),
        TempoUnit::DottedHalf => (MET_NOTE_HALF_UP, true),
    }
}

pub fn tenths_of(bits: u64) -> i128 {
    let exponent = (bits >> MANTISSA_BITS) & EXPONENT_MASK;
    let fraction = i128::from(bits & ((1_u64 << MANTISSA_BITS) - 1));
    if exponent == EXPONENT_MASK {
        return 0;
    }
    let (mantissa, power) = if exponent == 0 {
        (fraction, SUBNORMAL_POWER)
    } else {
        (
            fraction | (1_i128 << MANTISSA_BITS),
            i32::try_from(exponent).unwrap_or(0) - EXPONENT_BIAS,
        )
    };
    let scaled = mantissa * TENTHS;
    let shift = power.unsigned_abs();
    if power >= 0 {
        if shift >= MAX_SHIFT {
            i128::MAX
        } else {
            1_i128
                .checked_shl(shift)
                .and_then(|factor| scaled.checked_mul(factor))
                .unwrap_or(i128::MAX)
        }
    } else if shift >= MAX_SHIFT {
        0
    } else {
        (scaled + (1_i128 << (shift - 1))) >> shift
    }
}

pub fn bpm_text(tempo: Tempo) -> String {
    let tenths = tenths_of(tempo.bpm().to_bits());
    let whole = tenths / TENTHS;
    let rest = tenths % TENTHS;
    if rest == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{rest}")
    }
}

fn sorted(changes: &[TempoChange]) -> Vec<&TempoChange> {
    let mut sorted: Vec<&TempoChange> = changes.iter().collect();
    sorted.sort_by(|left, right| left.position.cmp(&right.position));
    sorted
}

fn pending(moment: Moment, change: &TempoChange) -> PendingTempo {
    PendingTempo {
        moment,
        unit: change.unit,
        bpm: bpm_text(change.bpm),
    }
}

pub const DEFAULT_BPM: &str = "120";

pub fn default_tempo() -> PendingTempo {
    PendingTempo {
        moment: Moment::Start,
        unit: TempoUnit::Quarter,
        bpm: DEFAULT_BPM.to_string(),
    }
}

fn in_effect(score: &Score, before: usize) -> Option<&TempoChange> {
    score
        .master_bars
        .iter()
        .take(before)
        .flat_map(|master| sorted(&master.tempo_changes))
        .last()
}

pub fn plan_tempos(score: &Score, first: usize, last: usize) -> Vec<Vec<PendingTempo>> {
    let mut plans = Vec::new();
    for index in first..=last {
        let Some(master) = score.master_bars.get(index) else {
            plans.push(Vec::new());
            continue;
        };
        let zero = Fraction::from_integer(0);
        let length = master.time_signature.bar_length();
        let changes = sorted(&master.tempo_changes);
        let mut marks: Vec<PendingTempo> = Vec::new();
        if !changes.iter().any(|change| change.position == zero) {
            let carried = if index == first {
                in_effect(score, first)
            } else {
                index
                    .checked_sub(1)
                    .and_then(|before| score.master_bars.get(before))
                    .and_then(|before| {
                        let before_length = before.time_signature.bar_length();
                        sorted(&before.tempo_changes)
                            .into_iter()
                            .rfind(|change| change.position >= before_length)
                    })
            };
            match carried {
                Some(change) => marks.push(pending(Moment::Start, change)),
                None if index == first => marks.push(default_tempo()),
                None => {}
            }
        }
        for change in changes {
            let moment = if change.position > zero && change.position >= length {
                if index < last {
                    continue;
                }
                Moment::End
            } else {
                Moment::At(change.position)
            };
            if marks.last().is_some_and(|known| known.moment == moment) {
                marks.pop();
            }
            marks.push(pending(moment, change));
        }
        plans.push(marks);
    }
    plans
}

pub fn nearest_column(grid: &Grid, position: Fraction) -> Option<usize> {
    let numer = i128::from(*position.numer());
    let denom = i128::from(*position.denom());
    let target = numer.checked_mul(grid.per_whole)?;
    grid.columns
        .iter()
        .enumerate()
        .filter_map(|(index, column)| {
            let scaled = column.ticks.checked_mul(denom)?;
            Some((scaled.checked_sub(target)?.checked_abs()?, index))
        })
        .min()
        .map(|(_, index)| index)
}

pub fn resolve(grid: &Grid, pending: &[PendingTempo]) -> Vec<TempoMark> {
    pending
        .iter()
        .map(|tempo| TempoMark {
            anchor: match tempo.moment {
                Moment::Start => MarkAnchor::Start,
                Moment::End => MarkAnchor::End,
                Moment::At(position) => {
                    nearest_column(grid, position).map_or(MarkAnchor::Start, MarkAnchor::Column)
                }
            },
            unit: tempo.unit,
            bpm: tempo.bpm.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tempo_text_rounds_to_tenths_from_the_bits_without_float_arithmetic() {
        for whole in 1_i32..=1000 {
            let tempo = Tempo::new(f64::from(whole)).unwrap();
            assert_eq!(bpm_text(tempo), whole.to_string());
        }
        assert_eq!(bpm_text(Tempo::new(92.5).unwrap()), "92.5");
        assert_eq!(bpm_text(Tempo::new(1.04).unwrap()), "1");
        assert_eq!(bpm_text(Tempo::new(1.05).unwrap()), "1.1");
        assert_eq!(bpm_text(Tempo::new(999.96).unwrap()), "1000");
        assert_eq!(tenths_of(0), 0);
        assert_eq!(tenths_of(0x7ff0_0000_0000_0000), 0);
        assert_eq!(tenths_of(1), 0);
        assert_eq!(tenths_of(0x7fef_ffff_ffff_ffff), i128::MAX);
    }

    #[test]
    fn every_tenth_of_a_bpm_displays_exactly() {
        for tenths in 10_u16..=10_000 {
            let tempo = Tempo::new(f64::from(tenths) / 10.0).unwrap();
            let whole = tenths / 10;
            let rest = tenths % 10;
            let expected = if rest == 0 {
                whole.to_string()
            } else {
                format!("{whole}.{rest}")
            };
            assert_eq!(bpm_text(tempo), expected, "{tenths}");
        }
    }

    #[test]
    fn a_shift_that_would_overflow_saturates_instead_of_wrapping() {
        let largest_finite = 0x7fef_ffff_ffff_ffff;
        assert_eq!(tenths_of(largest_finite), i128::MAX);
        let two_to_the_132 = (1075_u64 + 80) << 52;
        assert_eq!(tenths_of(two_to_the_132), i128::MAX);
        let two_to_the_60 = (1075_u64 + 8) << 52;
        assert_eq!(tenths_of(two_to_the_60), (1_i128 << 60) * 10);
    }

    #[test]
    fn every_tempo_unit_has_its_metronome_glyph() {
        assert_eq!(unit_glyph(TempoUnit::Eighth), (MET_NOTE_8TH_UP, false));
        assert_eq!(unit_glyph(TempoUnit::Quarter), (MET_NOTE_QUARTER_UP, false));
        assert_eq!(
            unit_glyph(TempoUnit::DottedQuarter),
            (MET_NOTE_QUARTER_UP, true)
        );
        assert_eq!(unit_glyph(TempoUnit::Half), (MET_NOTE_HALF_UP, false));
        assert_eq!(unit_glyph(TempoUnit::DottedHalf), (MET_NOTE_HALF_UP, true));
    }
}
