use std::collections::BTreeMap;
use std::num::NonZeroI64;

use opentab_model::Vibrato;

use super::effects::{LETTER_GAP, SLIDE_GAP, SLIDE_RISE, line, text};
use super::{ColumnMarks, HAMMER_EM, Link, LinkKind, MARK_EM, TabNotation};
use crate::display::{Dash, Line, Primitive, TextAnchor};
use crate::glyphs::{Glyph, Metrics, WIGGLE_VIBRATO, WIGGLE_VIBRATO_WIDE};
use crate::lanes::{BandLayout, Lane, LanePlacement};
use crate::notation::TIE_HEIGHT;
use crate::rhythm::{glyph, glyph_box};
use crate::structure::BarLayout;
use crate::ties::{MIN_TIE_HEIGHT, TIE_SHARE, TieAnchor, TieSpan, draw_ties, tie};
use crate::units::{Point, STAFF_SPACE, Units};

pub const LANE_END_GAP: Units = Units(STAFF_SPACE.0 / 4);
pub const LANE_TEXT_GAP: Units = Units(STAFF_SPACE.0 / 4);
pub const LANE_LINE_RISE: Units = Units(STAFF_SPACE.0 * 3 / 10);
pub const LANE_HOOK: Units = Units(STAFF_SPACE.0 * 2 / 5);
pub const LANE_DASH: Dash = Dash {
    on: Units(STAFF_SPACE.0 * 2 / 5),
    off: Units(STAFF_SPACE.0 * 3 / 10),
};
pub const WIGGLE_INSET: Units = Units(STAFF_SPACE.0 / 8);
pub const LET_RING: &str = "let ring";
pub const PALM_MUTE: &str = "P.M.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkAnchor {
    pub bar: usize,
    pub voice: u8,
    pub sequence: usize,
    pub line: usize,
    pub left: Units,
    pub right: Units,
    pub centre: Units,
    pub y: Units,
    pub top: Units,
    pub link: Option<Link>,
    pub incoming: Option<Link>,
    pub size: Units,
}

pub struct TextLane {
    pub lane: Lane,
    pub label: &'static str,
    pub flag: fn(&ColumnMarks) -> bool,
}

pub fn text_lanes() -> [TextLane; 2] {
    [
        TextLane {
            lane: Lane::LetRing,
            label: LET_RING,
            flag: |marks| marks.let_ring,
        },
        TextLane {
            lane: Lane::PalmMute,
            label: PALM_MUTE,
            flag: |marks| marks.palm_mute,
        },
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowEnds {
    pub incoming: Option<Units>,
    pub outgoing: Option<Units>,
    pub last_bar: usize,
}

pub struct RowParts<'a> {
    pub tabs: &'a [Option<&'a TabNotation>],
    pub bars: &'a [BarLayout],
    pub band: &'a BandLayout,
    pub top: Units,
    pub next: Option<ColumnMarks>,
    pub links: &'a [LinkAnchor],
    pub sequences: &'a BTreeMap<u8, usize>,
    pub ties: &'a [TieAnchor],
    pub tie_sequences: &'a BTreeMap<u8, usize>,
}

fn arc_height(length: Units) -> Units {
    Units(length.0 / TIE_SHARE).clamp(MIN_TIE_HEIGHT, TIE_HEIGHT)
}

fn letter(link: Link) -> &'static str {
    match link.to {
        Some(to) if to < link.from => "P",
        _ => "H",
    }
}

pub struct LinkSpan {
    pub link: Link,
    pub arc_from: Units,
    pub arc_to: Units,
    pub line_from: Units,
    pub line_to: Units,
    pub top: Units,
    pub y: Units,
    pub size: Units,
}

fn draw_span(span: &LinkSpan, out: &mut Vec<Primitive>) {
    match span.link.kind {
        LinkKind::Hammer => {
            let from = span.arc_from;
            let to = span.arc_to.max(from + STAFF_SPACE.half());
            out.push(tie(from, to, span.top, true, span.size));
            let peak = span.top - crate::notation::TIE_OFFSET - arc_height(to - from) - LETTER_GAP;
            out.push(text(
                letter(span.link),
                Point {
                    x: from + (to - from).half(),
                    y: peak,
                },
                HAMMER_EM,
                TextAnchor::Middle,
            ));
        }
        LinkKind::Slide => {
            let start = span.line_from;
            let stop = span.line_to.max(start + SLIDE_GAP);
            let rising = span.link.to.is_none_or(|to| to >= span.link.from);
            let (first, second) = if rising {
                (span.y + SLIDE_RISE, span.y - SLIDE_RISE)
            } else {
                (span.y - SLIDE_RISE, span.y + SLIDE_RISE)
            };
            out.push(line(
                Point { x: start, y: first },
                Point { x: stop, y: second },
            ));
        }
    }
}

fn outgoing(origin: &LinkAnchor, link: Link, end: Units, until: Units) -> LinkSpan {
    LinkSpan {
        link,
        arc_from: origin.centre,
        arc_to: end,
        line_from: origin.right + SLIDE_GAP,
        line_to: until - SLIDE_GAP,
        top: origin.top,
        y: origin.y,
        size: origin.size,
    }
}

fn incoming(target: &LinkAnchor, link: Link, edge: Units) -> LinkSpan {
    LinkSpan {
        link,
        arc_from: edge,
        arc_to: target.centre,
        line_from: edge,
        line_to: target.left - SLIDE_GAP,
        top: target.top,
        y: target.y,
        size: target.size,
    }
}

fn draw_links(parts: &RowParts<'_>, ends: RowEnds, contents: &mut [Vec<Primitive>]) {
    for origin in parts.links {
        let mut drawn = Vec::new();
        if let (Some(link), Some(edge), 0, 0) =
            (origin.incoming, ends.incoming, origin.bar, origin.sequence)
        {
            draw_span(&incoming(origin, link, edge), &mut drawn);
        }
        if let Some(link) = origin.link {
            let destination = parts.links.iter().find(|candidate| {
                candidate.voice == origin.voice
                    && candidate.sequence == origin.sequence + 1
                    && candidate.line == origin.line
            });
            match destination {
                Some(target) => draw_span(
                    &outgoing(origin, link, target.centre, target.left),
                    &mut drawn,
                ),
                None => {
                    let last = parts.sequences.get(&origin.voice).copied().unwrap_or(0);
                    let leaves = origin.bar == ends.last_bar && origin.sequence + 1 == last;
                    if let (true, Some(_), Some(edge)) = (leaves, link.to, ends.outgoing) {
                        draw_span(&outgoing(origin, link, edge, edge), &mut drawn);
                    }
                }
            }
        }
        if let Some(group) = contents.get_mut(origin.bar) {
            group.extend(drawn);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cell<K> {
    bar: usize,
    start: Units,
    end: Units,
    kind: Option<K>,
    last: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Run<K> {
    kind: K,
    cells: Vec<Cell<K>>,
    open: bool,
}

fn cells<K: Copy>(parts: &RowParts<'_>, pick: &dyn Fn(&ColumnMarks) -> Option<K>) -> Vec<Cell<K>> {
    let mut out = Vec::new();
    let count = parts.bars.len();
    for (position, (bar, tab)) in parts.bars.iter().zip(parts.tabs).enumerate() {
        let marks = tab.map_or(&[][..], |found| found.marks.as_slice());
        if bar.columns.is_empty() {
            out.push(Cell {
                bar: position,
                start: bar.rect.min.x,
                end: bar.rect.max.x,
                kind: None,
                last: position + 1 == count,
            });
        }
        for (index, column) in bar.columns.iter().enumerate() {
            let end = bar
                .columns
                .get(index + 1)
                .map_or(bar.rect.max.x, |next| next.x);
            out.push(Cell {
                bar: position,
                start: column.x,
                end,
                kind: marks.get(index).and_then(pick),
                last: position + 1 == count && index + 1 == bar.columns.len(),
            });
        }
    }
    out
}

fn runs<K: Copy + PartialEq>(
    parts: &RowParts<'_>,
    pick: &dyn Fn(&ColumnMarks) -> Option<K>,
) -> Vec<Run<K>> {
    let mut out: Vec<Run<K>> = Vec::new();
    let mut current: Option<Run<K>> = None;
    for cell in cells(parts, pick) {
        match (cell.kind, current.as_mut()) {
            (Some(kind), Some(run)) if run.kind == kind => run.cells.push(cell),
            (Some(kind), _) => {
                out.extend(current.take());
                current = Some(Run {
                    kind,
                    cells: vec![cell],
                    open: false,
                });
            }
            (None, _) => out.extend(current.take()),
        }
    }
    if let Some(mut run) = current.take() {
        let next = parts.next.as_ref().and_then(pick);
        run.open = run.cells.last().is_some_and(|cell| cell.last) && next == Some(run.kind);
        out.push(run);
    }
    out
}

fn text_run(
    parts: &RowParts<'_>,
    lane: LanePlacement,
    label: &str,
    run: &Run<()>,
    contents: &mut [Vec<Primitive>],
) {
    let (Some(first), Some(last)) = (run.cells.first(), run.cells.last()) else {
        return;
    };
    let baseline = parts.top + lane.bottom;
    let mut drawn = vec![text(
        label,
        Point {
            x: first.start,
            y: baseline,
        },
        MARK_EM,
        TextAnchor::Start,
    )];
    let from = first.start + super::measure(label, MARK_EM) + LANE_TEXT_GAP;
    let to = if run.open {
        last.end
    } else {
        last.end - LANE_END_GAP
    };
    let y = baseline - LANE_LINE_RISE;
    if to > from {
        drawn.push(Primitive::Line(Line {
            from: Point { x: from, y },
            to: Point { x: to, y },
            thickness: super::effects::effect_thickness(),
            dash: Some(LANE_DASH),
        }));
        if !run.open {
            drawn.push(line(
                Point { x: to, y },
                Point {
                    x: to,
                    y: y + LANE_HOOK,
                },
            ));
        }
    }
    if let Some(group) = contents.get_mut(first.bar) {
        group.extend(drawn);
    }
}

fn wiggle(kind: Vibrato) -> Glyph {
    match kind {
        Vibrato::Slight => WIGGLE_VIBRATO,
        Vibrato::Wide => WIGGLE_VIBRATO_WIDE,
    }
}

fn vibrato_run(
    parts: &RowParts<'_>,
    lane: LanePlacement,
    run: &Run<Vibrato>,
    contents: &mut [Vec<Primitive>],
) {
    let shape = wiggle(run.kind);
    let ink = glyph_box(shape, STAFF_SPACE);
    let advance = Metrics::bravura()
        .advance(shape.name)
        .filter(|advance| *advance > Units::ZERO)
        .unwrap_or(STAFF_SPACE);
    let Some(step) = NonZeroI64::new(advance.0) else {
        return;
    };
    let y = parts.top + lane.bottom;
    let mut index = 0;
    while index < run.cells.len() {
        let bar = run.cells[index].bar;
        let mut end = index;
        while end + 1 < run.cells.len() && run.cells[end + 1].bar == bar {
            end += 1;
        }
        let Some(layout) = parts.bars.get(bar) else {
            index = end + 1;
            continue;
        };
        let start = run.cells[index].start.max(layout.rect.min.x + WIGGLE_INSET);
        let limit = layout.rect.max.x - WIGGLE_INSET;
        let stop = run.cells[end].end.min(limit);
        let origin = start - ink.min.x;
        let room = stop - origin - ink.max.x;
        let mut count = if room < Units::ZERO {
            0
        } else {
            room.0 / step.get() + 1
        };
        if count == 0 && origin + ink.max.x <= limit {
            count = 1;
        }
        if let Some(group) = contents.get_mut(bar) {
            for repeat in 0..count {
                group.push(glyph(
                    shape,
                    Point {
                        x: origin + Units(advance.0.saturating_mul(repeat)),
                        y,
                    },
                    STAFF_SPACE,
                ));
            }
        }
        index = end + 1;
    }
}

fn draw_lanes(parts: &RowParts<'_>, contents: &mut [Vec<Primitive>]) {
    if let Some(lane) = parts.band.lane(Lane::Vibrato) {
        for run in runs(parts, &|marks: &ColumnMarks| marks.vibrato) {
            vibrato_run(parts, lane, &run, contents);
        }
    }
    let flags = text_lanes();
    for kind in flags {
        let Some(lane) = parts.band.lane(kind.lane) else {
            continue;
        };
        for run in runs(parts, &|marks: &ColumnMarks| {
            (kind.flag)(marks).then_some(())
        }) {
            text_run(parts, lane, kind.label, &run, contents);
        }
    }
}

pub fn finish_row(parts: &RowParts<'_>, ends: RowEnds, contents: &mut [Vec<Primitive>]) {
    let alone = parts.tabs.iter().flatten().any(|tab| tab.alone);
    if alone {
        draw_ties(
            parts.ties,
            TieSpan {
                incoming: ends.incoming,
                outgoing: ends.outgoing,
                last_bar: ends.last_bar,
            },
            parts.tie_sequences,
            contents,
        );
    }
    draw_links(parts, ends, contents);
    draw_lanes(parts, contents);
}
