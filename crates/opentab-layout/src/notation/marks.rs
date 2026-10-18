use opentab_model::{Accent, Note, NoteValue};

use super::heads::{head_rects, ledger_rect};
use super::{
    ARTICULATION_GAP, ArticulationPlan, BeatPlan, DOT_HALF_HEIGHT, Ink, LONG_REST_SHIFT,
    LedgerPlan, REST_ATTEMPTS, REST_CLEARANCE, REST_SHIFT, RestPlan, STAFF_HEIGHT, TIE_HEIGHT,
    TIE_OFFSET, offset_rect,
};
use crate::glyphs::{
    ARTIC_ACCENT_ABOVE, ARTIC_ACCENT_BELOW, ARTIC_MARCATO_ABOVE, ARTIC_MARCATO_BELOW,
    ARTIC_STACCATO_ABOVE, ARTIC_STACCATO_BELOW, Metrics, NOTEHEAD_BLACK, REST_HALF, REST_WHOLE,
};
use crate::rhythm::{
    RhythmEvent, RhythmPlan, RhythmStaff, StemDirection, glyph_box, rest_glyph, scale,
};
use crate::units::{Point, Rect, STAFF_SPACE, Units};

pub fn rest_plan(
    beat: &BeatPlan,
    staff: &RhythmStaff,
    whole_bar: bool,
    away: Option<StemDirection>,
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
    let shift = match away {
        Some(StemDirection::Up) => -distance,
        Some(StemDirection::Down) => distance,
        None => Units::ZERO,
    };
    RestPlan {
        glyph,
        y: staff.rest_y(value, shift),
        centred: whole_bar,
        width: glyph_box(glyph, beat.size).max.x,
        away,
    }
}

fn dots_rect(beat: &BeatPlan, staff: &RhythmStaff, y: Units) -> Option<Rect> {
    if beat.dots == 0 {
        return None;
    }
    let width = staff.dots_width(beat.dots);
    Some(Rect::new(
        Point {
            x: beat.dot_x,
            y: y - DOT_HALF_HEIGHT,
        },
        Point {
            x: beat.dot_x + width,
            y: y + DOT_HALF_HEIGHT,
        },
    ))
}

pub fn rest_dot_y(rest: &RestPlan, staff: &RhythmStaff) -> Units {
    let rect = glyph_box(rest.glyph, STAFF_SPACE);
    let middle = rest.y + (rect.min.y + rect.max.y).half();
    let half = staff.spacing.half();
    let steps = (middle - staff.reference).0.div_euclid(half.0.max(1));
    let snapped = staff.reference + Units(half.0 * steps);
    if staff.on_line(snapped) {
        snapped - half
    } else {
        snapped
    }
}

pub fn rest_body(beat: &BeatPlan, staff: &RhythmStaff, rest: &RestPlan) -> Vec<Rect> {
    let mut rects = vec![offset_rect(
        glyph_box(rest.glyph, beat.size),
        Point {
            x: Units::ZERO,
            y: rest.y,
        },
    )];
    if !rest.centred {
        let probe = BeatPlan {
            dot_x: rest.width,
            ..beat.clone()
        };
        rects.extend(dots_rect(&probe, staff, rest_dot_y(rest, staff)));
    }
    rects
}

pub fn content(
    beat: &BeatPlan,
    index: usize,
    plan: &RhythmPlan,
    events: &[RhythmEvent],
    staff: &RhythmStaff,
) -> Vec<Rect> {
    let mut rects = head_rects(beat);
    rects.extend(beat.ledgers.iter().map(ledger_rect));
    if let Some(rect) = events
        .get(index)
        .and_then(|event| plan.stem_rect(staff, event, index))
    {
        rects.push(rect);
    }
    for y in &beat.dot_ys {
        rects.extend(dots_rect(beat, staff, *y));
    }
    if let Some(rest) = &beat.rest {
        rects.extend(rest_body(beat, staff, rest));
    }
    rects
}

pub fn place_dots(beats: &mut [BeatPlan], members: &[usize], staff: &RhythmStaff, multi: bool) {
    let right = members
        .iter()
        .filter_map(|index| {
            let beat = &beats[*index];
            let edge = head_rects(beat).iter().map(|rect| rect.max.x).max()?;
            Some(beat.offset + edge)
        })
        .max();
    let Some(right) = right else {
        return;
    };
    let mut taken: Vec<(usize, Units)> = Vec::new();
    let mut order: Vec<usize> = members
        .iter()
        .copied()
        .filter(|index| beats[*index].dots > 0 && !beats[*index].heads.is_empty())
        .collect();
    order.sort_by_key(|index| beats[*index].slot.voice);
    for index in order {
        let beat = &mut beats[index];
        let downward = multi && beat.direction == StemDirection::Down;
        let step = if downward {
            staff.spacing
        } else {
            -staff.spacing
        };
        let voice = beat.slot.voice;
        let mut ys: Vec<Units> = beat
            .heads
            .iter()
            .map(|head| staff.dot_y(head.y, downward))
            .collect();
        ys.sort_unstable();
        ys.dedup();
        let mut placed = Vec::with_capacity(ys.len());
        for mut y in ys {
            while taken
                .iter()
                .any(|(other, used)| *other != voice && *used == y)
            {
                y += step;
            }
            placed.push(y);
        }
        for y in &placed {
            taken.push((voice, *y));
        }
        placed.sort_unstable();
        placed.dedup();
        beat.dot_x = right - beat.offset;
        beat.dot_ys = placed;
    }
}

fn separated(rects: &[Rect], obstacles: &[Rect]) -> bool {
    rects.iter().all(|rect| {
        obstacles.iter().all(|other| {
            rect.max.y + REST_CLEARANCE <= other.min.y || other.max.y + REST_CLEARANCE <= rect.min.y
        })
    })
}

pub struct RestContext<'a> {
    pub plan: &'a RhythmPlan,
    pub events: &'a [RhythmEvent],
    pub staff: &'a RhythmStaff,
}

fn spans_column(plan: &RhythmPlan, beats: &[BeatPlan], voice: usize, column: usize) -> Vec<usize> {
    plan.beams
        .iter()
        .enumerate()
        .filter(|(_, beam)| {
            let columns: Vec<usize> = beam
                .members
                .iter()
                .filter_map(|member| beats.get(*member))
                .map(|beat| beat.slot.column)
                .collect();
            let first = columns.iter().min();
            let last = columns.iter().max();
            let owner = beam
                .members
                .first()
                .and_then(|member| beats.get(*member))
                .map(|beat| beat.slot.voice);
            owner != Some(voice)
                && matches!((first, last), (Some(first), Some(last)) if *first <= column && column <= *last)
        })
        .map(|(index, _)| index)
        .collect()
}

pub fn clear_rests(beats: &mut [BeatPlan], context: &RestContext<'_>) {
    for index in 0..beats.len() {
        let Some(rest) = beats[index].rest else {
            continue;
        };
        let Some(away) = rest.away else {
            continue;
        };
        let voice = beats[index].slot.voice;
        let column = beats[index].slot.column;
        let mut obstacles = Vec::new();
        for (other_index, other) in beats.iter().enumerate() {
            let near = rest.centred || (other.slot.column == column && other.slot.grace.is_none());
            if other.slot.voice != voice && near {
                obstacles.extend(content(
                    other,
                    other_index,
                    context.plan,
                    context.events,
                    context.staff,
                ));
            }
        }
        let beams = if rest.centred {
            (0..context.plan.beams.len())
                .filter(|beam| {
                    context.plan.beams[*beam]
                        .members
                        .first()
                        .and_then(|member| beats.get(*member))
                        .is_some_and(|first| first.slot.voice != voice)
                })
                .collect()
        } else {
            spans_column(context.plan, beats, voice, column)
        };
        for beam in beams {
            let extent = context
                .plan
                .beam_extent(context.staff, &context.plan.beams[beam]);
            obstacles.push(Rect::new(
                Point {
                    x: Units::ZERO,
                    y: extent.top,
                },
                Point {
                    x: Units::ZERO,
                    y: extent.bottom,
                },
            ));
        }
        let step = Units(STAFF_SPACE.0 * away.sign());
        let mut moved = rest;
        for _ in 0..REST_ATTEMPTS {
            if separated(&rest_body(&beats[index], context.staff, &moved), &obstacles) {
                break;
            }
            moved.y += step;
        }
        beats[index].rest = Some(moved);
    }
}

pub fn finish_rest(beat: &mut BeatPlan, staff: &RhythmStaff) {
    let Some(rest) = beat.rest else {
        return;
    };
    if beat.dots > 0 && !rest.centred {
        beat.dot_x = rest.width;
        beat.dot_ys = vec![rest_dot_y(&rest, staff)];
    }
    let long = rest.glyph == REST_WHOLE || rest.glyph == REST_HALF;
    if long && (rest.y < Units::ZERO || rest.y > STAFF_HEIGHT) {
        let extension = scale(
            Metrics::bravura().engraving_defaults().leger_line_extension,
            beat.size,
        );
        beat.ledgers.push(LedgerPlan {
            y: rest.y,
            left: -extension,
            right: rest.width + extension,
        });
    }
}

pub fn articulations(
    beat: &BeatPlan,
    notes: &[Note],
    stem_end: Option<Units>,
    stem_side: bool,
) -> Vec<ArticulationPlan> {
    let staccato = notes.iter().any(|note| note.effects.staccato);
    let accent = notes
        .iter()
        .filter_map(|note| note.effects.accent)
        .max_by_key(|accent| matches!(accent, Accent::Heavy));
    if beat.heads.is_empty() || (!staccato && accent.is_none()) {
        return Vec::new();
    }
    let below = (beat.direction == StemDirection::Up) != stem_side;
    let reach = stem_end.filter(|_| stem_side);
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

pub fn beat_ink(beat: &BeatPlan, stem: Option<Rect>, staff: &RhythmStaff) -> Option<Ink> {
    let mut rects = head_rects(beat);
    for head in &beat.heads {
        if let Some(mark) = head.accidental {
            rects.push(offset_rect(
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
            rects.push(Rect::new(
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
    rects.extend(beat.ledgers.iter().map(ledger_rect));
    rects.extend(stem);
    if let Some(rest) = &beat.rest
        && !rest.centred
    {
        rects.push(offset_rect(
            glyph_box(rest.glyph, beat.size),
            Point {
                x: Units::ZERO,
                y: rest.y,
            },
        ));
    }
    for y in &beat.dot_ys {
        rects.extend(dots_rect(beat, staff, *y));
    }
    for articulation in &beat.articulations {
        rects.push(offset_rect(
            glyph_box(articulation.glyph, beat.size),
            Point {
                x: articulation.dx,
                y: articulation.y,
            },
        ));
    }
    let (first, rest) = rects.split_first()?;
    let mut ink = Ink::of(*first);
    for rect in rest {
        ink.add(*rect);
    }
    Some(ink)
}
