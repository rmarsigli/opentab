use opentab_model::{Beat, Grace, KeySignature, Note, NoteKind, NoteValue, Track};

use super::{
    ACCIDENTAL_COLUMN_GAP, ACCIDENTAL_GAP, AccidentalMark, BeatPlan, HALF_SPACE, HeadPlan, Ink,
    LedgerPlan, PARENTHESIS_GAP, RowInput, offset_rect,
};
use crate::accidentals::Carry;
use crate::drums::drum_glyph;
use crate::glyphs::{
    Anchor, Glyph, Metrics, NOTEHEAD_BLACK, NOTEHEAD_DIAMOND_BLACK, NOTEHEAD_DIAMOND_HALF,
    NOTEHEAD_DIAMOND_WHOLE, NOTEHEAD_HALF, NOTEHEAD_PARENTHESIS_LEFT, NOTEHEAD_PARENTHESIS_RIGHT,
    NOTEHEAD_WHOLE, NOTEHEAD_X_BLACK, NOTEHEAD_X_HALF, NOTEHEAD_X_WHOLE,
};
use crate::pitch::{Spelling, position, spell, top_step, written_midi};
use crate::rhythm::{
    CUE_SIZE, Extremes, RhythmEvent, RhythmPlan, RhythmStaff, StemAnchor, StemDirection, anchor,
    glyph_box, scale,
};
use crate::structure::BeatSlot;
use crate::units::{Point, Rect, STAFF_SPACE, Units};

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

fn units(value: i64) -> Units {
    Units(HALF_SPACE.0.saturating_mul(value))
}

fn head(
    track: &Track,
    note: &Note,
    value: NoteValue,
    input: &RowInput<'_>,
    size: Units,
) -> Option<HeadPlan> {
    let (spelling, position, glyph) = match (note.kind, input.kit) {
        (NoteKind::Percussion { articulation }, Some(kit)) => {
            let drum = kit.entry(articulation);
            let spelling = Spelling {
                step: top_step(input.clef) - drum.position,
                alter: 0,
            };
            (spelling, drum.position, drum_glyph(drum.head, value, note))
        }
        _ => {
            let midi = written_midi(track, note)?;
            let spelling = spell(midi, input.key);
            (
                spelling,
                position(input.clef, spelling.step),
                head_glyph(value, note),
            )
        }
    };
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

pub fn new_beat(input: &RowInput<'_>, slot: &BeatSlot, voice_slot: u8, beat: &Beat) -> BeatPlan {
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
        grace: slot
            .grace
            .map(|_| beat.effects.grace.unwrap_or(Grace::BeforeBeat)),
        direction: StemDirection::Up,
        offset: Units::ZERO,
        heads,
        rest: None,
        dot_x: Units::ZERO,
        dot_ys: Vec::new(),
        ledgers: Vec::new(),
        articulations: Vec::new(),
        tremolo: None,
        fingers: Vec::new(),
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

pub fn mark_accidentals(beats: &mut [BeatPlan], key: KeySignature) {
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

pub fn extremes(beat: &BeatPlan) -> Option<Extremes> {
    let top = beat.heads.iter().map(|head| head.y).min()?;
    let bottom = beat.heads.iter().map(|head| head.y).max()?;
    Some(Extremes { top, bottom })
}

pub fn event(beat: &BeatPlan, onset: i128, duration: opentab_model::Duration) -> RhythmEvent {
    RhythmEvent {
        voice: beat.slot.voice,
        onset,
        duration,
        grace: beat.slot.grace.is_some(),
        sounding: !beat.heads.is_empty(),
        slash: beat.grace == Some(Grace::BeforeBeat),
        extremes: extremes(beat),
        left: Units::ZERO,
        right: Units::ZERO,
    }
}

pub fn displace(beat: &mut BeatPlan) {
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

fn parenthesis_boxes(head: &HeadPlan, size: Units) -> (Rect, Rect) {
    let (left, right) = parenthesis_origins(head, size);
    (
        offset_rect(glyph_box(NOTEHEAD_PARENTHESIS_LEFT, size), left),
        offset_rect(glyph_box(NOTEHEAD_PARENTHESIS_RIGHT, size), right),
    )
}

pub fn head_rects(beat: &BeatPlan) -> Vec<Rect> {
    let mut rects = Vec::new();
    for head in &beat.heads {
        rects.push(offset_rect(
            glyph_box(head.glyph, beat.size),
            Point {
                x: head.dx,
                y: head.y,
            },
        ));
        if head.parentheses {
            let (left, right) = parenthesis_boxes(head, beat.size);
            rects.push(left);
            rects.push(right);
        }
    }
    rects
}

pub fn heads_ink(beat: &BeatPlan) -> Option<Ink> {
    let rects = head_rects(beat);
    let (first, rest) = rects.split_first()?;
    let mut ink = Ink::of(*first);
    for rect in rest {
        ink.add(*rect);
    }
    Some(ink)
}

pub fn ledger_rect(ledger: &LedgerPlan) -> Rect {
    let half = Metrics::bravura()
        .engraving_defaults()
        .leger_line_thickness
        .half();
    Rect::new(
        Point {
            x: ledger.left,
            y: ledger.y - half,
        },
        Point {
            x: ledger.right,
            y: ledger.y + half,
        },
    )
}

pub fn ledgers(beat: &BeatPlan) -> Vec<LedgerPlan> {
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

pub fn stem_anchor(beat: &BeatPlan, staff: &RhythmStaff) -> Option<StemAnchor> {
    let normal: Vec<&HeadPlan> = beat
        .heads
        .iter()
        .filter(|head| head.dx == Units::ZERO)
        .collect();
    let thickness = staff.stem_thickness();
    match beat.direction {
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
            Some(StemAnchor {
                dx: attach.x - thickness.half(),
                base: lowest.y + attach.y,
                tip: beat.heads.iter().map(|head| head.y).min()?,
                direction: StemDirection::Up,
            })
        }
        StemDirection::Down => {
            let highest = normal.iter().min_by_key(|head| head.y)?;
            let attach = anchor(highest.glyph, Anchor::StemDownNW, beat.size);
            Some(StemAnchor {
                dx: attach.x + thickness.half(),
                base: highest.y + attach.y,
                tip: beat.heads.iter().map(|head| head.y).max()?,
                direction: StemDirection::Down,
            })
        }
    }
}

fn shifted(rect: Rect, by: Units) -> Rect {
    offset_rect(
        rect,
        Point {
            x: by,
            y: Units::ZERO,
        },
    )
}

struct Body {
    rects: Vec<Rect>,
    left: Units,
    right: Units,
}

fn body(beat: &BeatPlan, stem: Option<Rect>) -> Option<Body> {
    let mut rects = head_rects(beat);
    if rects.is_empty() {
        return None;
    }
    rects.extend(beat.ledgers.iter().map(ledger_rect));
    rects.extend(stem);
    let left = rects.iter().map(|rect| rect.min.x).min()?;
    let right = rects.iter().map(|rect| rect.max.x).max()?;
    Some(Body { rects, left, right })
}

pub fn place_voices(
    beats: &mut [BeatPlan],
    members: &[usize],
    plan: &RhythmPlan,
    events: &[RhythmEvent],
    staff: &RhythmStaff,
) {
    let mut order: Vec<usize> = members
        .iter()
        .copied()
        .filter(|index| !beats[*index].heads.is_empty())
        .collect();
    order.sort_by_key(|index| {
        let beat = &beats[*index];
        (beat.direction == StemDirection::Up, beat.slot.voice)
    });
    let mut placed: Vec<(Units, Body)> = Vec::new();
    for index in order {
        let stem = events
            .get(index)
            .and_then(|event| plan.stem_rect(staff, event, index));
        let Some(this) = body(&beats[index], stem) else {
            continue;
        };
        let mut offset = Units::ZERO;
        loop {
            let next = placed
                .iter()
                .filter(|(other_offset, other)| {
                    this.rects.iter().any(|mine| {
                        other.rects.iter().any(|theirs| {
                            shifted(*mine, offset).overlaps(&shifted(*theirs, *other_offset))
                        })
                    })
                })
                .map(|(other_offset, other)| *other_offset + other.right - this.left)
                .max();
            match next {
                Some(next) if next > offset => offset = next,
                _ => break,
            }
        }
        beats[index].offset = offset;
        placed.push((offset, this));
    }
}

struct Pending {
    beat: usize,
    head: usize,
    glyph: Glyph,
    y: Units,
}

pub fn stack_accidentals(beats: &mut [BeatPlan], members: &[usize], size: Units) {
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
