use opentab_model::{
    Accent, Bar, Beat, BeatId, Grace, InstrumentKind, KeySignature, Note, NoteId, NoteKind,
    NoteValue, TimeSignature, Track,
};

use crate::accidentals::Carry;
use crate::glyphs::{
    ARTIC_ACCENT_ABOVE, ARTIC_ACCENT_BELOW, ARTIC_MARCATO_ABOVE, ARTIC_MARCATO_BELOW,
    ARTIC_STACCATO_ABOVE, ARTIC_STACCATO_BELOW, Anchor, FLAG_8TH_UP, Glyph, Metrics,
    NOTEHEAD_BLACK, NOTEHEAD_DIAMOND_BLACK, NOTEHEAD_DIAMOND_HALF, NOTEHEAD_DIAMOND_WHOLE,
    NOTEHEAD_HALF, NOTEHEAD_PARENTHESIS_LEFT, NOTEHEAD_PARENTHESIS_RIGHT, NOTEHEAD_WHOLE,
    NOTEHEAD_X_BLACK, NOTEHEAD_X_HALF, NOTEHEAD_X_WHOLE, REST_WHOLE,
};
use crate::grid::GridColumn;
use crate::pitch::{Spelling, position, spell, written_midi};
use crate::rhythm::{
    BeamLine, CUE_SIZE, Extremes, RhythmStaff, Stem, StemDirection, StemPolicy, Timing,
    TupletBracket, anchor, beam_groups, flags_of, glyph_box, rest_glyph, scale, stemmed,
    tuplet_runs,
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
pub const REST_ATTEMPTS: usize = 12;

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
pub struct StemPlan {
    pub dx: Units,
    pub base: Units,
    pub tip: Units,
    pub direction: StemDirection,
    pub beams: u8,
    pub end: Units,
    pub beamed: bool,
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
    pub stem: Option<StemPlan>,
    pub dot_x: Units,
    pub dot_ys: Vec<Units>,
    pub ledgers: Vec<LedgerPlan>,
    pub articulations: Vec<ArticulationPlan>,
    pub slash: bool,
    pub ink: Option<Ink>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeamPlan {
    pub beats: Vec<usize>,
    pub line: BeamLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TupletPlan {
    pub first: usize,
    pub last: usize,
    pub number: u8,
    pub direction: StemDirection,
    pub edge: Units,
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
    pub beams: Vec<BeamPlan>,
    pub tuplets: Vec<TupletPlan>,
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
}

pub fn staff_geometry(size: Units, policy: StemPolicy) -> RhythmStaff {
    RhythmStaff {
        reference: MIDDLE_LINE,
        spacing: STAFF_SPACE,
        policy,
        size,
        reach_reference: size == STAFF_SPACE,
    }
}

pub fn head_glyph(value: NoteValue, note: &Note) -> Glyph {
    let effects = &note.effects;
    match (value, effects.dead, effects.harmonic.is_some()) {
        (NoteValue::Whole, true, _) => NOTEHEAD_X_WHOLE,
        (NoteValue::Half, true, _) => NOTEHEAD_X_HALF,
        (_, true, _) => NOTEHEAD_X_BLACK,
        (NoteValue::Whole, false, true) => NOTEHEAD_DIAMOND_WHOLE,
        (NoteValue::Half, false, true) => NOTEHEAD_DIAMOND_HALF,
        (_, false, true) => NOTEHEAD_DIAMOND_BLACK,
        (NoteValue::Whole, false, false) => NOTEHEAD_WHOLE,
        (NoteValue::Half, false, false) => NOTEHEAD_HALF,
        (_, false, false) => NOTEHEAD_BLACK,
    }
}

pub fn notehead_glyphs() -> [Glyph; 9] {
    [
        NOTEHEAD_WHOLE,
        NOTEHEAD_HALF,
        NOTEHEAD_BLACK,
        NOTEHEAD_X_WHOLE,
        NOTEHEAD_X_HALF,
        NOTEHEAD_X_BLACK,
        NOTEHEAD_DIAMOND_WHOLE,
        NOTEHEAD_DIAMOND_HALF,
        NOTEHEAD_DIAMOND_BLACK,
    ]
}

pub fn drawable(track: &Track) -> bool {
    track.instrument.kind == InstrumentKind::Fretted
}

fn units(value: i64) -> Units {
    Units(HALF_SPACE.0.saturating_mul(value))
}

fn offset_rect(rect: Rect, by: Point) -> Rect {
    Rect::new(rect.min.offset(by), rect.max.offset(by))
}

fn head(
    track: &Track,
    note: &Note,
    value: NoteValue,
    input: &RowInput<'_>,
    size: Units,
) -> Option<HeadPlan> {
    let midi = written_midi(track, note)?;
    let spelling = spell(midi, input.key);
    let position = position(input.clef, spelling.step);
    let glyph = head_glyph(value, note);
    let string = match note.kind {
        NoteKind::Fretted { string, .. } => Some(string),
        NoteKind::Percussion { .. } => None,
    };
    Some(HeadPlan {
        id: note.id,
        spelling,
        position,
        y: units(position),
        glyph,
        width: glyph_box(glyph, size).max.x,
        dx: Units::ZERO,
        accidental: None,
        parentheses: note.effects.ghost,
        tie_origin: note.effects.tie_origin,
        tie_destination: note.effects.tie_destination,
        tie_up: false,
        string,
    })
}

fn new_beat(input: &RowInput<'_>, slot: &BeatSlot, voice_slot: u8, beat: &Beat) -> BeatPlan {
    let grace = slot.grace.and(beat.effects.grace);
    let size = if slot.grace.is_some() {
        CUE_SIZE
    } else {
        STAFF_SPACE
    };
    let value = beat.duration.value();
    let heads = beat
        .notes
        .iter()
        .filter_map(|note| head(input.track, note, value, input, size))
        .collect();
    BeatPlan {
        id: beat.id,
        slot: *slot,
        voice_slot,
        value,
        dots: beat.duration.dots(),
        size,
        grace: if slot.grace.is_some() {
            Some(grace.unwrap_or(Grace::BeforeBeat))
        } else {
            None
        },
        direction: StemDirection::Up,
        offset: Units::ZERO,
        heads,
        rest: None,
        stem: None,
        dot_x: Units::ZERO,
        dot_ys: Vec::new(),
        ledgers: Vec::new(),
        articulations: Vec::new(),
        slash: grace == Some(Grace::BeforeBeat),
        ink: None,
    }
}

fn order_key(beat: &BeatPlan) -> (usize, usize, usize, usize, usize) {
    match beat.slot.grace {
        Some(grace) => (
            beat.slot.column,
            0,
            grace.order,
            beat.slot.voice,
            beat.slot.beat,
        ),
        None => (beat.slot.column, 1, 0, beat.slot.voice, beat.slot.beat),
    }
}

fn mark_accidentals(beats: &mut [BeatPlan], key: KeySignature) {
    let mut order: Vec<usize> = (0..beats.len()).collect();
    order.sort_by_key(|index| order_key(&beats[*index]));
    let mut carry = Carry::new(key);
    for index in order {
        let beat = &mut beats[index];
        let mut heads: Vec<usize> = (0..beat.heads.len()).collect();
        heads.sort_by_key(|head| (beat.heads[*head].position, *head));
        for head in heads {
            let plan = &mut beat.heads[head];
            plan.accidental = carry
                .mark(plan.spelling, plan.tie_destination)
                .map(|accidental| AccidentalMark {
                    glyph: accidental.glyph(),
                    dx: Units::ZERO,
                });
        }
    }
}

fn extremes(beat: &BeatPlan) -> Option<Extremes> {
    let top = beat.heads.iter().map(|head| head.y).min()?;
    let bottom = beat.heads.iter().map(|head| head.y).max()?;
    Some(Extremes { top, bottom })
}

fn displace(beat: &mut BeatPlan) {
    let width = beat
        .heads
        .iter()
        .map(|head| head.width)
        .max()
        .unwrap_or_default();
    beat.heads.sort_by_key(|head| head.position);
    let up = beat.direction == StemDirection::Up;
    let count = beat.heads.len();
    let mut previous: Option<(i64, bool)> = None;
    for step in 0..count {
        let index = if up { count - 1 - step } else { step };
        let head = &mut beat.heads[index];
        let displaced = match previous {
            Some((position, false)) => (head.position - position).abs() <= 1,
            _ => false,
        };
        head.dx = match (displaced, up) {
            (false, _) => Units::ZERO,
            (true, true) => width,
            (true, false) => -width,
        };
        previous = Some((head.position, displaced));
    }
    let middle = beat.heads.len() / 2;
    let chord = beat.heads.len() > 1;
    for (index, head) in beat.heads.iter_mut().enumerate() {
        head.tie_up = if chord {
            index < middle
                || (index == middle && beat.direction == StemDirection::Down && count % 2 == 1)
        } else {
            beat.direction == StemDirection::Down
        };
    }
}

fn parenthesis_boxes(head: &HeadPlan, size: Units) -> (Rect, Rect) {
    let left = glyph_box(NOTEHEAD_PARENTHESIS_LEFT, size);
    let right = glyph_box(NOTEHEAD_PARENTHESIS_RIGHT, size);
    let gap = scale(PARENTHESIS_GAP, size);
    let left_origin = Point {
        x: head.dx - gap - left.max.x,
        y: head.y,
    };
    let right_origin = Point {
        x: head.dx + head.width + gap - right.min.x,
        y: head.y,
    };
    (
        offset_rect(left, left_origin),
        offset_rect(right, right_origin),
    )
}

pub fn parenthesis_origins(head: &HeadPlan, size: Units) -> (Point, Point) {
    let left = glyph_box(NOTEHEAD_PARENTHESIS_LEFT, size);
    let right = glyph_box(NOTEHEAD_PARENTHESIS_RIGHT, size);
    let gap = scale(PARENTHESIS_GAP, size);
    (
        Point {
            x: head.dx - gap - left.max.x,
            y: head.y,
        },
        Point {
            x: head.dx + head.width + gap - right.min.x,
            y: head.y,
        },
    )
}

fn heads_ink(beat: &BeatPlan) -> Option<Ink> {
    let mut ink: Option<Ink> = None;
    for head in &beat.heads {
        let rect = offset_rect(
            glyph_box(head.glyph, beat.size),
            Point {
                x: head.dx,
                y: head.y,
            },
        );
        let mut rects = vec![rect];
        if head.parentheses {
            let (left, right) = parenthesis_boxes(head, beat.size);
            rects.push(left);
            rects.push(right);
        }
        for rect in rects {
            match ink.as_mut() {
                Some(existing) => existing.add(rect),
                None => ink = Some(Ink::of(rect)),
            }
        }
    }
    ink
}

struct Placed {
    left: Units,
    right: Units,
    top: Units,
    bottom: Units,
}

fn place_voices(beats: &mut [BeatPlan], members: &[usize]) {
    let mut order: Vec<usize> = members
        .iter()
        .copied()
        .filter(|index| !beats[*index].heads.is_empty())
        .collect();
    order.sort_by_key(|index| {
        let beat = &beats[*index];
        (beat.direction == StemDirection::Up, beat.slot.voice)
    });
    let mut placed: Vec<Placed> = Vec::new();
    for index in order {
        let beat = &beats[index];
        let Some(ink) = heads_ink(beat) else {
            continue;
        };
        let top = ink.top;
        let bottom = ink.bottom;
        let mut offset = Units::ZERO;
        loop {
            let next = placed
                .iter()
                .filter(|other| {
                    offset + ink.left < other.right
                        && other.left < offset + ink.right
                        && top < other.bottom
                        && other.top < bottom
                })
                .map(|other| other.right - ink.left)
                .max();
            match next {
                Some(next) if next > offset => offset = next,
                _ => break,
            }
        }
        placed.push(Placed {
            left: offset + ink.left,
            right: offset + ink.right,
            top,
            bottom,
        });
        beats[index].offset = offset;
    }
}

struct Pending {
    beat: usize,
    head: usize,
    glyph: Glyph,
    y: Units,
}

fn stack_accidentals(beats: &mut [BeatPlan], members: &[usize], size: Units) {
    let mut edge: Option<Units> = None;
    let mut pending = Vec::new();
    for index in members {
        let beat = &beats[*index];
        if let Some(ink) = heads_ink(beat) {
            let left = beat.offset + ink.left;
            edge = Some(edge.map_or(left, |known| known.min(left)));
        }
        for (head_index, head) in beat.heads.iter().enumerate() {
            if let Some(mark) = head.accidental {
                pending.push(Pending {
                    beat: *index,
                    head: head_index,
                    glyph: mark.glyph,
                    y: head.y,
                });
            }
        }
    }
    let Some(edge) = edge else {
        return;
    };
    if pending.is_empty() {
        return;
    }
    pending.sort_by_key(|item| (item.y, item.beat, item.head));
    let column = pending
        .iter()
        .map(|item| glyph_box(item.glyph, size).max.x)
        .max()
        .unwrap_or_default()
        + scale(ACCIDENTAL_COLUMN_GAP, size);
    let start = edge - scale(ACCIDENTAL_GAP, size);
    let mut columns: Vec<Vec<(Units, Units)>> = Vec::new();
    for item in pending {
        let rect = glyph_box(item.glyph, size);
        let top = item.y + rect.min.y;
        let bottom = item.y + rect.max.y;
        let free = columns.iter().position(|taken| {
            taken
                .iter()
                .all(|(other_top, other_bottom)| bottom <= *other_top || *other_bottom <= top)
        });
        let slot = match free {
            Some(slot) => slot,
            None => {
                columns.push(Vec::new());
                columns.len() - 1
            }
        };
        columns[slot].push((top, bottom));
        let steps = i64::try_from(slot).unwrap_or(i64::MAX);
        let right = start - Units(column.0.saturating_mul(steps));
        let x = right - rect.max.x;
        let beat = &mut beats[item.beat];
        let offset = beat.offset;
        if let Some(mark) = beat.heads[item.head].accidental.as_mut() {
            mark.dx = x - offset;
        }
    }
}

fn stem_plan(beat: &BeatPlan, staff: &RhythmStaff) -> Option<StemPlan> {
    if beat.heads.is_empty() || !stemmed(beat.value) {
        return None;
    }
    let normal: Vec<&HeadPlan> = beat
        .heads
        .iter()
        .filter(|head| head.dx == Units::ZERO)
        .collect();
    let thickness = staff.stem_thickness();
    let (dx, base, tip) = match beat.direction {
        StemDirection::Up => {
            let lowest = normal.iter().max_by_key(|head| head.y)?;
            let attach = anchor(lowest.glyph, Anchor::StemUpSE, beat.size);
            let attach = if attach == Point::ORIGIN {
                Point {
                    x: lowest.width,
                    y: Units::ZERO,
                }
            } else {
                attach
            };
            let top = beat.heads.iter().map(|head| head.y).min()?;
            (attach.x - thickness.half(), lowest.y + attach.y, top)
        }
        StemDirection::Down => {
            let highest = normal.iter().min_by_key(|head| head.y)?;
            let attach = anchor(highest.glyph, Anchor::StemDownNW, beat.size);
            let bottom = beat.heads.iter().map(|head| head.y).max()?;
            (attach.x + thickness.half(), highest.y + attach.y, bottom)
        }
    };
    let beams = flags_of(beat.value);
    let stem = Stem {
        x: dx,
        base,
        tip,
        direction: beat.direction,
        beams,
    };
    Some(StemPlan {
        dx,
        base,
        tip,
        direction: beat.direction,
        beams,
        end: staff.stem_end(&stem),
        beamed: false,
    })
}

pub fn stem_of(plan: &StemPlan, x: Units) -> Stem {
    Stem {
        x: x + plan.dx,
        base: plan.base,
        tip: plan.tip,
        direction: plan.direction,
        beams: plan.beams,
    }
}

fn ledgers(beat: &BeatPlan) -> Vec<LedgerPlan> {
    let extension = scale(
        Metrics::bravura().engraving_defaults().leger_line_extension,
        beat.size,
    );
    let mut lines = Vec::new();
    let highest = beat
        .heads
        .iter()
        .map(|head| head.position)
        .min()
        .unwrap_or(0);
    let lowest = beat
        .heads
        .iter()
        .map(|head| head.position)
        .max()
        .unwrap_or(0);
    let mut add = |line: i64, covers: &dyn Fn(i64) -> bool| {
        let covered: Vec<&HeadPlan> = beat
            .heads
            .iter()
            .filter(|head| covers(head.position))
            .collect();
        let left = covered.iter().map(|head| head.dx).min();
        let right = covered.iter().map(|head| head.dx + head.width).max();
        if let (Some(left), Some(right)) = (left, right) {
            lines.push(LedgerPlan {
                y: units(line),
                left: left - extension,
                right: right + extension,
            });
        }
    };
    let mut line = -2;
    while line >= highest {
        add(line, &|position| position <= line);
        line -= 2;
    }
    let mut line = 10;
    while line <= lowest {
        add(line, &|position| position >= line);
        line += 2;
    }
    lines
}

fn articulations(beat: &BeatPlan, notes: &[Note], stem_side: bool) -> Vec<ArticulationPlan> {
    let staccato = notes.iter().any(|note| note.effects.staccato);
    let accent = notes
        .iter()
        .filter_map(|note| note.effects.accent)
        .max_by_key(|accent| matches!(accent, Accent::Heavy));
    if beat.heads.is_empty() || (!staccato && accent.is_none()) {
        return Vec::new();
    }
    let below = (beat.direction == StemDirection::Up) != stem_side;
    let reach = beat.stem.filter(|_| stem_side).map(|stem| stem.end);
    let mut wanted = Vec::new();
    if staccato {
        wanted.push(if below {
            ARTIC_STACCATO_BELOW
        } else {
            ARTIC_STACCATO_ABOVE
        });
    }
    match (accent, below) {
        (Some(Accent::Heavy), true) => wanted.push(ARTIC_MARCATO_BELOW),
        (Some(Accent::Heavy), false) => wanted.push(ARTIC_MARCATO_ABOVE),
        (Some(Accent::Normal), true) => wanted.push(ARTIC_ACCENT_BELOW),
        (Some(Accent::Normal), false) => wanted.push(ARTIC_ACCENT_ABOVE),
        (None, _) => {}
    }
    let width = beat
        .heads
        .iter()
        .find(|head| head.dx == Units::ZERO)
        .map_or(Units::ZERO, |head| head.width);
    let half = glyph_box(NOTEHEAD_BLACK, beat.size).max.y;
    let mut plans = Vec::new();
    if below {
        let lowest = beat
            .heads
            .iter()
            .map(|head| head.y)
            .max()
            .unwrap_or_default();
        let edge = (lowest + half).max(STAFF_HEIGHT);
        let mut cursor = reach.map_or(edge, |end| edge.max(end)) + ARTICULATION_GAP;
        for glyph in wanted {
            let rect = glyph_box(glyph, beat.size);
            let y = cursor - rect.min.y;
            plans.push(ArticulationPlan {
                glyph,
                dx: (width - rect.width()).half() - rect.min.x,
                y,
            });
            cursor = y + rect.max.y + ARTICULATION_GAP;
        }
    } else {
        let highest = beat
            .heads
            .iter()
            .map(|head| head.y)
            .min()
            .unwrap_or_default();
        let edge = (highest - half).min(Units::ZERO);
        let mut cursor = reach.map_or(edge, |end| edge.min(end)) - ARTICULATION_GAP;
        for glyph in wanted {
            let rect = glyph_box(glyph, beat.size);
            let y = cursor - rect.max.y;
            plans.push(ArticulationPlan {
                glyph,
                dx: (width - rect.width()).half() - rect.min.x,
                y,
            });
            cursor = y + rect.min.y - ARTICULATION_GAP;
        }
    }
    plans
}

fn beat_ink(beat: &BeatPlan, staff: &RhythmStaff) -> Option<Ink> {
    let mut ink = heads_ink(beat);
    let mut add = |rect: Rect| match ink.as_mut() {
        Some(existing) => existing.add(rect),
        None => ink = Some(Ink::of(rect)),
    };
    for head in &beat.heads {
        if let Some(mark) = head.accidental {
            add(offset_rect(
                glyph_box(mark.glyph, beat.size),
                Point {
                    x: mark.dx,
                    y: head.y,
                },
            ));
        }
        if head.tie_origin || head.tie_destination {
            let reach = scale(TIE_OFFSET + TIE_HEIGHT, beat.size);
            let y = if head.tie_up {
                head.y - reach
            } else {
                head.y + reach
            };
            add(Rect::new(
                Point {
                    x: head.dx,
                    y: y.min(head.y),
                },
                Point {
                    x: head.dx + head.width,
                    y: y.max(head.y),
                },
            ));
        }
    }
    for ledger in &beat.ledgers {
        let half = Metrics::bravura()
            .engraving_defaults()
            .leger_line_thickness
            .half();
        add(Rect::new(
            Point {
                x: ledger.left,
                y: ledger.y - half,
            },
            Point {
                x: ledger.right,
                y: ledger.y + half,
            },
        ));
    }
    if let Some(stem) = &beat.stem {
        let half = staff.stem_thickness().half();
        add(Rect::new(
            Point {
                x: stem.dx - half,
                y: stem.base.min(stem.end),
            },
            Point {
                x: stem.dx + half,
                y: stem.base.max(stem.end),
            },
        ));
        if !stem.beamed
            && let Some(flag) = staff.flag_extent(&stem_of(stem, Units::ZERO), stem.end)
        {
            add(flag);
        }
        if beat.slash {
            let (from, to) = slash(stem, staff);
            add(Rect::new(from, from).including(to));
        }
    }
    if let Some(rest) = &beat.rest
        && !rest.centred
    {
        add(offset_rect(
            glyph_box(rest.glyph, beat.size),
            Point {
                x: Units::ZERO,
                y: rest.y,
            },
        ));
    }
    if beat.dots > 0 {
        let width = staff.dots_width(beat.dots);
        for y in &beat.dot_ys {
            add(Rect::new(
                Point {
                    x: beat.dot_x,
                    y: *y - DOT_HALF_HEIGHT,
                },
                Point {
                    x: beat.dot_x + width,
                    y: *y + DOT_HALF_HEIGHT,
                },
            ));
        }
    }
    for articulation in &beat.articulations {
        add(offset_rect(
            glyph_box(articulation.glyph, beat.size),
            Point {
                x: articulation.dx,
                y: articulation.y,
            },
        ));
    }
    ink
}

pub fn slash(stem: &StemPlan, staff: &RhythmStaff) -> (Point, Point) {
    let flagged = Stem {
        x: stem.dx,
        base: stem.base,
        tip: stem.tip,
        direction: stem.direction,
        beams: stem.beams.max(1),
    };
    let origin = staff.flag_origin(&flagged, stem.end).map_or(
        Point {
            x: stem.dx,
            y: stem.end,
        },
        |(_, origin)| origin,
    );
    let south_west = anchor(FLAG_8TH_UP, Anchor::GraceNoteSlashSW, staff.size);
    let north_east = anchor(FLAG_8TH_UP, Anchor::GraceNoteSlashNE, staff.size);
    (origin.offset(south_west), origin.offset(north_east))
}

fn rest_plan(
    beat: &BeatPlan,
    staff: &RhythmStaff,
    whole_bar: bool,
    shift: Option<StemDirection>,
) -> RestPlan {
    let value = if whole_bar {
        NoteValue::Whole
    } else {
        beat.value
    };
    let glyph = if whole_bar {
        REST_WHOLE
    } else {
        rest_glyph(value)
    };
    let distance = match value {
        NoteValue::Whole | NoteValue::Half => LONG_REST_SHIFT,
        _ => REST_SHIFT,
    };
    let direction = shift;
    let shift = match shift {
        Some(StemDirection::Up) => -distance,
        Some(StemDirection::Down) => distance,
        None => Units::ZERO,
    };
    RestPlan {
        glyph,
        y: staff.rest_y(value, shift),
        centred: whole_bar,
        width: glyph_box(glyph, beat.size).max.x,
        away: direction,
    }
}

fn clear_rests(beats: &mut [BeatPlan], staff: &RhythmStaff) {
    for index in 0..beats.len() {
        let Some(rest) = beats[index].rest else {
            continue;
        };
        let Some(away) = rest.away else {
            continue;
        };
        if rest.centred {
            continue;
        }
        let column = beats[index].slot.column;
        let heads: Vec<(Units, Units)> = beats
            .iter()
            .filter(|other| other.slot.column == column && other.slot.grace.is_none())
            .flat_map(|other| &other.heads)
            .map(|head| {
                let rect = glyph_box(head.glyph, STAFF_SPACE);
                (
                    head.y + rect.min.y - REST_CLEARANCE,
                    head.y + rect.max.y + REST_CLEARANCE,
                )
            })
            .collect();
        let rect = glyph_box(rest.glyph, STAFF_SPACE);
        let step = Units(STAFF_SPACE.0 * away.sign());
        let mut y = rest.y;
        for _ in 0..REST_ATTEMPTS {
            let top = y + rect.min.y;
            let bottom = y + rect.max.y;
            if heads
                .iter()
                .all(|(low, high)| bottom <= *low || *high <= top)
            {
                break;
            }
            y += step;
        }
        let beat = &mut beats[index];
        if let Some(plan) = beat.rest.as_mut() {
            plan.y = y;
        }
        if beat.dots > 0 {
            beat.dot_ys = vec![staff.dot_y(y, false)];
        }
    }
}

fn voice_direction(voice_slot: u8) -> StemDirection {
    if voice_slot.is_multiple_of(2) {
        StemDirection::Up
    } else {
        StemDirection::Down
    }
}

pub fn analyse(input: &RowInput<'_>) -> Option<StaffNotation> {
    if !drawable(input.track) {
        return None;
    }
    let bar = input.bar?;
    let mut beats: Vec<BeatPlan> = Vec::new();
    let mut notes: Vec<&[Note]> = Vec::new();
    let mut lengths = Vec::new();
    for slot in input.slots {
        let Some(voice) = bar.voices.get(slot.voice) else {
            continue;
        };
        let Some(beat) = voice.beats.get(slot.beat) else {
            continue;
        };
        beats.push(new_beat(input, slot, voice.slot.get(), beat));
        notes.push(&beat.notes);
        lengths.push(beat.duration);
    }
    mark_accidentals(&mut beats, input.key);
    let timed = |beat: &BeatPlan| beat.slot.grace.is_none();
    let mut voices: Vec<usize> = beats.iter().map(|beat| beat.slot.voice).collect();
    voices.sort_unstable();
    voices.dedup();
    let with_notes = voices
        .iter()
        .filter(|voice| {
            beats
                .iter()
                .any(|beat| beat.slot.voice == **voice && timed(beat) && !beat.heads.is_empty())
        })
        .count();
    let with_content = voices
        .iter()
        .filter(|voice| {
            beats
                .iter()
                .any(|beat| beat.slot.voice == **voice && timed(beat))
        })
        .count();
    let staff = staff_geometry(STAFF_SPACE, StemPolicy::AwayFromReference);
    let cue = staff_geometry(CUE_SIZE, StemPolicy::Fixed(StemDirection::Up));
    let mut beams: Vec<BeamPlan> = Vec::new();
    let mut tuplet_members: Vec<(Vec<usize>, u8)> = Vec::new();
    let mut whole_rests: Vec<usize> = Vec::new();
    for voice in &voices {
        let members: Vec<usize> = (0..beats.len())
            .filter(|index| beats[*index].slot.voice == *voice && timed(&beats[*index]))
            .collect();
        let timings: Vec<Timing> = members
            .iter()
            .map(|index| {
                let beat = &beats[*index];
                Timing {
                    onset: input
                        .columns
                        .get(beat.slot.column)
                        .map_or(0, |column| column.ticks),
                    beamable: !beat.heads.is_empty() && flags_of(beat.value) > 0,
                }
            })
            .collect();
        let groups: Vec<Vec<usize>> = beam_groups(input.time, input.per_whole, &timings)
            .into_iter()
            .map(|group| group.into_iter().map(|offset| members[offset]).collect())
            .collect();
        for index in &members {
            let beat = &beats[*index];
            if beat.heads.is_empty() {
                continue;
            }
            let direction = if with_notes >= 2 {
                voice_direction(beat.voice_slot)
            } else {
                let group = groups.iter().find(|group| group.contains(index));
                match group {
                    Some(group) => {
                        staff.direction(group.iter().filter_map(|member| extremes(&beats[*member])))
                    }
                    None => staff.direction(extremes(beat)),
                }
            };
            beats[*index].direction = direction;
        }
        for group in groups {
            beams.push(BeamPlan {
                beats: group,
                line: BeamLine {
                    direction: StemDirection::Up,
                    start: Units::ZERO,
                    delta: Units::ZERO,
                },
            });
        }
        let durations: Vec<opentab_model::Duration> =
            members.iter().map(|index| lengths[*index]).collect();
        for run in tuplet_runs(&durations) {
            tuplet_members.push((members[run.first..=run.last].to_vec(), run.number));
        }
        let rests_only = members.len() == 1;
        if let Some(only) = members.first()
            && rests_only
            && notes[*only].is_empty()
            && lengths[*only].length() >= input.time.bar_length()
        {
            whole_rests.push(*only);
        }
    }
    for beat in &mut beats {
        if beat.slot.grace.is_some() {
            beat.direction = StemDirection::Up;
        } else if beat.heads.is_empty() && with_content >= 2 {
            beat.direction = voice_direction(beat.voice_slot);
        }
        displace(beat);
    }
    let column_count = input.columns.len();
    for column in 0..column_count {
        let members: Vec<usize> = (0..beats.len())
            .filter(|index| beats[*index].slot.column == column && timed(&beats[*index]))
            .collect();
        place_voices(&mut beats, &members);
        stack_accidentals(&mut beats, &members, STAFF_SPACE);
        let graces: Vec<usize> = (0..beats.len())
            .filter(|index| beats[*index].slot.column == column && !timed(&beats[*index]))
            .collect();
        for grace in graces {
            stack_accidentals(&mut beats, &[grace], CUE_SIZE);
        }
    }
    for (index, beat) in beats.iter_mut().enumerate() {
        let geometry = if beat.slot.grace.is_some() {
            &cue
        } else {
            &staff
        };
        beat.stem = stem_plan(beat, geometry);
        beat.ledgers = ledgers(beat);
        if beat.heads.is_empty() && notes[index].is_empty() {
            let shift = (with_content >= 2).then_some(beat.direction);
            beat.rest = Some(rest_plan(
                beat,
                geometry,
                whole_rests.contains(&index),
                shift,
            ));
        }
        let downward = with_notes >= 2 && beat.direction == StemDirection::Down;
        if beat.dots > 0 {
            match &beat.rest {
                Some(rest) if !rest.centred => {
                    beat.dot_x = rest.width;
                    beat.dot_ys = vec![geometry.dot_y(rest.y, false)];
                }
                Some(_) => {}
                None => {
                    beat.dot_x = beat
                        .heads
                        .iter()
                        .map(|head| head.dx + head.width)
                        .max()
                        .unwrap_or_default();
                    let mut ys: Vec<Units> = beat
                        .heads
                        .iter()
                        .map(|head| geometry.dot_y(head.y, downward))
                        .collect();
                    ys.sort_unstable();
                    ys.dedup();
                    beat.dot_ys = ys;
                }
            }
        }
    }
    clear_rests(&mut beats, &staff);
    for beam in &mut beams {
        let stems: Vec<Stem> = beam
            .beats
            .iter()
            .filter_map(|index| {
                beats[*index]
                    .stem
                    .as_ref()
                    .map(|plan| stem_of(plan, Units::ZERO))
            })
            .collect();
        if stems.len() != beam.beats.len() {
            beam.beats.clear();
            continue;
        }
        if let Some(line) = staff.beam_line(&stems) {
            beam.line = line;
            for index in &beam.beats {
                if let Some(stem) = beats[*index].stem.as_mut() {
                    stem.beamed = true;
                    stem.direction = line.direction;
                    stem.end = line.outer();
                }
            }
        }
    }
    beams.retain(|beam| !beam.beats.is_empty());
    for (index, beat) in beats.iter_mut().enumerate() {
        if beat.slot.grace.is_none() {
            beat.articulations = articulations(beat, notes[index], with_notes >= 2);
        }
    }
    for beat in &mut beats {
        let geometry = if beat.slot.grace.is_some() {
            &cue
        } else {
            &staff
        };
        beat.ink = beat_ink(beat, geometry);
    }
    let mut top = Units::ZERO;
    let mut bottom = STAFF_HEIGHT;
    for beat in &beats {
        if let Some(ink) = beat.ink {
            top = top.min(ink.top);
            bottom = bottom.max(ink.bottom);
        }
        if let Some(rest) = &beat.rest
            && rest.centred
        {
            let rect = glyph_box(rest.glyph, beat.size);
            top = top.min(rest.y + rect.min.y);
            bottom = bottom.max(rest.y + rect.max.y);
        }
    }
    for beam in &beams {
        let levels = beam
            .beats
            .iter()
            .filter_map(|index| beats[*index].stem.map(|stem| stem.beams))
            .max()
            .unwrap_or(1);
        let extent = staff.beam_extent(&beam.line, levels);
        top = top.min(extent.top);
        bottom = bottom.max(extent.bottom);
    }
    let mut tuplets = Vec::new();
    for (members, number) in tuplet_members {
        let (Some(first), Some(last)) = (members.first(), members.last()) else {
            continue;
        };
        let direction = members
            .iter()
            .find_map(|index| beats[*index].stem.map(|stem| stem.direction))
            .unwrap_or(StemDirection::Up);
        let mut reach: Option<Extremes> = None;
        let mut include = |low: Units, high: Units| {
            reach = Some(match reach {
                Some(known) => Extremes {
                    top: known.top.min(low),
                    bottom: known.bottom.max(high),
                },
                None => Extremes {
                    top: low,
                    bottom: high,
                },
            });
        };
        for index in &members {
            if let Some(ink) = beats[*index].ink {
                include(ink.top, ink.bottom);
            }
        }
        for beam in &beams {
            if beam.beats.iter().any(|index| members.contains(index)) {
                let levels = beam
                    .beats
                    .iter()
                    .filter_map(|index| beats[*index].stem.map(|stem| stem.beams))
                    .max()
                    .unwrap_or(1);
                let extent = staff.beam_extent(&beam.line, levels);
                include(extent.top, extent.bottom);
            }
        }
        let reach = reach.unwrap_or(Extremes {
            top: Units::ZERO,
            bottom: STAFF_HEIGHT,
        });
        let edge = staff.tuplet_edge(direction, reach);
        let bracket = TupletBracket {
            left: Units::ZERO,
            right: Units::ZERO,
            edge,
            direction,
            number,
        };
        let extent = staff.tuplet_extent(&bracket);
        top = top.min(extent.top);
        bottom = bottom.max(extent.bottom);
        tuplets.push(TupletPlan {
            first: *first,
            last: *last,
            number,
            direction,
            edge,
        });
    }
    let mut columns = vec![ColumnNeed::default(); column_count];
    for (column, need) in columns.iter_mut().enumerate() {
        for beat in beats.iter().filter(|beat| beat.slot.column == column) {
            let Some(ink) = beat.ink else {
                continue;
            };
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
        beams,
        tuplets,
        columns,
        top,
        bottom,
    })
}
