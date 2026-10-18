use crate::display::{PathSegment, PathStyle, Primitive, Shape};
use crate::geometry::NUMBER_EM;
use crate::glyphs::MET_AUGMENTATION_DOT;
use crate::lanes::{BandLayout, Lane};
use crate::marks::fit::{ELLIPSIS, Fitted, descent, fit_line};
use crate::marks::tempo::{MarkAnchor, TempoMark, unit_glyph};
use crate::marks::{
    CHORD_EM, CHORD_MAX, DYNAMIC_GAP, DYNAMIC_SIZE, MARK_GAP, NUMBER_GAP, SECTION_EM, SECTION_GAP,
    SECTION_LINE, SECTION_PAD, SectionMark, TEMPO_DOT_GAP, TEMPO_EM, TEMPO_SIZE, TEMPO_TEXT_GAP,
    TEXT_BACK, TEXT_EM, dynamic_ascent, dynamic_glyph,
};
use crate::marks::{DroppedMark, MarkKind};
use crate::plan::{BarPlan, notehead_width};
use crate::rhythm::{glyph, glyph_box};
use crate::structure::{BarLayout, StaffLayout};
use crate::text::text_font;
use crate::units::{Point, Units};

pub struct SystemMarks<'a> {
    pub plans: &'a [BarPlan],
    pub staves: &'a [StaffLayout],
    pub bands: &'a [BandLayout],
    pub left: Units,
    pub right: Units,
    pub number: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MarkOutput {
    pub items: Vec<Primitive>,
    pub dropped: Vec<DroppedMark>,
}

impl MarkOutput {
    fn drop_mark(&mut self, bar: usize, kind: MarkKind) {
        self.dropped.push(DroppedMark { bar, kind });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request {
    pub desired: Units,
    pub width: Units,
    pub least: Units,
    pub back: Units,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot {
    pub left: Units,
    pub width: Units,
}

pub fn place(requests: &[Request], left: Units, right: Units) -> Vec<Option<Slot>> {
    let mut order: Vec<usize> = (0..requests.len()).collect();
    order.sort_by_key(|index| requests[*index].desired);
    let mut slots = vec![None; requests.len()];
    let mut cursor = left;
    for index in order {
        let request = requests[index];
        let fitting = right - request.width;
        let start = if fitting >= request.desired - request.back {
            request.desired.min(fitting)
        } else {
            request.desired
        }
        .max(cursor);
        let width = request.width.min(right - start);
        if width < request.least || width <= Units::ZERO {
            continue;
        }
        slots[index] = Some(Slot { left: start, width });
        cursor = start + width + MARK_GAP;
    }
    slots
}

fn least_text(em: Units, width: Units) -> Units {
    width.min(text_font().advance(ELLIPSIS, em))
}

fn column_x(bar: &BarLayout, column: usize) -> Units {
    bar.columns.get(column).map_or(bar.content, |found| found.x)
}

struct TextItem {
    bar: usize,
    desired: Units,
    line: Fitted,
    back: Units,
}

fn draw_texts(
    texts: Vec<TextItem>,
    marks: &SystemMarks<'_>,
    baseline: Units,
    kind: MarkKind,
    out: &mut MarkOutput,
) {
    let requests: Vec<Request> = texts
        .iter()
        .map(|item| Request {
            desired: item.desired,
            width: item.line.width(),
            least: least_text(item.line.em, item.line.width()),
            back: item.back.min(item.line.width()),
        })
        .collect();
    for (item, slot) in texts.iter().zip(place(&requests, marks.left, marks.right)) {
        let line = slot.and_then(|slot| {
            if slot.width < item.line.width() {
                fit_line(&item.line.text, item.line.em, slot.width)
            } else {
                Some(item.line.clone())
            }
            .map(|line| (slot, line))
        });
        match line {
            Some((slot, line)) => out.items.push(line.run(slot.left, baseline)),
            None => out.drop_mark(item.bar, kind),
        }
    }
}

fn lane_rows(marks: &SystemMarks<'_>, lane: Lane) -> Vec<(usize, Units)> {
    marks
        .staves
        .iter()
        .enumerate()
        .filter_map(|(row, staff)| {
            let placed = marks.bands.get(row)?.lane(lane)?;
            Some((row, staff.top + placed.bottom))
        })
        .collect()
}

fn draw_chords(marks: &SystemMarks<'_>, out: &mut MarkOutput) {
    for (row, bottom) in lane_rows(marks, Lane::Chord) {
        let Some(staff) = marks.staves.get(row) else {
            continue;
        };
        let mut items = Vec::new();
        for (plan, bar) in marks.plans.iter().zip(&staff.bars) {
            let Some(row_marks) = plan.marks.rows.get(row) else {
                continue;
            };
            for chord in &row_marks.chords {
                items.push(TextItem {
                    bar: plan.index,
                    desired: column_x(bar, chord.column),
                    line: chord.name.clone(),
                    back: CHORD_MAX,
                });
            }
        }
        draw_texts(
            items,
            marks,
            bottom - descent(CHORD_EM),
            MarkKind::Chord,
            out,
        );
    }
}

fn draw_free_texts(marks: &SystemMarks<'_>, out: &mut MarkOutput) {
    let room = marks.right - marks.left;
    for (row, bottom) in lane_rows(marks, Lane::Text) {
        let Some(staff) = marks.staves.get(row) else {
            continue;
        };
        let mut items = Vec::new();
        for (plan, bar) in marks.plans.iter().zip(&staff.bars) {
            let Some(row_marks) = plan.marks.rows.get(row) else {
                continue;
            };
            for text in &row_marks.texts {
                if let Some(line) = fit_line(&text.text, TEXT_EM, room) {
                    items.push(TextItem {
                        bar: plan.index,
                        desired: column_x(bar, text.column),
                        line,
                        back: TEXT_BACK,
                    });
                }
            }
        }
        draw_texts(items, marks, bottom - descent(TEXT_EM), MarkKind::Text, out);
    }
}

struct TempoItem {
    bar: usize,
    desired: Units,
    mark: TempoMark,
    text: Fitted,
}

fn tempo_width(item: &TempoItem) -> Units {
    let (note, dotted) = unit_glyph(item.mark.unit);
    let mut width = glyph_box(note, TEMPO_SIZE).width();
    if dotted {
        width += TEMPO_DOT_GAP + glyph_box(MET_AUGMENTATION_DOT, TEMPO_SIZE).width();
    }
    width + TEMPO_TEXT_GAP + item.text.width()
}

fn draw_tempo(item: &TempoItem, left: Units, baseline: Units, out: &mut Vec<Primitive>) {
    let (note, dotted) = unit_glyph(item.mark.unit);
    let note_box = glyph_box(note, TEMPO_SIZE);
    let origin = Point {
        x: left - note_box.min.x,
        y: baseline - note_box.max.y,
    };
    out.push(glyph(note, origin, TEMPO_SIZE));
    let mut cursor = left + note_box.width();
    if dotted {
        let dot = glyph_box(MET_AUGMENTATION_DOT, TEMPO_SIZE);
        let at = cursor + TEMPO_DOT_GAP;
        out.push(glyph(
            MET_AUGMENTATION_DOT,
            Point {
                x: at - dot.min.x,
                y: origin.y,
            },
            TEMPO_SIZE,
        ));
        cursor = at + dot.width();
    }
    out.push(item.text.run(cursor + TEMPO_TEXT_GAP, baseline));
}

fn anchor_x(bar: &BarLayout, anchor: MarkAnchor) -> Units {
    match anchor {
        MarkAnchor::Column(column) => column_x(bar, column),
        MarkAnchor::Start => bar.content,
        MarkAnchor::End => bar.rect.max.x,
    }
}

fn draw_tempos(marks: &SystemMarks<'_>, out: &mut MarkOutput) {
    let (Some(staff), Some(lane)) = (
        marks.staves.first(),
        marks.bands.first().and_then(|band| band.lane(Lane::Tempo)),
    ) else {
        return;
    };
    let mut items = Vec::new();
    for (plan, bar) in marks.plans.iter().zip(&staff.bars) {
        for mark in &plan.marks.tempos {
            let Some(text) = fit_line(
                &format!("= {}", mark.bpm),
                TEMPO_EM,
                marks.right - marks.left,
            ) else {
                continue;
            };
            items.push(TempoItem {
                bar: plan.index,
                desired: anchor_x(bar, mark.anchor),
                mark: mark.clone(),
                text,
            });
        }
    }
    let requests: Vec<Request> = items
        .iter()
        .map(|item| {
            let width = tempo_width(item);
            Request {
                desired: item.desired,
                width,
                least: width,
                back: width,
            }
        })
        .collect();
    let baseline = staff.top + lane.bottom - descent(TEMPO_EM);
    for (item, slot) in items.iter().zip(place(&requests, marks.left, marks.right)) {
        match slot {
            Some(slot) => draw_tempo(item, slot.left, baseline, &mut out.items),
            None => out.drop_mark(item.bar, MarkKind::Tempo),
        }
    }
}

fn boxed(left: Units, top: Units, right: Units, bottom: Units) -> Primitive {
    Primitive::Shape(Shape {
        segments: vec![
            PathSegment::MoveTo(Point { x: left, y: top }),
            PathSegment::LineTo(Point { x: right, y: top }),
            PathSegment::LineTo(Point {
                x: right,
                y: bottom,
            }),
            PathSegment::LineTo(Point { x: left, y: bottom }),
            PathSegment::Close,
        ],
        style: PathStyle::Stroke(SECTION_LINE),
    })
}

fn box_width(section: &SectionMark) -> Units {
    section.letter.as_ref().map_or(Units::ZERO, |letter| {
        letter.width() + SECTION_PAD + SECTION_PAD + SECTION_LINE
    })
}

fn draw_section(
    section: &SectionMark,
    text: Option<&Fitted>,
    slot: Slot,
    baseline: Units,
    out: &mut Vec<Primitive>,
) {
    let mut cursor = slot.left;
    if let Some(letter) = &section.letter {
        let half = SECTION_LINE.half();
        let outer = box_width(section);
        out.push(boxed(
            slot.left + half,
            baseline + letter.ink.min.y - SECTION_PAD,
            slot.left + outer - half,
            baseline + letter.ink.max.y + SECTION_PAD,
        ));
        out.push(letter.run(slot.left + half + SECTION_PAD, baseline));
        cursor = slot.left + outer + SECTION_GAP;
    }
    let room = slot.left + slot.width - cursor;
    if let Some(line) = text.and_then(|line| fit_line(&line.text, SECTION_EM, room)) {
        out.push(line.run(cursor, baseline));
    }
}

fn draw_sections(marks: &SystemMarks<'_>, out: &mut MarkOutput) {
    let (Some(staff), Some(lane)) = (
        marks.staves.first(),
        marks
            .bands
            .first()
            .and_then(|band| band.lane(Lane::Section)),
    ) else {
        return;
    };
    let shift = marks.number.as_ref().map_or(Units::ZERO, |number| {
        text_font().measure(number, NUMBER_EM) + NUMBER_GAP
    });
    let mut found = Vec::new();
    for (offset, (plan, bar)) in marks.plans.iter().zip(&staff.bars).enumerate() {
        let Some(section) = &plan.marks.section else {
            continue;
        };
        let text = fit_line(&section.text, SECTION_EM, marks.right - marks.left);
        let boxed = box_width(section);
        let text_width = text.as_ref().map_or(Units::ZERO, |line| {
            line.width()
                + if boxed > Units::ZERO {
                    SECTION_GAP
                } else {
                    Units::ZERO
                }
        });
        let width = boxed + text_width;
        let desired = bar.rect.min.x + if offset == 0 { shift } else { Units::ZERO };
        let least = if boxed > Units::ZERO {
            boxed
        } else {
            least_text(SECTION_EM, width)
        };
        found.push((
            plan.index,
            section,
            text,
            Request {
                desired,
                width,
                least,
                back: boxed,
            },
        ));
    }
    let requests: Vec<Request> = found.iter().map(|(_, _, _, request)| *request).collect();
    let bottom = staff.top + lane.bottom - SECTION_LINE.half() - SECTION_PAD;
    let baseline = bottom - descent(SECTION_EM);
    for ((bar, section, text, _), slot) in
        found.iter().zip(place(&requests, marks.left, marks.right))
    {
        match slot {
            Some(slot) => draw_section(section, text.as_ref(), slot, baseline, &mut out.items),
            None => out.drop_mark(*bar, MarkKind::Section),
        }
    }
}

fn draw_dynamics(marks: &SystemMarks<'_>, out: &mut MarkOutput) {
    for (row, staff) in marks.staves.iter().enumerate() {
        let floor = marks
            .plans
            .iter()
            .filter_map(|plan| plan.marks.rows.get(row))
            .filter(|row_marks| !row_marks.dynamics.is_empty())
            .map(|row_marks| row_marks.floor)
            .max();
        let Some(floor) = floor else {
            continue;
        };
        let baseline = staff.top + floor + DYNAMIC_GAP + dynamic_ascent();
        let mut found = Vec::new();
        for (plan, bar) in marks.plans.iter().zip(&staff.bars) {
            let Some(row_marks) = plan.marks.rows.get(row) else {
                continue;
            };
            for mark in &row_marks.dynamics {
                let sign = dynamic_glyph(mark.dynamic);
                let width = glyph_box(sign, DYNAMIC_SIZE).width();
                found.push((
                    plan.index,
                    sign,
                    Request {
                        desired: column_x(bar, mark.column) + notehead_width().half()
                            - width.half(),
                        width,
                        least: width,
                        back: width,
                    },
                ));
            }
        }
        let requests: Vec<Request> = found.iter().map(|(_, _, request)| *request).collect();
        for ((bar, sign, _), slot) in found.iter().zip(place(&requests, marks.left, marks.right)) {
            let Some(slot) = slot else {
                out.drop_mark(*bar, MarkKind::Dynamic);
                continue;
            };
            {
                let ink = glyph_box(*sign, DYNAMIC_SIZE);
                out.items.push(glyph(
                    *sign,
                    Point {
                        x: slot.left - ink.min.x,
                        y: baseline,
                    },
                    DYNAMIC_SIZE,
                ));
            }
        }
    }
}

pub fn draw_marks(marks: &SystemMarks<'_>, out: &mut MarkOutput) {
    draw_tempos(marks, out);
    draw_sections(marks, out);
    draw_chords(marks, out);
    draw_free_texts(marks, out);
    draw_dynamics(marks, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(desired: i64, width: i64, least: i64) -> Request {
        Request {
            desired: Units(desired),
            width: Units(width),
            least: Units(least),
            back: Units(width),
        }
    }

    #[test]
    fn placed_items_keep_their_place_when_free_and_never_overlap_or_leave_the_bounds() {
        let bounds = (Units(0), Units(100_000));
        let free = place(
            &[request(10_000, 5_000, 5_000), request(30_000, 5_000, 5_000)],
            bounds.0,
            bounds.1,
        );
        assert_eq!(
            free,
            vec![
                Some(Slot {
                    left: Units(10_000),
                    width: Units(5_000)
                }),
                Some(Slot {
                    left: Units(30_000),
                    width: Units(5_000)
                })
            ]
        );
        let gap = MARK_GAP.0;
        let crowded = place(
            &[
                request(10_000, 30_000, 30_000),
                request(12_000, 30_000, 1_000),
                request(99_000, 30_000, 30_000),
            ],
            bounds.0,
            bounds.1,
        );
        assert_eq!(crowded[0].unwrap().left, Units(10_000));
        assert_eq!(crowded[1].unwrap().left, Units(40_000 + gap));
        assert_eq!(crowded[1].unwrap().width, Units(30_000));
        assert_eq!(crowded[2], None);
        let clipped = place(&[request(90_000, 30_000, 1_000)], bounds.0, bounds.1);
        assert_eq!(
            clipped[0],
            Some(Slot {
                left: Units(70_000),
                width: Units(30_000)
            })
        );
        let squeezed = place(
            &[
                request(60_000, 30_000, 30_000),
                request(65_000, 30_000, 1_000),
            ],
            bounds.0,
            bounds.1,
        );
        assert_eq!(squeezed[1].unwrap().left, Units(90_000 + gap));
        assert_eq!(squeezed[1].unwrap().width, Units(10_000 - gap));
    }
}
