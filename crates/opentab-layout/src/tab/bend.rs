use std::collections::BTreeSet;
use std::num::NonZeroI64;

use opentab_model::Bend;

use super::FretPlan;
use super::effects::{arrow_box, arrow_down, arrow_up, curve, line, text, text_height};
use super::label::{bend_text, measure};
use crate::display::{PathSegment, Primitive, TextAnchor};
use crate::glyphs::{ARROWHEAD_BLACK_DOWN, ARROWHEAD_BLACK_UP};
use crate::rhythm::glyph;
use crate::units::{Point, Rect, STAFF_SPACE, Units};

pub const BEND_WIDTH: Units = Units(STAFF_SPACE.0 * 5 / 4);
pub const BEND_RISE: Units = Units(STAFF_SPACE.0 * 3 / 2);
pub const BEND_TIER: Units = Units(STAFF_SPACE.0 * 2);
pub const BEND_STAGGER: Units = Units(STAFF_SPACE.0 * 2);
pub const DIP_DEPTH: Units = Units(STAFF_SPACE.0 * 5 / 4);
pub const BEND_LABEL_GAP: Units = Units(STAFF_SPACE.0 / 5);
pub const BEND_EM: Units = Units(STAFF_SPACE.0 * 11 / 10);
const QUARTERS: NonZeroI64 = NonZeroI64::new(4).unwrap();
pub const ARROW_SIZE: Units = Units(STAFF_SPACE.0 * 3 / 5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BendShape {
    pub origin: i64,
    pub middle: i64,
    pub destination: i64,
    pub held: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Heading {
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BendSegment {
    pub from: Point,
    pub to: Point,
    pub straight: bool,
    pub heading: Heading,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BendDraw {
    pub segments: Vec<BendSegment>,
}

struct Group {
    shape: BendShape,
    fret: usize,
}

struct Tiers {
    rising: Vec<i64>,
    falling: Vec<i64>,
}

pub fn shape_of(bend: &Bend, tied: bool) -> Option<BendShape> {
    let origin = i64::from(bend.origin().value());
    let middle = i64::from(bend.middle_value());
    let destination = i64::from(bend.destination().value());
    if origin == 0 && middle == 0 && destination == 0 {
        return None;
    }
    Some(BendShape {
        origin,
        middle,
        destination,
        held: tied && origin != 0,
    })
}

fn rank(values: &[i64], value: i64) -> i64 {
    let found = values.iter().position(|known| *known == value).unwrap_or(0);
    i64::try_from(found).unwrap_or(i64::MAX)
}

fn tiers(groups: &[Group]) -> Tiers {
    let mut rising = BTreeSet::new();
    let mut falling = BTreeSet::new();
    for group in groups {
        let shape = group.shape;
        for value in [shape.origin, shape.middle, shape.destination] {
            if value > 0 {
                rising.insert(value);
            } else if value < 0 {
                falling.insert(-value);
            }
        }
    }
    Tiers {
        rising: rising.into_iter().collect(),
        falling: falling.into_iter().collect(),
    }
}

fn level(tiers: &Tiers, fret: &FretPlan, value: i64) -> Units {
    if value > 0 {
        -BEND_RISE - Units(BEND_TIER.0.saturating_mul(rank(&tiers.rising, value)))
    } else if value < 0 {
        fret.label().max.y
            + DIP_DEPTH
            + Units(BEND_TIER.0.saturating_mul(rank(&tiers.falling, -value)))
    } else {
        fret.y
    }
}

fn heading(from: i64, to: i64) -> Heading {
    if to > from {
        Heading::Up
    } else {
        Heading::Down
    }
}

fn label(value: i64) -> Option<String> {
    (value != 0).then(|| bend_text(value))
}

fn segments(group: &Group, fret: &FretPlan, tiers: &Tiers, offset: Units) -> Vec<BendSegment> {
    let shape = group.shape;
    let box_ = fret.label();
    let mut out = Vec::new();
    let mut value = 0;
    let mut point = Point {
        x: box_.max.x,
        y: fret.y,
    };
    if shape.origin != 0 {
        value = shape.origin;
        let y = level(tiers, fret, value);
        if shape.held {
            point = Point { x: box_.max.x, y };
        } else {
            let x = fret.dx + offset;
            let start = if value > 0 { box_.min.y } else { box_.max.y };
            point = Point { x, y };
            out.push(BendSegment {
                from: Point { x, y: start },
                to: point,
                straight: true,
                heading: heading(0, value),
                label: label(value),
            });
        }
    }
    let mut first = true;
    for next in [shape.middle, shape.destination] {
        if next == value {
            continue;
        }
        let width = if first {
            BEND_WIDTH + offset
        } else {
            BEND_WIDTH
        };
        first = false;
        let to = Point {
            x: point.x.max(box_.max.x) + width,
            y: level(tiers, fret, next),
        };
        out.push(BendSegment {
            from: point,
            to,
            straight: false,
            heading: heading(value, next),
            label: label(next),
        });
        point = to;
        value = next;
    }
    out
}

pub fn plan_bends(frets: &[FretPlan]) -> Vec<BendDraw> {
    let mut groups: Vec<Group> = Vec::new();
    for (index, fret) in frets.iter().enumerate() {
        let Some(shape) = fret.bend else {
            continue;
        };
        if !groups.iter().any(|group| group.shape == shape) {
            groups.push(Group { shape, fret: index });
        }
    }
    let tiers = tiers(&groups);
    groups
        .iter()
        .enumerate()
        .filter_map(|(order, group)| {
            let fret = frets.get(group.fret)?;
            let offset = Units(BEND_STAGGER.0.saturating_mul(i64::try_from(order).ok()?));
            Some(BendDraw {
                segments: segments(group, fret, &tiers, offset),
            })
        })
        .collect()
}

pub fn label_rect(segment: &BendSegment) -> Option<Rect> {
    let text = segment.label.as_ref()?;
    let half = measure(text, BEND_EM).half();
    let height = text_height(BEND_EM);
    let (top, bottom) = match segment.heading {
        Heading::Up => (
            segment.to.y - BEND_LABEL_GAP - height,
            segment.to.y - BEND_LABEL_GAP,
        ),
        Heading::Down => (
            segment.to.y + BEND_LABEL_GAP,
            segment.to.y + BEND_LABEL_GAP + height,
        ),
    };
    Some(Rect::new(
        Point {
            x: segment.to.x - half,
            y: top,
        },
        Point {
            x: segment.to.x + half,
            y: bottom,
        },
    ))
}

pub fn bend_rects(bends: &[BendDraw]) -> Vec<Rect> {
    let mut rects = Vec::new();
    let arrow = arrow_box(ARROWHEAD_BLACK_UP);
    let half = arrow.width().half();
    let height = arrow.height();
    for segment in bends.iter().flat_map(|bend| &bend.segments) {
        rects.push(Rect::new(segment.from, segment.from).including(segment.to));
        rects.push(Rect::new(
            Point {
                x: segment.to.x - half,
                y: segment.to.y - height,
            },
            Point {
                x: segment.to.x + half,
                y: segment.to.y + height,
            },
        ));
        rects.extend(label_rect(segment));
    }
    rects
}

fn shaft(segment: &BendSegment, end: Point) -> Primitive {
    if segment.straight {
        return line(segment.from, end);
    }
    let from = segment.from;
    let (first, second) = match segment.heading {
        Heading::Up => (
            Point {
                x: from.x + (end.x - from.x).scaled(3, QUARTERS),
                y: from.y,
            },
            Point {
                x: end.x,
                y: from.y + (end.y - from.y).half(),
            },
        ),
        Heading::Down => (
            Point {
                x: from.x + (end.x - from.x).half(),
                y: from.y,
            },
            Point {
                x: end.x,
                y: from.y + (end.y - from.y).half().half(),
            },
        ),
    };
    curve(vec![
        PathSegment::MoveTo(from),
        PathSegment::CubicTo(first, second, end),
    ])
}

pub fn draw_bends(bends: &[BendDraw], at: Point, items: &mut Vec<Primitive>) {
    for segment in bends.iter().flat_map(|bend| &bend.segments) {
        let shifted = BendSegment {
            from: segment.from.offset(at),
            to: segment.to.offset(at),
            ..segment.clone()
        };
        let (head, glyph_name, end) = match shifted.heading {
            Heading::Up => {
                let (origin, base) = arrow_up(shifted.to);
                (
                    origin,
                    ARROWHEAD_BLACK_UP,
                    Point {
                        x: shifted.to.x,
                        y: base,
                    },
                )
            }
            Heading::Down => {
                let (origin, tail) = arrow_down(shifted.to);
                (
                    origin,
                    ARROWHEAD_BLACK_DOWN,
                    Point {
                        x: shifted.to.x,
                        y: tail,
                    },
                )
            }
        };
        items.push(shaft(&shifted, end));
        items.push(glyph(glyph_name, head, ARROW_SIZE));
        if let (Some(text_value), Some(rect)) = (&shifted.label, label_rect(&shifted)) {
            items.push(text(
                text_value,
                Point {
                    x: shifted.to.x,
                    y: rect.max.y,
                },
                BEND_EM,
                TextAnchor::Middle,
            ));
        }
    }
}
