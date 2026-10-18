use std::collections::BTreeMap;

use crate::display::{
    Colour, FilledRect, Font, Group, GroupId, Line, Primitive, TextAnchor, TextRun,
};
use crate::glyphs::{Metrics, NOTEHEAD_PARENTHESIS_LEFT, NOTEHEAD_PARENTHESIS_RIGHT};
use crate::notation::{BeatPlan, StaffNotation, finger_rect, parenthesis_origins, staff_geometry};
use crate::rhythm::{RhythmCanvas, RhythmStaff, StemPolicy, draw_rhythm, glyph};
use crate::structure::{BarLayout, StaffLayout};
use crate::tab::FINGER_EM;
use crate::ties::TieAnchor;
use crate::units::{Point, Rect, STAFF_SPACE, Units};

fn beat_x(beat: &BeatPlan, bar: &BarLayout) -> Units {
    if let Some(rest) = &beat.rest
        && rest.centred
    {
        return bar.content + (bar.rect.max.x - bar.content - rest.width).half();
    }
    bar.x_of(&beat.slot).unwrap_or(bar.content) + beat.offset
}

pub struct TieRow<'a> {
    pub position: usize,
    pub anchors: &'a mut Vec<TieAnchor>,
    pub sequences: &'a mut BTreeMap<u8, usize>,
}

fn beat_items(beat: &BeatPlan, x: Units, top: Units, geometry: &RhythmStaff) -> Vec<Primitive> {
    let mut items = Vec::new();
    let thickness = Metrics::bravura().engraving_defaults().leger_line_thickness;
    for finger in &beat.fingers {
        let rect = finger_rect(finger);
        items.push(Primitive::FilledRect(FilledRect {
            rect: Rect::new(
                Point {
                    x: x + rect.min.x,
                    y: top + rect.min.y,
                },
                Point {
                    x: x + rect.max.x,
                    y: top + rect.max.y,
                },
            ),
            colour: Colour::White,
        }));
        items.push(Primitive::Text(TextRun {
            origin: Point {
                x: x + finger.right,
                y: top + finger.baseline,
            },
            text: finger.text.to_string(),
            font: Font::Text,
            em: FINGER_EM,
            anchor: TextAnchor::End,
        }));
    }
    for ledger in &beat.ledgers {
        items.push(Primitive::Line(Line {
            from: Point {
                x: x + ledger.left,
                y: top + ledger.y,
            },
            to: Point {
                x: x + ledger.right,
                y: top + ledger.y,
            },
            thickness,
            dash: None,
        }));
    }
    for head in &beat.heads {
        let y = top + head.y;
        let mut note_items = Vec::new();
        if let Some(mark) = head.accidental {
            note_items.push(glyph(mark.glyph, Point { x: x + mark.dx, y }, beat.size));
        }
        note_items.push(glyph(head.glyph, Point { x: x + head.dx, y }, beat.size));
        if head.parentheses {
            let (left, right) = parenthesis_origins(head, beat.size);
            for (part, origin) in [
                (NOTEHEAD_PARENTHESIS_LEFT, left),
                (NOTEHEAD_PARENTHESIS_RIGHT, right),
            ] {
                note_items.push(glyph(
                    part,
                    Point {
                        x: x + origin.x,
                        y: top + origin.y,
                    },
                    beat.size,
                ));
            }
        }
        items.push(Primitive::Group(Group {
            id: GroupId(head.id.0.0),
            items: note_items,
        }));
    }
    if beat.dots > 0 {
        let ys: Vec<Units> = beat.dot_ys.iter().map(|y| top + *y).collect();
        geometry.draw_dots(x + beat.dot_x, &ys, beat.dots, &mut items);
    }
    for articulation in &beat.articulations {
        items.push(glyph(
            articulation.glyph,
            Point {
                x: x + articulation.dx,
                y: top + articulation.y,
            },
            beat.size,
        ));
    }
    if let Some(rest) = &beat.rest {
        items.push(glyph(rest.glyph, Point { x, y: top + rest.y }, beat.size));
    }
    if let Some(tremolo) = &beat.tremolo {
        items.push(glyph(
            tremolo.glyph,
            Point {
                x: x + tremolo.dx,
                y: top + tremolo.y,
            },
            beat.size,
        ));
    }
    items
}

fn record_ties(beat: &BeatPlan, x: Units, top: Units, row: &mut TieRow<'_>) {
    if beat.slot.grace.is_some() {
        return;
    }
    let counter = row.sequences.entry(beat.voice_slot).or_insert(0);
    let sequence = *counter;
    *counter += 1;
    for head in &beat.heads {
        row.anchors.push(TieAnchor {
            bar: row.position,
            voice: beat.voice_slot,
            sequence,
            key: head
                .string
                .map_or(head.spelling.step, |string| -1 - i64::from(string)),
            left: x + head.dx,
            right: x + head.dx + head.width,
            y: top + head.y,
            up: head.tie_up,
            origin: head.tie_origin,
            destination: head.tie_destination,
            size: beat.size,
        });
    }
}

pub fn draw_bar(
    notation: &StaffNotation,
    bar: &BarLayout,
    staff: &StaffLayout,
    items: &mut Vec<Primitive>,
    row: &mut TieRow<'_>,
) {
    let top = staff.top;
    let geometry = staff_geometry(STAFF_SPACE, StemPolicy::AwayFromReference);
    let xs: Vec<Units> = notation
        .beats
        .iter()
        .map(|beat| beat_x(beat, bar))
        .collect();
    let mut per_beat: Vec<Vec<Primitive>> = notation
        .beats
        .iter()
        .zip(&notation.events)
        .zip(&xs)
        .map(|((beat, event), x)| beat_items(beat, *x, top, &geometry.for_event(event)))
        .collect();
    let mut shared = Vec::new();
    draw_rhythm(
        &geometry,
        &notation.rhythm,
        &notation.events,
        &mut RhythmCanvas {
            xs: &xs,
            top,
            events: &mut per_beat,
            shared: &mut shared,
        },
    );
    for ((beat, x), beat_items) in notation.beats.iter().zip(&xs).zip(per_beat) {
        record_ties(beat, *x, top, row);
        items.push(Primitive::Group(Group {
            id: GroupId(beat.id.0.0),
            items: beat_items,
        }));
    }
    items.extend(shared);
}
