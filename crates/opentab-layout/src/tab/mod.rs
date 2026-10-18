mod bend;
mod draw;
mod effects;
mod label;
mod rhythm;
mod runs;

use opentab_model::{
    Bar, Beat, BeatId, NoteId, NoteKind, Slide, StrokeDirection, TimeSignature, Track, Vibrato,
};

pub use bend::{BEND_RISE, BendDraw, BendSegment, BendShape, Heading, plan_bends, shape_of};
pub use draw::{TabRow, draw_bar};
use effects::HAMMER_EM;
pub use label::{
    FINGER_EM, FRET_EM, LABEL_PAD, MARK_EM, baseline_below_centre, bend_text, digit_height,
    finger_text, fret_text, half_head, label_box, measure,
};
pub use rhythm::{RHYTHM_GAP, TabRhythm, tab_staff, tremolo_glyph};
pub use runs::{LinkAnchor, RowEnds, RowParts, finish_row};

use crate::grid::{GridColumn, ticks};
use crate::lanes::{Band, Lane, LaneSet};
use crate::notation::{ColumnNeed, Ink, drawable, offset_rect};
use crate::rhythm::{CUE_SIZE, scale};
use crate::structure::BeatSlot;
use crate::units::{Point, Rect, STAFF_SPACE, Units};

pub const VOICE_GAP: Units = Units(STAFF_SPACE.0 / 5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    Hammer,
    Slide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    pub kind: LinkKind,
    pub from: u8,
    pub to: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FretPlan {
    pub id: NoteId,
    pub line: usize,
    pub y: Units,
    pub dx: Units,
    pub text: String,
    pub width: Units,
    pub em: Units,
    pub tie_origin: bool,
    pub tie_destination: bool,
    pub link: Option<Link>,
    pub incoming: Option<Link>,
    pub slide: Option<Slide>,
    pub bend: Option<BendShape>,
    pub finger: Option<&'static str>,
    pub merged: bool,
}

impl FretPlan {
    pub fn label(&self) -> Rect {
        offset_rect(
            label_box(self.width, self.em),
            Point {
                x: self.dx,
                y: self.y,
            },
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabBeat {
    pub id: BeatId,
    pub slot: BeatSlot,
    pub voice_slot: u8,
    pub size: Units,
    pub centre: Units,
    pub frets: Vec<FretPlan>,
    pub mark: Option<String>,
    pub stroke: Option<StrokeDirection>,
    pub bends: Vec<BendDraw>,
    pub ink: Option<Ink>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ColumnMarks {
    pub let_ring: bool,
    pub palm_mute: bool,
    pub vibrato: Option<Vibrato>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabNotation {
    pub beats: Vec<TabBeat>,
    pub columns: Vec<ColumnNeed>,
    pub marks: Vec<ColumnMarks>,
    pub band: Band,
    pub bottom: Units,
    pub height: Units,
    pub rhythm: Option<TabRhythm>,
    pub alone: bool,
}

pub struct TabInput<'a> {
    pub track: &'a Track,
    pub bar: Option<&'a Bar>,
    pub previous: Option<&'a Bar>,
    pub next: Option<&'a Bar>,
    pub slots: &'a [BeatSlot],
    pub columns: &'a [GridColumn],
    pub per_whole: i128,
    pub time: TimeSignature,
    pub lines: usize,
    pub spacing: Units,
    pub alone: bool,
}

fn line_y(line: usize, spacing: Units) -> Units {
    Units(
        spacing
            .0
            .saturating_mul(i64::try_from(line).unwrap_or(i64::MAX)),
    )
}

fn following<'a>(input: &TabInput<'a>, slot: &BeatSlot) -> Option<&'a Beat> {
    let bar = input.bar?;
    let voice = bar.voices.get(slot.voice)?;
    if let Some(beat) = voice.beats.get(slot.beat + 1) {
        return Some(beat);
    }
    input
        .next?
        .voices
        .iter()
        .find(|candidate| candidate.slot == voice.slot)?
        .beats
        .first()
}

fn preceding<'a>(input: &TabInput<'a>, slot: &BeatSlot) -> Option<&'a Beat> {
    let bar = input.bar?;
    let voice = bar.voices.get(slot.voice)?;
    if let Some(before) = slot.beat.checked_sub(1) {
        return voice.beats.get(before);
    }
    input
        .previous?
        .voices
        .iter()
        .find(|candidate| candidate.slot == voice.slot)?
        .beats
        .last()
}

fn link_kind(effects: &opentab_model::NoteEffects) -> Option<LinkKind> {
    if effects.hammer_on_pull_off {
        Some(LinkKind::Hammer)
    } else if matches!(effects.slide, Some(Slide::Shift | Slide::Legato)) {
        Some(LinkKind::Slide)
    } else {
        None
    }
}

fn incoming_of(input: &TabInput<'_>, slot: &BeatSlot, string: u8, fret: u8) -> Option<Link> {
    preceding(input, slot)?
        .notes
        .iter()
        .find_map(|note| match note.kind {
            NoteKind::Fretted {
                string: other,
                fret: from,
            } if other == string => link_kind(&note.effects).map(|kind| Link {
                kind,
                from,
                to: Some(fret),
            }),
            _ => None,
        })
}

fn link_of(input: &TabInput<'_>, slot: &BeatSlot, string: u8, fret: u8, kind: LinkKind) -> Link {
    let to = following(input, slot).and_then(|beat| {
        beat.notes.iter().find_map(|note| match note.kind {
            NoteKind::Fretted {
                string: other,
                fret,
            } if other == string => Some(fret),
            _ => None,
        })
    });
    Link {
        kind,
        from: fret,
        to,
    }
}

fn fret_plans(input: &TabInput<'_>, slot: &BeatSlot, beat: &Beat, size: Units) -> Vec<FretPlan> {
    let em = scale(FRET_EM, size);
    let mut frets = Vec::new();
    for note in &beat.notes {
        let NoteKind::Fretted { string, fret } = note.kind else {
            continue;
        };
        let line = usize::from(string);
        if line >= input.lines {
            continue;
        }
        let Some(text) = fret_text(note) else {
            continue;
        };
        let effects = &note.effects;
        let link = link_kind(effects).map(|kind| link_of(input, slot, string, fret, kind));
        let incoming = incoming_of(input, slot, string, fret);
        let slide = effects.slide.filter(|kind| {
            matches!(
                kind,
                Slide::IntoFromBelow
                    | Slide::IntoFromAbove
                    | Slide::OutDownwards
                    | Slide::OutUpwards
            )
        });
        frets.push(FretPlan {
            id: note.id,
            line,
            y: line_y(line, input.spacing),
            dx: Units::ZERO,
            merged: false,
            width: measure(&text, em),
            text,
            em,
            tie_origin: effects.tie_origin,
            tie_destination: effects.tie_destination,
            link,
            incoming,
            slide,
            bend: effects
                .bend
                .as_ref()
                .and_then(|bend| shape_of(bend, effects.tie_destination)),
            finger: if input.alone {
                effects.left_hand_finger.map(finger_text)
            } else {
                None
            },
        });
    }
    frets.sort_by_key(|fret| fret.line);
    frets
}

fn mark_of(beat: &Beat) -> Option<String> {
    let mut marks: Vec<&'static str> = Vec::new();
    for note in &beat.notes {
        let found = [
            note.effects.tap.map(label::tap_mark),
            note.effects
                .harmonic
                .and_then(|harmonic| label::harmonic_mark(harmonic.kind)),
        ];
        for mark in found.into_iter().flatten() {
            if !marks.contains(&mark) {
                marks.push(mark);
            }
        }
    }
    (!marks.is_empty()).then(|| marks.join(" "))
}

fn new_beat(input: &TabInput<'_>, slot: &BeatSlot, voice_slot: u8, beat: &Beat) -> TabBeat {
    let size = if slot.grace.is_some() {
        CUE_SIZE
    } else {
        STAFF_SPACE
    };
    TabBeat {
        id: beat.id,
        slot: *slot,
        voice_slot,
        size,
        centre: half_head(size),
        frets: fret_plans(input, slot, beat, size),
        mark: if slot.grace.is_none() {
            mark_of(beat)
        } else {
            None
        },
        stroke: if slot.grace.is_none() {
            beat.effects.stroke.map(|stroke| stroke.direction)
        } else {
            None
        },
        bends: Vec::new(),
        ink: None,
    }
}

fn merge_marks(beats: &mut [TabBeat], columns: usize) {
    for column in 0..columns {
        let mut words: Vec<String> = Vec::new();
        let mut holder: Option<usize> = None;
        for (index, beat) in beats.iter_mut().enumerate() {
            if beat.slot.column != column || beat.slot.grace.is_some() {
                continue;
            }
            let Some(mark) = beat.mark.take() else {
                continue;
            };
            holder.get_or_insert(index);
            for word in mark.split(' ') {
                if !words.iter().any(|known| known == word) {
                    words.push(word.to_string());
                }
            }
        }
        if let Some(beat) = holder.and_then(|index| beats.get_mut(index)) {
            beat.mark = Some(words.join(" "));
        }
    }
}

fn reserve_lane_labels(columns: &mut [ColumnNeed], marks: &[ColumnMarks]) {
    for kind in runs::text_lanes() {
        let width = measure(kind.label, MARK_EM) + runs::LANE_TEXT_GAP;
        let mut before = false;
        for (need, mark) in columns.iter_mut().zip(marks) {
            let flagged = (kind.flag)(mark);
            if flagged && !before {
                need.right = need.right.max(width);
            }
            before = flagged;
        }
    }
}

fn separate_voices(beats: &mut [TabBeat], columns: usize) {
    for column in 0..columns {
        let mut placed: Vec<Rect> = Vec::new();
        let mut shown: Vec<(usize, String)> = Vec::new();
        for beat in beats
            .iter_mut()
            .filter(|beat| beat.slot.column == column && beat.slot.grace.is_none())
        {
            for fret in &mut beat.frets {
                if shown
                    .iter()
                    .any(|(line, text)| *line == fret.line && *text == fret.text)
                {
                    fret.merged = true;
                    continue;
                }
                shown.push((fret.line, fret.text.clone()));
                while let Some(blocker) = placed
                    .iter()
                    .find(|rect| rect.overlaps(&fret.label()))
                    .copied()
                {
                    fret.dx += blocker.max.x - fret.label().min.x + VOICE_GAP;
                }
                placed.push(fret.label());
            }
        }
    }
}

fn column_marks(input: &TabInput<'_>, bar: &Bar) -> Vec<ColumnMarks> {
    let mut marks = vec![ColumnMarks::default(); input.columns.len()];
    for slot in input.slots.iter().filter(|slot| slot.grace.is_none()) {
        let Some(beat) = bar
            .voices
            .get(slot.voice)
            .and_then(|voice| voice.beats.get(slot.beat))
        else {
            continue;
        };
        let Some(onset) = input.columns.get(slot.column).map(|column| column.ticks) else {
            continue;
        };
        let length = ticks(beat.duration.length(), input.per_whole).unwrap_or(0);
        let end = onset.saturating_add(length.max(1));
        let let_ring = beat.notes.iter().any(|note| note.effects.let_ring);
        let palm_mute = beat.notes.iter().any(|note| note.effects.palm_mute);
        let vibrato = beat
            .notes
            .iter()
            .filter_map(|note| note.effects.vibrato)
            .max_by_key(|kind| matches!(kind, Vibrato::Wide));
        for (column, mark) in input.columns.iter().zip(marks.iter_mut()) {
            if column.ticks < onset || column.ticks >= end {
                continue;
            }
            mark.let_ring |= let_ring;
            mark.palm_mute |= palm_mute;
            mark.vibrato = match (mark.vibrato, vibrato) {
                (Some(Vibrato::Wide), _) | (_, Some(Vibrato::Wide)) => Some(Vibrato::Wide),
                (known, None) => known,
                (_, found) => found,
            };
        }
    }
    marks
}

fn lanes_of(beats: &[TabBeat], marks: &[ColumnMarks]) -> LaneSet {
    let mut lanes = LaneSet::EMPTY;
    if beats.iter().any(|beat| beat.mark.is_some()) {
        lanes = lanes.with(Lane::Marks);
    }
    if marks.iter().any(|mark| mark.vibrato.is_some()) {
        lanes = lanes.with(Lane::Vibrato);
    }
    if marks.iter().any(|mark| mark.let_ring) {
        lanes = lanes.with(Lane::LetRing);
    }
    if marks.iter().any(|mark| mark.palm_mute) {
        lanes = lanes.with(Lane::PalmMute);
    }
    lanes
}

pub fn analyse_tab(input: &TabInput<'_>) -> Option<TabNotation> {
    if !drawable(input.track) {
        return None;
    }
    let bar = input.bar?;
    let mut beats: Vec<TabBeat> = Vec::new();
    for slot in input.slots {
        let Some(voice) = bar.voices.get(slot.voice) else {
            continue;
        };
        let Some(beat) = voice.beats.get(slot.beat) else {
            continue;
        };
        beats.push(new_beat(input, slot, voice.slot.get(), beat));
    }
    merge_marks(&mut beats, input.columns.len());
    separate_voices(&mut beats, input.columns.len());
    let height = line_y(input.lines.saturating_sub(1), input.spacing);
    for beat in &mut beats {
        beat.bends = plan_bends(&beat.frets);
        beat.ink = effects::beat_ink(beat, input.alone);
    }
    let rhythm = if input.alone {
        rhythm::analyse(input, bar, &beats, height)
    } else {
        None
    };
    let mut columns = vec![ColumnNeed::default(); input.columns.len()];
    let mut top = Units::ZERO;
    let mut bottom = height;
    for (index, beat) in beats.iter().enumerate() {
        let mut ink = beat.ink;
        if let Some(extra) = rhythm.as_ref().and_then(|plan| plan.ink(index)) {
            match ink.as_mut() {
                Some(known) => known.add(extra),
                None => ink = Some(Ink::of(extra)),
            }
        }
        let Some(ink) = ink else {
            continue;
        };
        top = top.min(ink.top);
        bottom = bottom.max(ink.bottom);
        let Some(need) = columns.get_mut(beat.slot.column) else {
            continue;
        };
        if beat.slot.grace.is_some() {
            need.grace_lead = need.grace_lead.max(-ink.left);
            need.grace_right = need.grace_right.max(ink.right);
        } else {
            need.lead = need.lead.max(-ink.left);
            need.right = need.right.max(ink.right);
        }
    }
    if let Some(reach) = rhythm.as_ref().and_then(TabRhythm::reach) {
        bottom = bottom.max(reach);
    }
    let marks = column_marks(input, bar);
    reserve_lane_labels(&mut columns, &marks);
    let lanes = lanes_of(&beats, &marks);
    Some(TabNotation {
        beats,
        columns,
        marks,
        band: Band { near: -top, lanes },
        bottom,
        height,
        rhythm,
        alone: input.alone,
    })
}
