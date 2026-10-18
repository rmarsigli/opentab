use std::sync::OnceLock;

use opentab_model::{Duration, Fraction, NoteValue, TimeSignature, Tuplet};

const UNITS_PER_WHOLE: i64 = 128;
const MAX_DOTS: u8 = 3;
const MAX_BAR_UNITS: usize = 768;
const TUPLETS: [(u8, u8); 4] = [(3, 2), (5, 4), (6, 4), (7, 8)];
const TUPLET_VALUES: [NoteValue; 5] = [
    NoteValue::Half,
    NoteValue::Quarter,
    NoteValue::Eighth,
    NoteValue::Sixteenth,
    NoteValue::ThirtySecond,
];
const NOTE_VALUES: [NoteValue; 8] = [
    NoteValue::Whole,
    NoteValue::Half,
    NoteValue::Quarter,
    NoteValue::Eighth,
    NoteValue::Sixteenth,
    NoteValue::ThirtySecond,
    NoteValue::SixtyFourth,
    NoteValue::OneTwentyEighth,
];

#[derive(Debug, Clone)]
pub struct Candidate {
    pub durations: Vec<Duration>,
    pub units: usize,
}

pub struct FillTable {
    candidates: Vec<Candidate>,
    min_beats: Vec<usize>,
}

fn to_units(length: Fraction) -> Option<usize> {
    let scaled = length * Fraction::from_integer(UNITS_PER_WHOLE);
    if scaled.is_integer() {
        usize::try_from(scaled.to_integer()).ok()
    } else {
        None
    }
}

fn single_candidates() -> Vec<Candidate> {
    let mut singles = Vec::new();
    for value in NOTE_VALUES {
        for dots in 0..=MAX_DOTS {
            let Ok(duration) = Duration::new(value, dots, None) else {
                continue;
            };
            if let Some(units) = to_units(duration.length()) {
                singles.push(Candidate {
                    durations: vec![duration],
                    units,
                });
            }
        }
    }
    singles.sort_by_key(|candidate| std::cmp::Reverse(candidate.units));
    singles
}

fn group_candidates() -> Vec<Candidate> {
    let mut groups = Vec::new();
    for (num, den) in TUPLETS {
        for value in TUPLET_VALUES {
            let Ok(tuplet) = Tuplet::new(num, den) else {
                continue;
            };
            let Ok(duration) = Duration::new(value, 0, Some(tuplet)) else {
                continue;
            };
            let durations = vec![duration; usize::from(num)];
            let total = durations
                .iter()
                .fold(Fraction::from_integer(0), |sum, item| sum + item.length());
            if let Some(units) = to_units(total) {
                groups.push(Candidate { durations, units });
            }
        }
    }
    groups.sort_by_key(|candidate| std::cmp::Reverse(candidate.units));
    groups
}

fn build_table() -> FillTable {
    let singles = single_candidates();
    let mut min_beats = vec![0usize; MAX_BAR_UNITS + 1];
    for units in 1..=MAX_BAR_UNITS {
        let best = singles
            .iter()
            .filter(|candidate| candidate.units <= units)
            .filter_map(|candidate| min_beats.get(units - candidate.units).copied())
            .min()
            .unwrap_or(usize::MAX);
        if let Some(slot) = min_beats.get_mut(units) {
            *slot = best.saturating_add(1);
        }
    }
    let mut candidates = singles;
    candidates.extend(group_candidates());
    FillTable {
        candidates,
        min_beats,
    }
}

impl FillTable {
    pub fn shared() -> &'static FillTable {
        static TABLE: OnceLock<FillTable> = OnceLock::new();
        TABLE.get_or_init(build_table)
    }

    fn min_beats(&self, units: usize) -> usize {
        self.min_beats.get(units).copied().unwrap_or(usize::MAX)
    }

    pub fn fill_bar(
        &self,
        time_signature: TimeSignature,
        beat_cap: usize,
        picks: &[usize],
    ) -> Vec<Duration> {
        let mut remaining = to_units(time_signature.bar_length()).unwrap_or(0);
        let cap = beat_cap.max(self.min_beats(remaining));
        let mut out: Vec<Duration> = Vec::new();
        while remaining > 0 {
            let left = cap.saturating_sub(out.len());
            let options: Vec<&Candidate> = self
                .candidates
                .iter()
                .filter(|candidate| {
                    candidate.units <= remaining
                        && candidate.durations.len() <= left
                        && self.min_beats(remaining - candidate.units)
                            <= left - candidate.durations.len()
                })
                .collect();
            let pick = picks.get(out.len()).copied().unwrap_or(0);
            let Some(index) = pick.checked_rem(options.len()) else {
                break;
            };
            let Some(chosen) = options.get(index) else {
                break;
            };
            remaining -= chosen.units;
            out.extend(chosen.durations.iter().copied());
        }
        out
    }
}
