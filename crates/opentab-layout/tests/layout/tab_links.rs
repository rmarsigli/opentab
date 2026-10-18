use opentab_layout::{Line, PathSegment, PathStyle, Point, StaffMode};
use opentab_model::{Score, Slide};

use crate::music::{build, measure, note_at, options, render};
use crate::support::systems;
use crate::tab::tab_rows;
use crate::tab_effects::{lines, quarters, shapes, words};

const BARS: usize = 24;

fn chained(kind: Option<Slide>) -> Score {
    let measures: Vec<_> = (0..BARS)
        .map(|_| {
            measure(
                (4, 4),
                0,
                vec![(0, quarters(&[(2, 5), (2, 5), (2, 5), (2, 7)]))],
            )
        })
        .collect();
    let mut score = build(&measures);
    for bar in 0..BARS - 1 {
        let note = note_at(&mut score, bar, 0, 3, 0);
        match kind {
            Some(slide) => note.effects.slide = Some(slide),
            None => note.effects.hammer_on_pull_off = true,
        }
    }
    score
}

fn arc_start(items: &[opentab_layout::Primitive]) -> Vec<Point> {
    shapes(items)
        .into_iter()
        .filter(|shape| shape.style == PathStyle::Fill)
        .filter_map(|shape| match shape.segments.first() {
            Some(PathSegment::MoveTo(point)) => Some(*point),
            _ => None,
        })
        .collect()
}

#[test]
fn a_hammer_on_across_a_system_break_draws_both_halves() {
    let score = chained(None);
    let list = render(&score, &options(StaffMode::Both));
    let count = systems(&list).len();
    assert!(count >= 2);
    let rows = tab_rows(&list);
    let mut letters = 0;
    for (index, (_, system, row)) in rows.iter().enumerate() {
        for bar in &row.bars {
            letters += words(&bar.items).iter().filter(|word| *word == "P").count();
        }
        if index > 0 {
            let first = &row.staff.bars[0];
            let starts = arc_start(&row.bars[0].items);
            assert!(
                starts.iter().any(|point| point.x == first.content),
                "system {} opens with the incoming half arc",
                system.index
            );
        }
        if index + 1 < rows.len() {
            let last = row.bars.last().unwrap();
            let ends: Vec<_> = shapes(&last.items)
                .into_iter()
                .filter(|shape| shape.style == PathStyle::Fill)
                .filter_map(|shape| match shape.segments.get(1) {
                    Some(PathSegment::CubicTo(_, _, end)) => Some(end.x),
                    _ => None,
                })
                .collect();
            assert!(
                ends.iter()
                    .any(|x| *x > row.staff.bars.last().unwrap().columns[3].x),
                "system {} ends with the outgoing half arc",
                system.index
            );
        }
    }
    assert_eq!(
        letters,
        BARS - 1 + count - 1,
        "one letter per link plus one per break"
    );
}

#[test]
fn a_shift_slide_across_a_system_break_draws_both_halves() {
    let score = chained(Some(Slide::Shift));
    let list = render(&score, &options(StaffMode::Both));
    let rows = tab_rows(&list);
    assert!(rows.len() >= 2);
    let diagonal = |line: &Line| line.from.x != line.to.x && line.from.y != line.to.y;
    let mut total = 0;
    for (index, (_, _, row)) in rows.iter().enumerate() {
        for bar in &row.bars {
            total += lines(&bar.items)
                .iter()
                .filter(|line| diagonal(line))
                .count();
        }
        if index > 0 {
            let first = &row.staff.bars[0];
            assert!(
                lines(&row.bars[0].items)
                    .iter()
                    .any(|line| diagonal(line) && line.from.x == first.content),
                "the incoming slide starts at the bar content"
            );
        }
        if index + 1 < rows.len() {
            let edge = row.staff.bars.last().unwrap().rect.max.x;
            assert!(
                lines(&row.bars.last().unwrap().items)
                    .iter()
                    .any(|line| diagonal(line)
                        && line.to.x < edge
                        && line.to.x > edge - opentab_layout::STAFF_SPACE),
                "the outgoing slide runs to the system edge"
            );
        }
    }
    assert_eq!(total, BARS - 1 + rows.len() - 1);
}
