use std::cell::RefCell;
use std::collections::BTreeMap;
use std::num::NonZeroI64;

use opentab_layout::{
    DisplayList, FRET_EM, Group, Page, PathSegment, Primitive, Rect, StaffKind, StaffMode,
    SystemLayout, TextRun, Units, baseline_below_centre, fret_text, layout,
};
use opentab_model::{NoteKind, Score};
use opentab_testkit::arb_score;
use proptest::prelude::*;
use proptest::test_runner::TestRunner;

use crate::music::{Row, overlaps, rows, tall};
use crate::support::{check, ink};

const CASES: u32 = 256;
const MIN_LABELS: usize = 3_000;
const MIN_SAME_STRING_PAIRS: usize = 1_000;
const TEXT_ASCENT: i64 = 15;
const TEXT_DESCENT: i64 = 4;
const PARTS: NonZeroI64 = NonZeroI64::new(20).unwrap();

#[derive(Debug, Clone, Copy, Default)]
pub struct Tally {
    pub labels: usize,
    pub same_string_pairs: usize,
}

pub struct Label {
    pub text: TextRun,
    pub white: Rect,
}

pub fn label_of(group: &Group) -> Vec<Label> {
    let mut out = Vec::new();
    let mut white = None;
    for item in &group.items {
        match item {
            Primitive::FilledRect(rect) => white = Some(rect.rect),
            Primitive::Text(text) => {
                if let Some(rect) = white.take() {
                    out.push(Label {
                        text: text.clone(),
                        white: rect,
                    });
                }
            }
            _ => {}
        }
    }
    out
}

pub fn note_groups(bar: &Group) -> BTreeMap<u64, Vec<Label>> {
    let mut out: BTreeMap<u64, Vec<Label>> = BTreeMap::new();
    for item in &bar.items {
        let Primitive::Group(beat) = item else {
            continue;
        };
        for inner in &beat.items {
            if let Primitive::Group(note) = inner {
                out.entry(note.id.0).or_default().extend(label_of(note));
            }
        }
    }
    out
}

pub fn tab_rows(list: &DisplayList) -> Vec<(&Page, &SystemLayout, Row<'_>)> {
    let mut out = Vec::new();
    for page in &list.pages {
        for (system, rows) in rows(page) {
            for row in rows {
                if row.staff.kind == StaffKind::Tab {
                    out.push((page, system, row));
                }
            }
        }
    }
    out
}

fn centre_y(text: &TextRun) -> Units {
    text.origin.y - baseline_below_centre(text.em)
}

fn vertical_span(item: &Primitive) -> Option<(Units, Units)> {
    match item {
        Primitive::Glyph(glyph) => {
            let rect = ink(glyph);
            Some((rect.min.y, rect.max.y))
        }
        Primitive::Line(line) => Some((line.from.y.min(line.to.y), line.from.y.max(line.to.y))),
        Primitive::FilledRect(rect) => Some((rect.rect.min.y, rect.rect.max.y)),
        Primitive::Text(text) => Some((
            text.origin.y - text.em.scaled(TEXT_ASCENT, PARTS),
            text.origin.y + text.em.scaled(TEXT_DESCENT, PARTS),
        )),
        Primitive::Shape(shape) => {
            let ys: Vec<Units> = shape
                .segments
                .iter()
                .flat_map(|segment| match segment {
                    PathSegment::MoveTo(point) | PathSegment::LineTo(point) => vec![point.y],
                    PathSegment::QuadTo(first, point) => vec![first.y, point.y],
                    PathSegment::CubicTo(first, second, point) => {
                        vec![first.y, second.y, point.y]
                    }
                    PathSegment::Close => Vec::new(),
                })
                .collect();
            Some((*ys.iter().min()?, *ys.iter().max()?))
        }
        Primitive::Group(_) => None,
    }
}

fn flat(items: &[Primitive]) -> Vec<&Primitive> {
    let mut out = Vec::new();
    for item in items {
        match item {
            Primitive::Group(group) => out.extend(flat(&group.items)),
            other => out.push(other),
        }
    }
    out
}

pub fn check_tab(score: &Score, list: &DisplayList) -> Tally {
    let mut tally = Tally::default();
    for (_, system, row) in tab_rows(list) {
        let staff = row.staff;
        let track = &score.tracks[staff.track];
        let mut boxes: Vec<(usize, Rect, bool)> = Vec::new();
        for (layout, group) in staff.bars.iter().zip(&row.bars) {
            for item in flat(&group.items) {
                if let Some((top, bottom)) = vertical_span(item) {
                    assert!(
                        top >= system.rect.min.y && bottom <= system.rect.max.y,
                        "tab item {item:?} leaves system {} ({:?})",
                        system.index,
                        system.rect
                    );
                }
            }
            let crowded = list.crowded.contains(&layout.index);
            let mut found = note_groups(group);
            let Some(bar) = track
                .staves
                .get(staff.staff)
                .and_then(|model| model.bars.get(layout.index))
            else {
                assert!(found.is_empty());
                continue;
            };
            for note in bar
                .voices
                .iter()
                .flat_map(|voice| &voice.beats)
                .flat_map(|beat| &beat.notes)
            {
                let NoteKind::Fretted { string, .. } = note.kind else {
                    continue;
                };
                let line = usize::from(string);
                let labels = found.remove(&note.id.0.0).unwrap_or_default();
                if line >= staff.lines {
                    assert!(labels.is_empty());
                    continue;
                }
                assert_eq!(labels.len(), 1, "note {} has one fret label", note.id.0.0);
                let label = &labels[0];
                assert_eq!(Some(label.text.text.clone()), fret_text(note));
                assert_eq!(
                    centre_y(&label.text),
                    staff.line_y(line),
                    "note {} sits on string line {line}",
                    note.id.0.0
                );
                assert!(label.white.min.x <= label.text.origin.x);
                assert!(label.white.max.x >= label.text.origin.x);
                let size = label.text.em;
                assert!(size <= FRET_EM);
                boxes.push((line, label.white, crowded));
                tally.labels += 1;
            }
            assert!(found.is_empty(), "stray note groups {:?}", found.keys());
        }
        for (index, (line, rect, crowded)) in boxes.iter().enumerate() {
            for (other_line, other, other_crowded) in &boxes[index + 1..] {
                if line != other_line || *crowded || *other_crowded {
                    continue;
                }
                tally.same_string_pairs += 1;
                assert!(
                    !overlaps(*rect, *other),
                    "fret labels overlap on string {line}: {rect:?} {other:?}"
                );
            }
        }
    }
    tally
}

fn arb_mode() -> impl Strategy<Value = Option<StaffMode>> {
    proptest::option::of(prop_oneof![Just(StaffMode::Tab), Just(StaffMode::Both)])
}

#[test]
fn every_fretted_note_has_one_label_on_its_string_and_labels_never_collide() {
    let mut runner = TestRunner::new(ProptestConfig {
        cases: CASES,
        failure_persistence: None,
        ..ProptestConfig::default()
    });
    let total = RefCell::new(Tally::default());
    runner
        .run(&(arb_score(), arb_mode()), |(score, mode)| {
            let options = tall(mode);
            match layout(&score, &options) {
                Ok(list) => {
                    check(&score, &options, &list);
                    let tally = check_tab(&score, &list);
                    let mut sum = total.borrow_mut();
                    sum.labels += tally.labels;
                    sum.same_string_pairs += tally.same_string_pairs;
                }
                Err(_) => prop_assert!(
                    score.master_bars.is_empty()
                        || score.tracks.iter().all(|track| track.staves.is_empty())
                ),
            }
            Ok(())
        })
        .unwrap();
    let tally = total.into_inner();
    assert!(tally.labels >= MIN_LABELS, "{tally:?}");
    assert!(
        tally.same_string_pairs >= MIN_SAME_STRING_PAIRS,
        "{tally:?}"
    );
}
