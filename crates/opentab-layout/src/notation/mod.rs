mod guitar;
mod heads;
mod marks;

use opentab_model::{
    Bar, BeatId, Duration, Grace, InstrumentKind, KeySignature, Note, NoteId, NoteValue,
    TimeSignature, Track, TremoloPicking,
};

pub use guitar::{finger_rect, guitar_rects};
pub use heads::{head_glyph, notehead_glyphs, parenthesis_origins};

use crate::drums::DrumStaffKit;
use crate::glyphs::Glyph;
use crate::grid::GridColumn;
use crate::pitch::Spelling;
use crate::rhythm::{
    BeamSlope, Extremes, RhythmEvent, RhythmGroups, RhythmPlan, RhythmStaff, StemDirection,
    StemPolicy, directions, fit_stems, glyph_box, group,
};
use crate::structure::{BeatSlot, ClefSign};
use crate::units::{Point, Rect, STAFF_SPACE, Units};

pub const HALF_SPACE: Units = Units(STAFF_SPACE.0 / 2);
pub const STAFF_HEIGHT: Units = Units(STAFF_SPACE.0 * 4);
pub const MIDDLE_LINE: Units = Units(STAFF_SPACE.0 * 2);
pub const ACCIDENTAL_GAP: Units = Units(STAFF_SPACE.0 / 5);
pub const ACCIDENTAL_COLUMN_GAP: Units = Units(STAFF_SPACE.0 / 8);
pub const PARENTHESIS_GAP: Units = Units(STAFF_SPACE.0 / 10);
pub const GRACE_GAP: Units = Units(STAFF_SPACE.0 / 4);
pub const ARTICULATION_GAP: Units = Units(STAFF_SPACE.0 / 2);
pub const REST_SHIFT: Units = Units(STAFF_SPACE.0 * 2);
pub const LONG_REST_SHIFT: Units = STAFF_SPACE;
pub const TIE_OFFSET: Units = Units(STAFF_SPACE.0 * 2 / 5);
pub const TIE_HEIGHT: Units = STAFF_SPACE;
pub const DOT_HALF_HEIGHT: Units = Units(STAFF_SPACE.0 / 5);
pub const REST_CLEARANCE: Units = Units(STAFF_SPACE.0 / 4);
pub const REST_ATTEMPTS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccidentalMark {
    pub glyph: Glyph,
    pub dx: Units,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadPlan {
    pub id: NoteId,
    pub spelling: Spelling,
    pub position: i64,
    pub y: Units,
    pub glyph: Glyph,
    pub width: Units,
    pub dx: Units,
    pub accidental: Option<AccidentalMark>,
    pub parentheses: bool,
    pub tie_origin: bool,
    pub tie_destination: bool,
    pub tie_up: bool,
    pub string: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestPlan {
    pub glyph: Glyph,
    pub y: Units,
    pub centred: bool,
    pub width: Units,
    pub away: Option<StemDirection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedgerPlan {
    pub y: Units,
    pub left: Units,
    pub right: Units,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArticulationPlan {
    pub glyph: Glyph,
    pub dx: Units,
    pub y: Units,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FingerPlan {
    pub text: &'static str,
    pub right: Units,
    pub baseline: Units,
    pub width: Units,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Ink {
    pub left: Units,
    pub right: Units,
    pub top: Units,
    pub bottom: Units,
}

impl Ink {
    pub fn add(&mut self, rect: Rect) {
        self.left = self.left.min(rect.min.x);
        self.right = self.right.max(rect.max.x);
        self.top = self.top.min(rect.min.y);
        self.bottom = self.bottom.max(rect.max.y);
    }

    pub fn of(rect: Rect) -> Ink {
        Ink {
            left: rect.min.x,
            right: rect.max.x,
            top: rect.min.y,
            bottom: rect.max.y,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeatPlan {
    pub id: BeatId,
    pub slot: BeatSlot,
    pub voice_slot: u8,
    pub value: NoteValue,
    pub dots: u8,
    pub size: Units,
    pub grace: Option<Grace>,
    pub direction: StemDirection,
    pub offset: Units,
    pub heads: Vec<HeadPlan>,
    pub rest: Option<RestPlan>,
    pub dot_x: Units,
    pub dot_ys: Vec<Units>,
    pub ledgers: Vec<LedgerPlan>,
    pub articulations: Vec<ArticulationPlan>,
    pub tremolo: Option<ArticulationPlan>,
    pub fingers: Vec<FingerPlan>,
    pub ink: Option<Ink>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ColumnNeed {
    pub lead: Units,
    pub right: Units,
    pub grace_lead: Units,
    pub grace_right: Units,
}

impl ColumnNeed {
    pub fn grace_advance(&self) -> Units {
        self.grace_lead + self.grace_right + GRACE_GAP
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaffNotation {
    pub beats: Vec<BeatPlan>,
    pub events: Vec<RhythmEvent>,
    pub groups: RhythmGroups,
    pub rhythm: RhythmPlan,
    pub columns: Vec<ColumnNeed>,
    pub top: Units,
    pub bottom: Units,
}

pub struct RowInput<'a> {
    pub track: &'a Track,
    pub bar: Option<&'a Bar>,
    pub slots: &'a [BeatSlot],
    pub columns: &'a [GridColumn],
    pub per_whole: i128,
    pub clef: ClefSign,
    pub key: KeySignature,
    pub time: TimeSignature,
    pub kit: Option<&'a DrumStaffKit>,
}

pub fn staff_geometry(size: Units, policy: StemPolicy) -> RhythmStaff {
    RhythmStaff {
        reference: MIDDLE_LINE,
        spacing: STAFF_SPACE,
        policy,
        size,
        reach_reference: size == STAFF_SPACE,
        slope: BeamSlope::Follow,
    }
}

pub fn drawable(track: &Track) -> bool {
    track.instrument.kind == InstrumentKind::Fretted
}

pub fn offset_rect(rect: Rect, by: Point) -> Rect {
    Rect::new(rect.min.offset(by), rect.max.offset(by))
}

fn voice_direction(voice_slot: u8) -> StemDirection {
    if voice_slot.is_multiple_of(2) {
        StemDirection::Up
    } else {
        StemDirection::Down
    }
}

fn members(beats: &[BeatPlan], column: usize, grace: bool) -> Vec<usize> {
    (0..beats.len())
        .filter(|index| {
            let beat = &beats[*index];
            beat.slot.column == column && beat.slot.grace.is_some() == grace
        })
        .collect()
}

fn voices_with(beats: &[BeatPlan], wanted: impl Fn(&BeatPlan) -> bool) -> usize {
    let mut voices: Vec<usize> = beats
        .iter()
        .filter(|beat| beat.slot.grace.is_none() && wanted(beat))
        .map(|beat| beat.slot.voice)
        .collect();
    voices.sort_unstable();
    voices.dedup();
    voices.len()
}

fn widen(reach: &mut Extremes, top: Units, bottom: Units) {
    reach.top = reach.top.min(top);
    reach.bottom = reach.bottom.max(bottom);
}

pub fn analyse(input: &RowInput<'_>) -> Option<StaffNotation> {
    if !drawable(input.track) && input.kit.is_none() {
        return None;
    }
    let bar = input.bar?;
    let mut beats: Vec<BeatPlan> = Vec::new();
    let mut notes: Vec<&[Note]> = Vec::new();
    let mut lengths: Vec<Duration> = Vec::new();
    let mut tremolos: Vec<Option<TremoloPicking>> = Vec::new();
    for slot in input.slots {
        let Some(voice) = bar.voices.get(slot.voice) else {
            continue;
        };
        let Some(beat) = voice.beats.get(slot.beat) else {
            continue;
        };
        beats.push(heads::new_beat(input, slot, voice.slot.get(), beat));
        notes.push(&beat.notes);
        lengths.push(beat.duration);
        tremolos.push(beat.effects.tremolo_picking);
    }
    if input.kit.is_none() {
        heads::mark_accidentals(&mut beats, input.key);
    }
    let with_notes = voices_with(&beats, |beat| !beat.heads.is_empty());
    let with_content = voices_with(&beats, |_| true);
    let staff = staff_geometry(STAFF_SPACE, StemPolicy::AwayFromReference);
    let onset = |beat: &BeatPlan| {
        input
            .columns
            .get(beat.slot.column)
            .map_or(0, |column| column.ticks)
    };
    let mut events: Vec<RhythmEvent> = beats
        .iter()
        .zip(&lengths)
        .map(|(beat, length)| heads::event(beat, onset(beat), *length))
        .collect();
    let groups = group(input.time, input.per_whole, &events);
    let chosen = directions(&staff, &events, &groups);
    for (beat, direction) in beats.iter_mut().zip(chosen) {
        let timed = beat.slot.grace.is_none();
        let shared = if beat.heads.is_empty() {
            with_content >= 2
        } else {
            with_notes >= 2
        };
        beat.direction = if timed && shared {
            voice_direction(beat.voice_slot)
        } else if input.kit.is_some() {
            StemDirection::Up
        } else {
            direction
        };
        heads::displace(beat);
        beat.ledgers = heads::ledgers(beat);
    }
    let anchors: Vec<_> = beats
        .iter()
        .zip(&events)
        .map(|(beat, event)| heads::stem_anchor(beat, &staff.for_event(event)))
        .collect();
    let mut rhythm = fit_stems(&staff, &events, &anchors, &groups);
    let mut whole_rests = Vec::new();
    let mut voices: Vec<usize> = beats.iter().map(|beat| beat.slot.voice).collect();
    voices.sort_unstable();
    voices.dedup();
    for voice in voices {
        let timed: Vec<usize> = (0..beats.len())
            .filter(|index| beats[*index].slot.voice == voice && beats[*index].slot.grace.is_none())
            .collect();
        if let [only] = timed.as_slice()
            && notes[*only].is_empty()
            && lengths[*only].length() >= input.time.bar_length()
        {
            whole_rests.push(*only);
        }
    }
    for index in 0..beats.len() {
        if beats[index].heads.is_empty() && notes[index].is_empty() {
            let away = (with_content >= 2).then_some(beats[index].direction);
            let plan = marks::rest_plan(&beats[index], &staff, whole_rests.contains(&index), away);
            beats[index].rest = Some(plan);
        }
    }
    for column in 0..input.columns.len() {
        let timed = members(&beats, column, false);
        heads::place_voices(&mut beats, &timed, &rhythm, &events, &staff);
        heads::stack_accidentals(&mut beats, &timed, STAFF_SPACE);
        marks::place_dots(&mut beats, &timed, &staff, with_notes >= 2);
        for grace in members(&beats, column, true) {
            heads::stack_accidentals(&mut beats, &[grace], staff.cue().size);
            marks::place_dots(&mut beats, &[grace], &staff.cue(), false);
        }
    }
    marks::clear_rests(
        &mut beats,
        &marks::RestContext {
            plan: &rhythm,
            events: &events,
            staff: &staff,
        },
    );
    for (index, beat) in beats.iter_mut().enumerate() {
        marks::finish_rest(beat, &staff);
        if beat.slot.grace.is_none() {
            let end = rhythm.stem(index).map(|fitted| fitted.end);
            beat.articulations = marks::articulations(beat, notes[index], end, with_notes >= 2);
            beat.tremolo = guitar::tremolo(beat, tremolos[index], rhythm.stem(index));
            beat.fingers = guitar::fingers(beat, notes[index]);
        }
        let event = &mut events[index];
        let geometry = staff.for_event(event);
        let stem = rhythm.stem_rect(&staff, event, index);
        beat.ink = marks::beat_ink(beat, stem, &geometry);
        event.left = beat
            .heads
            .iter()
            .map(|head| head.dx)
            .min()
            .unwrap_or_default();
        event.right = beat
            .heads
            .iter()
            .map(|head| head.dx + head.width)
            .max()
            .or(beat.rest.map(|rest| rest.width))
            .unwrap_or_default();
    }
    let inks: Vec<Option<Extremes>> = beats
        .iter()
        .map(|beat| {
            beat.ink.map(|ink| Extremes {
                top: ink.top,
                bottom: ink.bottom,
            })
        })
        .collect();
    rhythm.place_tuplets(&staff, &groups, &inks);
    let mut reach = Extremes {
        top: Units::ZERO,
        bottom: STAFF_HEIGHT,
    };
    for beat in &beats {
        if let Some(ink) = beat.ink {
            widen(&mut reach, ink.top, ink.bottom);
        }
        if let Some(rest) = &beat.rest
            && rest.centred
        {
            let rect = glyph_box(rest.glyph, beat.size);
            widen(&mut reach, rest.y + rect.min.y, rest.y + rect.max.y);
        }
    }
    if let Some(extent) = rhythm.extent(&staff, &events) {
        widen(&mut reach, extent.top, extent.bottom);
    }
    let mut columns = vec![ColumnNeed::default(); input.columns.len()];
    for (column, need) in columns.iter_mut().enumerate() {
        for beat in beats.iter().filter(|beat| beat.slot.column == column) {
            let Some(ink) = beat.ink else {
                continue;
            };
            if beat.rest.is_some_and(|rest| rest.centred) {
                continue;
            }
            if beat.slot.grace.is_some() {
                need.grace_lead = need.grace_lead.max(-ink.left);
                need.grace_right = need.grace_right.max(ink.right);
            } else {
                need.lead = need.lead.max(-(beat.offset + ink.left));
                need.right = need.right.max(beat.offset + ink.right);
            }
        }
    }
    Some(StaffNotation {
        beats,
        events,
        groups,
        rhythm,
        columns,
        top: reach.top,
        bottom: reach.bottom,
    })
}
