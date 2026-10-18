use std::collections::BTreeSet;
use std::ops::RangeInclusive;

use opentab_model::{Bar, Clef, InstrumentKind, Score, Track};

use crate::drums::DrumKit;
use crate::furniture::StaffShape;
use crate::geometry::{MAX_TAB_STRINGS, MIN_TAB_STRINGS, STANDARD_LINES, TAB_LINE_SPACING};
use crate::options::{LayoutError, LayoutOptions, StaffMode};
use crate::structure::{ClefSign, StaffKind};
use crate::units::STAFF_SPACE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaffSpec {
    pub track: usize,
    pub staff: usize,
    pub source: usize,
    pub kind: StaffKind,
    pub shape: StaffShape,
}

pub struct Selection {
    pub tracks: Vec<usize>,
    pub first: usize,
    pub last: usize,
    pub specs: Vec<StaffSpec>,
    pub sources: Vec<(usize, usize)>,
    pub kits: Vec<Option<DrumKit>>,
}

pub const TAB_PROGRAMS: [RangeInclusive<u8>; 2] = [24..=39, 104..=107];

pub fn tab_by_default(track: &Track) -> bool {
    let program = track.instrument.midi_program;
    (MIN_TAB_STRINGS..=MAX_TAB_STRINGS).contains(&track.tuning.strings.len())
        && TAB_PROGRAMS.iter().any(|range| range.contains(&program))
}

pub fn staff_kinds(track: &Track, mode: Option<StaffMode>) -> Vec<StaffKind> {
    let strings = track.tuning.strings.len();
    let fretted = track.instrument.kind == InstrumentKind::Fretted && strings > 0;
    if !fretted {
        return vec![StaffKind::Standard];
    }
    match mode {
        Some(StaffMode::Standard) => vec![StaffKind::Standard],
        Some(StaffMode::Tab) => vec![StaffKind::Tab],
        Some(StaffMode::Both) => vec![StaffKind::Standard, StaffKind::Tab],
        None if tab_by_default(track) => vec![StaffKind::Standard, StaffKind::Tab],
        None => vec![StaffKind::Standard],
    }
}

pub fn select(score: &Score, options: &LayoutOptions) -> Result<Selection, LayoutError> {
    let bars = score.master_bars.len();
    if bars == 0 {
        return Err(LayoutError::NoBars);
    }
    let (first, last) = match options.bars {
        Some(range) if range.first <= range.last && range.last < bars => (range.first, range.last),
        Some(range) => {
            return Err(LayoutError::Bars {
                first: range.first,
                last: range.last,
                bars,
            });
        }
        None => (0, bars - 1),
    };
    let count = score.tracks.len();
    let tracks: Vec<usize> = match &options.tracks {
        Some(chosen) => {
            if let Some(index) = chosen.iter().find(|index| **index >= count) {
                return Err(LayoutError::Track {
                    index: *index,
                    tracks: count,
                });
            }
            chosen
                .iter()
                .copied()
                .collect::<BTreeSet<usize>>()
                .into_iter()
                .collect()
        }
        None => (0..count).collect(),
    };
    let mut specs = Vec::new();
    let mut sources = Vec::new();
    let mut drawn = Vec::new();
    for track_index in &tracks {
        let Some(track) = score.tracks.get(*track_index) else {
            continue;
        };
        let kinds = staff_kinds(track, options.staff_mode);
        for staff_index in 0..track.staves.len() {
            let source = sources.len();
            sources.push((*track_index, staff_index));
            for kind in &kinds {
                let shape = match kind {
                    StaffKind::Standard => StaffShape {
                        lines: STANDARD_LINES,
                        spacing: STAFF_SPACE,
                    },
                    StaffKind::Tab => StaffShape {
                        lines: track.tuning.strings.len(),
                        spacing: TAB_LINE_SPACING,
                    },
                };
                specs.push(StaffSpec {
                    track: *track_index,
                    staff: staff_index,
                    source,
                    kind: *kind,
                    shape,
                });
            }
        }
        if !track.staves.is_empty() {
            drawn.push(*track_index);
        }
    }
    if specs.is_empty() {
        return Err(LayoutError::NoStaves);
    }
    let kits = score
        .tracks
        .iter()
        .map(|track| {
            (track.instrument.kind == InstrumentKind::Percussion).then(|| DrumKit::of(score, track))
        })
        .collect();
    Ok(Selection {
        tracks: drawn,
        first,
        last,
        specs,
        sources,
        kits,
    })
}

pub fn model_bar(score: &Score, source: (usize, usize), index: usize) -> Option<&Bar> {
    score
        .tracks
        .get(source.0)?
        .staves
        .get(source.1)?
        .bars
        .get(index)
}

pub fn standard_clef(track: &Track, bar: Option<&Bar>, previous: Option<ClefSign>) -> ClefSign {
    if track.instrument.kind == InstrumentKind::Percussion {
        return ClefSign::Neutral;
    }
    let treble = if track.transpose.octave() < 0 {
        ClefSign::Treble8vb
    } else {
        ClefSign::Treble
    };
    match bar.map(|bar| bar.clef) {
        Some(Clef::Treble) => treble,
        Some(Clef::Bass) => ClefSign::Bass,
        Some(Clef::Neutral) => ClefSign::Neutral,
        None => previous.unwrap_or(treble),
    }
}
