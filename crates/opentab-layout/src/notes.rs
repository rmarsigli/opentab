use std::collections::BTreeMap;

use crate::display::{Group, GroupId, Line, Primitive};
use crate::glyphs::{Metrics, NOTEHEAD_PARENTHESIS_LEFT, NOTEHEAD_PARENTHESIS_RIGHT};
use crate::notation::{
    BeatPlan, MIDDLE_LINE, StaffNotation, parenthesis_origins, slash, staff_geometry, stem_of,
};
use crate::rhythm::{BeamLine, RhythmStaff, Stem, StemPolicy, TupletBracket, glyph};
use crate::structure::{BarLayout, StaffLayout};
use crate::ties::TieAnchor;
use crate::units::{Point, STAFF_SPACE, Units};

fn geometry(staff: &StaffLayout, size: Units) -> RhythmStaff {
    RhythmStaff {
        reference: staff.top + MIDDLE_LINE,
        ..staff_geometry(size, StemPolicy::AwayFromReference)
    }
}

fn beat_x(beat: &BeatPlan, bar: &BarLayout) -> Units {
    if let Some(rest) = &beat.rest
        && rest.centred
    {
        return bar.content + (bar.rect.max.x - bar.content - rest.width).half();
    }
    bar.x_of(&beat.slot).unwrap_or(bar.content) + beat.offset
}

fn absolute(stem: Stem, top: Units) -> Stem {
    Stem {
        base: stem.base + top,
        tip: stem.tip + top,
        ..stem
    }
}

pub fn draw_bar(
    notation: &StaffNotation,
    bar: &BarLayout,
    staff: &StaffLayout,
    position: usize,
    items: &mut Vec<Primitive>,
    anchors: &mut Vec<TieAnchor>,
    sequences: &mut BTreeMap<u8, usize>,
) {
    let top = staff.top;
    let xs: Vec<Units> = notation
        .beats
        .iter()
        .map(|beat| beat_x(beat, bar))
        .collect();
    let normal = geometry(staff, STAFF_SPACE);
    let mut beamed: BTreeMap<usize, Units> = BTreeMap::new();
    let mut beams = Vec::new();
    for plan in &notation.beams {
        let stems: Vec<Stem> = plan
            .beats
            .iter()
            .filter_map(|index| {
                let beat = notation.beats.get(*index)?;
                let stem = beat.stem.as_ref()?;
                Some(absolute(stem_of(stem, xs[*index]), top))
            })
            .collect();
        let (Some(first), Some(last)) = (stems.first(), stems.last()) else {
            continue;
        };
        let line = BeamLine {
            start: plan.line.start + top,
            ..plan.line
        };
        for (index, stem) in plan.beats.iter().zip(&stems) {
            beamed.insert(*index, line.at(stem.x, first.x, last.x));
        }
        beams.push((stems, line));
    }
    for (index, beat) in notation.beats.iter().enumerate() {
        let x = xs[index];
        let counter = sequences.entry(beat.voice_slot).or_insert(0);
        let sequence = *counter;
        *counter += 1;
        let geometry = geometry(staff, beat.size);
        let mut beat_items = Vec::new();
        let thickness = Metrics::bravura().engraving_defaults().leger_line_thickness;
        for ledger in &beat.ledgers {
            beat_items.push(Primitive::Line(Line {
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
            beat_items.push(Primitive::Group(Group {
                id: GroupId(head.id.0.0),
                items: note_items,
            }));
            anchors.push(TieAnchor {
                bar: position,
                voice: beat.voice_slot,
                sequence,
                key: head
                    .string
                    .map_or(head.spelling.step, |string| -1 - i64::from(string)),
                left: x + head.dx,
                right: x + head.dx + head.width,
                y,
                up: head.tie_up,
                origin: head.tie_origin,
                destination: head.tie_destination,
                size: beat.size,
            });
        }
        if let Some(plan) = &beat.stem {
            let stem = absolute(stem_of(plan, x), top);
            match beamed.get(&index) {
                Some(end) => geometry.draw_stem(&stem, *end, &mut beat_items),
                None => {
                    let end = top + plan.end;
                    geometry.draw_stem(&stem, end, &mut beat_items);
                    geometry.draw_flag(&stem, end, &mut beat_items);
                }
            }
            if beat.slash {
                let (from, to) = slash(plan, &geometry);
                let shift = Point { x, y: top };
                beat_items.push(Primitive::Line(Line {
                    from: from.offset(shift),
                    to: to.offset(shift),
                    thickness: geometry.stem_thickness(),
                    dash: None,
                }));
            }
        }
        if beat.dots > 0 {
            let ys: Vec<Units> = beat.dot_ys.iter().map(|y| top + *y).collect();
            geometry.draw_dots(x + beat.dot_x, &ys, beat.dots, &mut beat_items);
        }
        for articulation in &beat.articulations {
            beat_items.push(glyph(
                articulation.glyph,
                Point {
                    x: x + articulation.dx,
                    y: top + articulation.y,
                },
                beat.size,
            ));
        }
        if let Some(rest) = &beat.rest {
            beat_items.push(glyph(rest.glyph, Point { x, y: top + rest.y }, beat.size));
        }
        items.push(Primitive::Group(Group {
            id: GroupId(beat.id.0.0),
            items: beat_items,
        }));
    }
    for (stems, line) in &beams {
        normal.draw_beams(stems, line, items);
    }
    for tuplet in &notation.tuplets {
        let (Some(first), Some(last)) = (
            notation.beats.get(tuplet.first),
            notation.beats.get(tuplet.last),
        ) else {
            continue;
        };
        let left = xs[tuplet.first]
            + first
                .heads
                .iter()
                .map(|head| head.dx)
                .min()
                .unwrap_or_default();
        let right = xs[tuplet.last]
            + last
                .heads
                .iter()
                .map(|head| head.dx + head.width)
                .max()
                .or(last.rest.map(|rest| rest.width))
                .unwrap_or_default();
        normal.draw_tuplet(
            &TupletBracket {
                left,
                right,
                edge: top + tuplet.edge,
                direction: tuplet.direction,
                number: tuplet.number,
            },
            items,
        );
    }
}
