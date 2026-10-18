mod draw;
pub mod fit;
mod tempo;
mod title;

use std::collections::BTreeMap;

use opentab_model::{Bar, Beat, Dynamic, Marker, MasterBar};

pub use draw::{SystemMarks, draw_marks};
pub use fit::{ELLIPSIS, Fitted, ascent, clean, descent, fit_line, ink, line_height, wrap};
pub use tempo::{
    MarkAnchor, Moment, PendingTempo, TempoMark, bpm_text, nearest_column, plan_tempos, resolve,
    unit_glyph,
};
pub use title::{
    ALBUM_EM, ARTIST_EM, BLOCK_GAP, CREDIT_EM, Frame, SUBTITLE_EM, SUBTITLE_LINES, TITLE_EM,
    TITLE_LINES, TitleBlock, credits, tab_credit, title_block,
};

use crate::geometry::{COUNT_EM, COUNT_RISE, ENDING_RISE, NOTATION_CLEARANCE};
use crate::glyphs::{
    DYNAMIC_FF, DYNAMIC_FFF, DYNAMIC_FORTE, DYNAMIC_MF, DYNAMIC_MP, DYNAMIC_PIANO, DYNAMIC_PP,
    DYNAMIC_PPP, Glyph, MET_AUGMENTATION_DOT, MET_NOTE_8TH_UP, MET_NOTE_HALF_UP,
    MET_NOTE_QUARTER_UP, Metrics,
};
use crate::grid::Grid;
use crate::lanes::{Lane, LaneSet};
use crate::notation::{ColumnNeed, StaffNotation};
use crate::plan::{VerticalNeeds, below_share};
use crate::rhythm::glyph_box;
use crate::select::{Selection, StaffSpec};
use crate::structure::StaffKind;
use crate::tab::TabNotation;
use crate::units::{STAFF_SPACE, Units};

pub const CHORD_EM: Units = Units(STAFF_SPACE.0 * 3 / 2);
pub const TEXT_EM: Units = Units(STAFF_SPACE.0 * 5 / 4);
pub const TEMPO_EM: Units = Units(STAFF_SPACE.0 * 7 / 5);
pub const TEMPO_SIZE: Units = Units(STAFF_SPACE.0 * 3 / 5);
pub const TEMPO_DOT_GAP: Units = Units(STAFF_SPACE.0 / 5);
pub const TEMPO_TEXT_GAP: Units = Units(STAFF_SPACE.0 * 2 / 5);
pub const SECTION_EM: Units = Units(STAFF_SPACE.0 * 7 / 5);
pub const SECTION_PAD: Units = Units(STAFF_SPACE.0 / 4);
pub const SECTION_LINE: Units = Units(STAFF_SPACE.0 / 8);
pub const SECTION_GAP: Units = Units(STAFF_SPACE.0 * 2 / 5);
pub const LETTER_MAX: Units = Units(STAFF_SPACE.0 * 6);
pub const CHORD_MAX: Units = Units(STAFF_SPACE.0 * 20);
pub const MARK_GAP: Units = Units(STAFF_SPACE.0 / 2);
pub const TEXT_BACK: Units = Units(STAFF_SPACE.0 * 4);
pub const DYNAMIC_SIZE: Units = STAFF_SPACE;
pub const DYNAMIC_GAP: Units = Units(STAFF_SPACE.0 * 3 / 4);
pub const NUMBER_GAP: Units = Units(STAFF_SPACE.0 / 2);

pub const DYNAMIC_GLYPHS: [Glyph; 8] = [
    DYNAMIC_PPP,
    DYNAMIC_PP,
    DYNAMIC_PIANO,
    DYNAMIC_MP,
    DYNAMIC_MF,
    DYNAMIC_FORTE,
    DYNAMIC_FF,
    DYNAMIC_FFF,
];

pub fn dynamic_glyph(dynamic: Dynamic) -> Glyph {
    match dynamic {
        Dynamic::Ppp => DYNAMIC_PPP,
        Dynamic::Pp => DYNAMIC_PP,
        Dynamic::P => DYNAMIC_PIANO,
        Dynamic::Mp => DYNAMIC_MP,
        Dynamic::Mf => DYNAMIC_MF,
        Dynamic::F => DYNAMIC_FORTE,
        Dynamic::Ff => DYNAMIC_FF,
        Dynamic::Fff => DYNAMIC_FFF,
    }
}

fn reach(glyphs: &[Glyph], size: Units) -> (Units, Units) {
    glyphs
        .iter()
        .fold((Units::ZERO, Units::ZERO), |(up, down), glyph| {
            let rect = glyph_box(*glyph, size);
            (up.max(-rect.min.y), down.max(rect.max.y))
        })
}

pub fn dynamic_ascent() -> Units {
    reach(&DYNAMIC_GLYPHS, DYNAMIC_SIZE).0
}

pub fn dynamic_height() -> Units {
    let (up, down) = reach(&DYNAMIC_GLYPHS, DYNAMIC_SIZE);
    up + down
}

pub fn tempo_glyph_height() -> Units {
    let (up, down) = reach(
        &[
            MET_NOTE_8TH_UP,
            MET_NOTE_QUARTER_UP,
            MET_NOTE_HALF_UP,
            MET_AUGMENTATION_DOT,
        ],
        TEMPO_SIZE,
    );
    up + down
}

pub fn section_box_height() -> Units {
    line_height(SECTION_EM) + SECTION_PAD + SECTION_PAD + SECTION_LINE
}

pub fn lane_height(lane: Lane) -> Units {
    match lane {
        Lane::Chord => line_height(CHORD_EM),
        Lane::Section => section_box_height(),
        Lane::Tempo => ascent(TEMPO_EM).max(tempo_glyph_height()) + descent(TEMPO_EM),
        Lane::Text | Lane::Marks | Lane::Vibrato | Lane::LetRing | Lane::PalmMute => {
            line_height(TEXT_EM)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnText {
    pub column: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnChord {
    pub column: usize,
    pub name: Fitted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DynamicMark {
    pub column: usize,
    pub dynamic: Dynamic,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RowMarks {
    pub chords: Vec<ColumnChord>,
    pub texts: Vec<ColumnText>,
    pub dynamics: Vec<DynamicMark>,
    pub floor: Units,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionMark {
    pub letter: Option<Fitted>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BarMarks {
    pub tempos: Vec<TempoMark>,
    pub section: Option<SectionMark>,
    pub rows: Vec<RowMarks>,
    pub columns: Vec<ColumnNeed>,
    pub endings: bool,
    pub count: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MarkState {
    pub dynamics: BTreeMap<(usize, u8), Dynamic>,
}

pub struct MarkInput<'a> {
    pub selection: &'a Selection,
    pub master: &'a MasterBar,
    pub bars: &'a [Option<&'a Bar>],
    pub grid: &'a Grid,
    pub notation: &'a [Option<StaffNotation>],
    pub tab: &'a [Option<TabNotation>],
    pub tempos: &'a [PendingTempo],
}

fn section_of(marker: Option<&Marker>) -> Option<SectionMark> {
    let marker = marker?;
    let letter = fit_line(&marker.letter, SECTION_EM, LETTER_MAX);
    let text = clean(&marker.text);
    (letter.is_some() || !text.is_empty()).then_some(SectionMark { letter, text })
}

fn beat_at<'a>(
    input: &MarkInput<'a>,
    source: usize,
    voice: usize,
    beat: usize,
) -> Option<&'a Beat> {
    input
        .bars
        .get(source)
        .copied()
        .flatten()?
        .voices
        .get(voice)?
        .beats
        .get(beat)
}

fn collect(
    input: &MarkInput<'_>,
    sources: &[usize],
    pick: fn(&Beat) -> Option<&String>,
) -> Vec<ColumnText> {
    let mut found: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for source in sources {
        for slot in input.grid.slots.get(*source).map_or(&[][..], Vec::as_slice) {
            let Some(beat) = beat_at(input, *source, slot.voice, slot.beat) else {
                continue;
            };
            let Some(text) = pick(beat).map(|text| clean(text)) else {
                continue;
            };
            if text.is_empty() {
                continue;
            }
            let words = found.entry(slot.column).or_default();
            if !words.contains(&text) {
                words.push(text);
            }
        }
    }
    found
        .into_iter()
        .map(|(column, words)| ColumnText {
            column,
            text: words.join(" "),
        })
        .collect()
}

fn floor_of(input: &MarkInput<'_>, row: usize, spec: &StaffSpec) -> Units {
    let height = spec.shape.height();
    let ink = match spec.kind {
        StaffKind::Standard => input
            .notation
            .get(row)
            .and_then(Option::as_ref)
            .map(|notation| notation.bottom),
        StaffKind::Tab => input
            .tab
            .get(row)
            .and_then(Option::as_ref)
            .map(|tab| tab.bottom),
    };
    ink.map_or(height, |bottom| bottom.max(height))
}

fn dynamic_row(specs: &[StaffSpec], source: usize) -> Option<usize> {
    specs
        .iter()
        .position(|spec| spec.source == source && spec.kind == StaffKind::Standard)
        .or_else(|| specs.iter().position(|spec| spec.source == source))
}

fn dynamics(input: &MarkInput<'_>, source: usize, state: &mut MarkState) -> Vec<DynamicMark> {
    let mut marks: BTreeMap<usize, Dynamic> = BTreeMap::new();
    for slot in input.grid.slots.get(source).map_or(&[][..], Vec::as_slice) {
        if slot.grace.is_some() {
            continue;
        }
        let Some(voice) = input
            .bars
            .get(source)
            .copied()
            .flatten()
            .and_then(|bar| bar.voices.get(slot.voice))
        else {
            continue;
        };
        let Some(beat) = voice.beats.get(slot.beat) else {
            continue;
        };
        if beat.notes.is_empty() {
            continue;
        }
        let key = (source, voice.slot.get());
        let before = state.dynamics.get(&key).copied().unwrap_or_default();
        state.dynamics.insert(key, beat.effects.dynamic);
        if before != beat.effects.dynamic {
            marks.entry(slot.column).or_insert(beat.effects.dynamic);
        }
    }
    marks
        .into_iter()
        .map(|(column, dynamic)| DynamicMark { column, dynamic })
        .collect()
}

fn widen(columns: &mut [ColumnNeed], column: usize, width: Units) {
    if let Some(need) = columns.get_mut(column) {
        need.right = need.right.max(width);
    }
}

pub fn analyse_marks(input: &MarkInput<'_>, state: &mut MarkState) -> BarMarks {
    let specs = &input.selection.specs;
    let mut rows = vec![RowMarks::default(); specs.len()];
    let mut columns = vec![ColumnNeed::default(); input.grid.columns.len()];
    let mut seen_tracks: Vec<usize> = Vec::new();
    for (row, spec) in specs.iter().enumerate() {
        rows[row].floor = floor_of(input, row, spec);
        if seen_tracks.contains(&spec.track) {
            continue;
        }
        seen_tracks.push(spec.track);
        let mut sources: Vec<usize> = specs
            .iter()
            .filter(|other| other.track == spec.track)
            .map(|other| other.source)
            .collect();
        sources.dedup();
        rows[row].chords = collect(input, &sources, |beat| beat.effects.chord_name.as_ref())
            .into_iter()
            .filter_map(|found| {
                fit_line(&found.text, CHORD_EM, CHORD_MAX).map(|name| ColumnChord {
                    column: found.column,
                    name,
                })
            })
            .collect();
        rows[row].texts = collect(input, &sources, |beat| beat.effects.text.as_ref());
    }
    let mut sources: Vec<usize> = specs.iter().map(|spec| spec.source).collect();
    sources.dedup();
    for source in sources {
        let Some(row) = dynamic_row(specs, source) else {
            continue;
        };
        rows[row].dynamics = dynamics(input, source, state);
    }
    for row in &rows {
        for chord in &row.chords {
            widen(&mut columns, chord.column, chord.name.width());
        }
        for mark in &row.dynamics {
            widen(
                &mut columns,
                mark.column,
                glyph_box(dynamic_glyph(mark.dynamic), DYNAMIC_SIZE).width(),
            );
        }
    }
    BarMarks {
        tempos: resolve(input.grid, input.tempos),
        section: section_of(input.master.marker.as_ref()),
        rows,
        columns,
        endings: input.master.alternate_endings != 0,
        count: input.master.repeat_end.is_some_and(|count| count > 2)
            && input.master.alternate_endings == 0,
    }
}

impl BarMarks {
    pub fn lanes(&self, row: usize) -> LaneSet {
        let mut lanes = LaneSet::EMPTY;
        if row == 0 {
            if !self.tempos.is_empty() {
                lanes = lanes.with(Lane::Tempo);
            }
            if self.section.is_some() {
                lanes = lanes.with(Lane::Section);
            }
        }
        if let Some(marks) = self.rows.get(row) {
            if !marks.chords.is_empty() {
                lanes = lanes.with(Lane::Chord);
            }
            if !marks.texts.is_empty() {
                lanes = lanes.with(Lane::Text);
            }
        }
        lanes
    }

    pub fn furniture_reach(&self) -> Units {
        let ending = if self.endings {
            ENDING_RISE
                + Metrics::bravura()
                    .engraving_defaults()
                    .repeat_ending_line_thickness
        } else {
            Units::ZERO
        };
        let count = if self.count {
            COUNT_RISE + ascent(COUNT_EM)
        } else {
            Units::ZERO
        };
        ending.max(count)
    }

    pub fn apply(&self, specs: &[StaffSpec], needs: &mut VerticalNeeds) {
        for (row, band) in needs.bands.iter_mut().enumerate() {
            band.lanes = band.lanes.union(self.lanes(row));
            if row == 0 {
                band.near = band.near.max(self.furniture_reach());
            }
        }
        for (row, (marks, spec)) in self.rows.iter().zip(specs).enumerate() {
            if marks.dynamics.is_empty() {
                continue;
            }
            let bottom = marks.floor + DYNAMIC_GAP + dynamic_height() + NOTATION_CLEARANCE;
            let extra = (bottom - spec.shape.height() - below_share(specs, row)).max(Units::ZERO);
            if let Some(slot) = needs.below_staff.get_mut(row) {
                *slot = (*slot).max(extra);
            }
        }
    }
}
