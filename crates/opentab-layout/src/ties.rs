use std::collections::BTreeMap;
use std::num::NonZeroI64;

use crate::display::{PathSegment, PathStyle, Primitive, Shape};
use crate::glyphs::Metrics;
use crate::notation::{TIE_HEIGHT, TIE_OFFSET};
use crate::rhythm::scale;
use crate::units::{Point, STAFF_SPACE, Units};

pub const TIE_GAP: Units = Units(STAFF_SPACE.0 / 8);
pub const TIE_END_GAP: Units = Units(STAFF_SPACE.0 / 4);
pub const MIN_TIE_HEIGHT: Units = Units(STAFF_SPACE.0 * 2 / 5);
pub const MIN_TIE_LENGTH: Units = Units(STAFF_SPACE.0 / 2);
pub const TIE_SHARE: i64 = 5;
const THIRDS: NonZeroI64 = NonZeroI64::new(3).unwrap();
const QUARTERS: NonZeroI64 = NonZeroI64::new(4).unwrap();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TieAnchor {
    pub bar: usize,
    pub voice: u8,
    pub sequence: usize,
    pub key: i64,
    pub left: Units,
    pub right: Units,
    pub y: Units,
    pub up: bool,
    pub origin: bool,
    pub destination: bool,
    pub size: Units,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TieSpan {
    pub incoming: Option<Units>,
    pub outgoing: Option<Units>,
    pub last_bar: usize,
}

pub fn tie(from: Units, to: Units, y: Units, up: bool, size: Units) -> Primitive {
    let to = to.max(from + scale(MIN_TIE_LENGTH, size));
    let length = to - from;
    let height =
        Units(length.0 / TIE_SHARE).clamp(scale(MIN_TIE_HEIGHT, size), scale(TIE_HEIGHT, size));
    let middle = scale(
        Metrics::bravura()
            .engraving_defaults()
            .tie_midpoint_thickness,
        size,
    );
    let direction = if up { -1 } else { 1 };
    let offset = Units(scale(TIE_OFFSET, size).0 * direction);
    let start = Point {
        x: from,
        y: y + offset,
    };
    let end = Point {
        x: to,
        y: y + offset,
    };
    let outer = Units(height.scaled(4, THIRDS).0 * direction);
    let inner = Units((height - middle).scaled(4, THIRDS).0 * direction);
    let quarter = length.scaled(1, QUARTERS);
    let control = |x: Units, rise: Units| Point {
        x,
        y: y + offset + rise,
    };
    Primitive::Shape(Shape {
        segments: vec![
            PathSegment::MoveTo(start),
            PathSegment::CubicTo(
                control(from + quarter, outer),
                control(to - quarter, outer),
                end,
            ),
            PathSegment::CubicTo(
                control(to - quarter, inner),
                control(from + quarter, inner),
                start,
            ),
            PathSegment::Close,
        ],
        style: PathStyle::Fill,
    })
}

pub fn draw_ties(
    anchors: &[TieAnchor],
    span: TieSpan,
    counts: &BTreeMap<u8, usize>,
    groups: &mut [Vec<Primitive>],
) {
    let mut matched = vec![false; anchors.len()];
    for (index, anchor) in anchors.iter().enumerate() {
        if !anchor.destination {
            continue;
        }
        let source = (0..index).rev().find(|earlier| {
            let candidate = &anchors[*earlier];
            candidate.voice == anchor.voice
                && candidate.key == anchor.key
                && candidate.sequence + 1 == anchor.sequence
        });
        let shape = match source {
            Some(earlier) => {
                matched[earlier] = true;
                let start = &anchors[earlier];
                Some(tie(
                    start.right + TIE_GAP,
                    anchor.left - TIE_GAP,
                    start.y,
                    start.up,
                    start.size,
                ))
            }
            None if anchor.bar == 0 && anchor.sequence == 0 => span.incoming.map(|from| {
                tie(
                    from,
                    anchor.left - TIE_GAP,
                    anchor.y,
                    anchor.up,
                    anchor.size,
                )
            }),
            None => None,
        };
        if let (Some(shape), Some(group)) = (shape, groups.get_mut(anchor.bar)) {
            group.push(shape);
        }
    }
    for (index, anchor) in anchors.iter().enumerate() {
        let last = counts.get(&anchor.voice).copied().unwrap_or(0);
        if !anchor.origin
            || matched[index]
            || anchor.bar != span.last_bar
            || anchor.sequence + 1 != last
        {
            continue;
        }
        if let (Some(to), Some(group)) = (span.outgoing, groups.get_mut(anchor.bar)) {
            group.push(tie(
                anchor.right + TIE_GAP,
                to,
                anchor.y,
                anchor.up,
                anchor.size,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor(bar: usize, sequence: usize, origin: bool, destination: bool) -> TieAnchor {
        let left = i64::try_from(sequence).unwrap() * 5000 + 5000;
        TieAnchor {
            bar,
            voice: 0,
            sequence,
            key: -3,
            left: Units(left),
            right: Units(left + 1000),
            y: Units(2048),
            up: false,
            origin,
            destination,
            size: STAFF_SPACE,
        }
    }

    fn shapes(groups: &[Vec<Primitive>]) -> Vec<usize> {
        groups.iter().map(Vec::len).collect()
    }

    #[test]
    fn ties_join_notes_in_the_system_and_halves_cross_its_edges() {
        let anchors = [
            anchor(0, 0, false, true),
            anchor(0, 1, true, false),
            anchor(1, 2, true, true),
            anchor(1, 4, false, true),
            anchor(1, 5, true, false),
        ];
        let counts = BTreeMap::from([(0, 6)]);
        let mut groups = vec![Vec::new(), Vec::new()];
        let span = TieSpan {
            incoming: Some(Units(3000)),
            outgoing: Some(Units(30000)),
            last_bar: 1,
        };
        draw_ties(&anchors, span, &counts, &mut groups);
        assert_eq!(shapes(&groups), vec![1, 2]);
        let mut alone = vec![Vec::new(), Vec::new()];
        let closed = TieSpan {
            incoming: None,
            outgoing: None,
            last_bar: 1,
        };
        draw_ties(&anchors, closed, &counts, &mut alone);
        assert_eq!(shapes(&alone), vec![0, 1]);
    }

    #[test]
    fn a_tie_curves_away_from_its_side() {
        let Primitive::Shape(down) = tie(Units(0), Units(4000), Units(1000), false, STAFF_SPACE)
        else {
            panic!("a tie is a shape");
        };
        let Primitive::Shape(up) = tie(Units(0), Units(4000), Units(1000), true, STAFF_SPACE)
        else {
            panic!("a tie is a shape");
        };
        let lowest = |shape: &Shape| {
            shape
                .segments
                .iter()
                .filter_map(|segment| match segment {
                    PathSegment::CubicTo(first, _, _) => Some(first.y),
                    _ => None,
                })
                .max()
                .unwrap()
        };
        assert!(lowest(&down) > Units(1000 + 409));
        assert!(lowest(&up) < Units(1000));
    }
}
