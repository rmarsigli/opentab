use opentab_model::{Slide, StrokeDirection};

use super::bend::{ARROW_SIZE, bend_rects};
use super::label::{LABEL_PAD, baseline_below_centre, digit_height, measure};
use super::{FINGER_EM, FretPlan, LinkKind, MARK_EM, TabBeat};
use crate::display::{
    Colour, FilledRect, Font, Line, PathSegment, PathStyle, Primitive, Shape, TextAnchor, TextRun,
};
use crate::glyphs::{ARROWHEAD_BLACK_DOWN, ARROWHEAD_BLACK_UP, Glyph, Metrics};
use crate::notation::{Ink, TIE_HEIGHT, TIE_OFFSET};
use crate::rhythm::{glyph, glyph_box};
use crate::units::{Point, Rect, STAFF_SPACE, Units};

pub const SLIDE_LENGTH: Units = STAFF_SPACE;
pub const SLIDE_RISE: Units = Units(STAFF_SPACE.0 * 7 / 20);
pub const SLIDE_GAP: Units = Units(STAFF_SPACE.0 / 8);
pub const LINK_ROOM: Units = Units(STAFF_SPACE.0 * 3 / 5);
pub const STROKE_GAP: Units = Units(STAFF_SPACE.0 * 3 / 10);
pub const STROKE_REACH: Units = Units(STAFF_SPACE.0 * 3 / 5);
pub const HAMMER_EM: Units = STAFF_SPACE;
pub const LETTER_GAP: Units = Units(STAFF_SPACE.0 / 8);
pub const LABEL_ASCENT: i64 = 6;
pub const LABEL_ASCENT_PARTS: i64 = 5;
pub const TAB_FINGER_GAP: Units = Units(STAFF_SPACE.0 / 10);

pub fn effect_thickness() -> Units {
    Metrics::bravura().engraving_defaults().stem_thickness
}

pub fn text_height(em: Units) -> Units {
    Units(digit_height(em).0.saturating_mul(LABEL_ASCENT) / LABEL_ASCENT_PARTS)
}

pub fn hammer_top(fret: &FretPlan) -> Units {
    fret.label().min.y - TIE_OFFSET - TIE_HEIGHT - LETTER_GAP - text_height(HAMMER_EM)
}

pub fn arrow_box(direction: Glyph) -> Rect {
    glyph_box(direction, ARROW_SIZE)
}

pub fn arrow_up(apex: Point) -> (Point, Units) {
    let rect = arrow_box(ARROWHEAD_BLACK_UP);
    let origin = Point {
        x: apex.x - rect.width().half(),
        y: apex.y - rect.min.y,
    };
    (origin, apex.y + rect.height())
}

pub fn arrow_down(apex: Point) -> (Point, Units) {
    let rect = arrow_box(ARROWHEAD_BLACK_DOWN);
    let origin = Point {
        x: apex.x - rect.width().half(),
        y: apex.y - rect.max.y,
    };
    (origin, apex.y - rect.height())
}

pub fn text(text: &str, origin: Point, em: Units, anchor: TextAnchor) -> Primitive {
    Primitive::Text(TextRun {
        origin,
        text: text.to_string(),
        font: Font::Text,
        em,
        anchor,
    })
}

pub fn white(rect: Rect) -> Primitive {
    Primitive::FilledRect(FilledRect {
        rect,
        colour: Colour::White,
    })
}

pub fn line(from: Point, to: Point) -> Primitive {
    Primitive::Line(Line {
        from,
        to,
        thickness: effect_thickness(),
        dash: None,
    })
}

pub fn curve(segments: Vec<PathSegment>) -> Primitive {
    Primitive::Shape(Shape {
        segments,
        style: PathStyle::Stroke(effect_thickness()),
    })
}

pub fn slide_segment(fret: &FretPlan, slide: Slide) -> Option<(Point, Point)> {
    let label = fret.label();
    let before_end = label.min.x - SLIDE_GAP;
    let before_start = before_end - SLIDE_LENGTH;
    let after_start = label.max.x + SLIDE_GAP;
    let after_end = after_start + SLIDE_LENGTH;
    let low = fret.y + SLIDE_RISE;
    let high = fret.y - SLIDE_RISE;
    let point = |x: Units, y: Units| Point { x, y };
    match slide {
        Slide::IntoFromBelow => Some((point(before_start, low), point(before_end, high))),
        Slide::IntoFromAbove => Some((point(before_start, high), point(before_end, low))),
        Slide::OutDownwards => Some((point(after_start, high), point(after_end, low))),
        Slide::OutUpwards => Some((point(after_start, low), point(after_end, high))),
        Slide::Shift | Slide::Legato => None,
    }
}

pub fn finger_rect(fret: &FretPlan, finger: &str) -> Rect {
    let label = fret.label();
    let width = measure(finger, FINGER_EM);
    let bottom = fret.y - LABEL_PAD;
    Rect::new(
        Point {
            x: label.min.x - width - TAB_FINGER_GAP,
            y: bottom - digit_height(FINGER_EM) - TAB_FINGER_GAP,
        },
        Point {
            x: label.min.x,
            y: bottom,
        },
    )
}

pub fn draw_finger(fret: &FretPlan, finger: &str, at: Point, items: &mut Vec<Primitive>) {
    let rect = finger_rect(fret, finger);
    items.push(white(Rect::new(rect.min.offset(at), rect.max.offset(at))));
    items.push(text(
        finger,
        Point {
            x: at.x + rect.max.x - TAB_FINGER_GAP,
            y: at.y + rect.max.y,
        },
        FINGER_EM,
        TextAnchor::End,
    ));
}

pub struct StrokeGeometry {
    pub x: Units,
    pub top: Units,
    pub bottom: Units,
}

pub fn stroke_geometry(beat: &TabBeat) -> Option<StrokeGeometry> {
    beat.stroke?;
    let left = beat.frets.iter().map(|fret| fret.label().min.x).min()?;
    let top = beat.frets.iter().map(|fret| fret.y).min()? - STROKE_REACH;
    let bottom = beat.frets.iter().map(|fret| fret.y).max()? + STROKE_REACH;
    let half = arrow_box(ARROWHEAD_BLACK_UP).width().half();
    Some(StrokeGeometry {
        x: left - STROKE_GAP - half,
        top,
        bottom,
    })
}

pub fn draw_stroke(beat: &TabBeat, at: Point, items: &mut Vec<Primitive>) {
    let (Some(direction), Some(geometry)) = (beat.stroke, stroke_geometry(beat)) else {
        return;
    };
    let x = at.x + geometry.x;
    let top = at.y + geometry.top;
    let bottom = at.y + geometry.bottom;
    match direction {
        StrokeDirection::Down => {
            let (head, base) = arrow_up(Point { x, y: top });
            items.push(line(Point { x, y: base }, Point { x, y: bottom }));
            items.push(glyph(ARROWHEAD_BLACK_UP, head, ARROW_SIZE));
        }
        StrokeDirection::Up => {
            let (head, tail) = arrow_down(Point { x, y: bottom });
            items.push(line(Point { x, y: top }, Point { x, y: tail }));
            items.push(glyph(ARROWHEAD_BLACK_DOWN, head, ARROW_SIZE));
        }
    }
}

pub fn label_baseline(fret: &FretPlan) -> Units {
    fret.y + baseline_below_centre(fret.em)
}

pub fn draw_label(fret: &FretPlan, at: Point, items: &mut Vec<Primitive>) {
    let label = fret.label();
    items.push(white(Rect::new(label.min.offset(at), label.max.offset(at))));
    items.push(text(
        &fret.text,
        Point {
            x: at.x + fret.dx,
            y: at.y + label_baseline(fret),
        },
        fret.em,
        TextAnchor::Middle,
    ));
}

fn fret_rects(fret: &FretPlan, alone: bool) -> Vec<Rect> {
    let label = fret.label();
    let mut rects = vec![label];
    if let Some(finger) = fret.finger {
        rects.push(finger_rect(fret, finger));
    }
    if let Some((from, to)) = fret.slide.and_then(|slide| slide_segment(fret, slide)) {
        rects.push(Rect::new(from, from).including(to));
    }
    if let Some(link) = fret.link {
        rects.push(Rect::new(
            label.min,
            Point {
                x: label.max.x + LINK_ROOM,
                y: label.max.y,
            },
        ));
        if link.kind == LinkKind::Hammer {
            rects.push(Rect::new(
                Point {
                    x: label.min.x,
                    y: hammer_top(fret),
                },
                label.max,
            ));
        }
    }
    if fret
        .incoming
        .is_some_and(|link| link.kind == LinkKind::Hammer)
    {
        rects.push(Rect::new(
            Point {
                x: label.min.x,
                y: hammer_top(fret),
            },
            label.max,
        ));
    }
    if alone && (fret.tie_origin || fret.tie_destination) {
        rects.push(Rect::new(
            label.min,
            Point {
                x: label.max.x,
                y: label.max.y + TIE_OFFSET + TIE_HEIGHT,
            },
        ));
    }
    rects
}

pub fn beat_ink(beat: &TabBeat, alone: bool) -> Option<Ink> {
    let mut rects: Vec<Rect> = beat
        .frets
        .iter()
        .flat_map(|fret| fret_rects(fret, alone))
        .collect();
    rects.extend(bend_rects(&beat.bends));
    if let Some(mark) = &beat.mark {
        let half = measure(mark, MARK_EM).half();
        rects.push(Rect::new(
            Point {
                x: -half,
                y: Units::ZERO,
            },
            Point {
                x: half,
                y: Units::ZERO,
            },
        ));
    }
    if let Some(geometry) = stroke_geometry(beat) {
        let half = arrow_box(ARROWHEAD_BLACK_UP).width().half();
        rects.push(Rect::new(
            Point {
                x: geometry.x - half,
                y: geometry.top,
            },
            Point {
                x: geometry.x + half,
                y: geometry.bottom,
            },
        ));
    }
    let shift = Point {
        x: beat.centre,
        y: Units::ZERO,
    };
    let (first, rest) = rects.split_first()?;
    let mut ink = Ink::of(Rect::new(first.min.offset(shift), first.max.offset(shift)));
    for rect in rest {
        ink.add(Rect::new(rect.min.offset(shift), rect.max.offset(shift)));
    }
    Some(ink)
}
