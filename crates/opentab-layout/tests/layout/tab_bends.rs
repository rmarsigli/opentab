use opentab_layout::text::text_font;
use opentab_layout::{Rect, StaffMode, TextRun, Units};
use opentab_model::{NoteValue, Score};

use crate::music::{build, glyphs, measure, note_at, options, play, render};
use crate::support::ink;
use crate::tab_effects::{bars_of_quarters, beat_group, bend, glyph_names, lines, tab_bar, texts};

const BEND_EM: Units = Units(1126);

#[derive(Debug, Clone, Copy)]
struct Case {
    name: &'static str,
    values: [i16; 3],
    tied: bool,
    up: usize,
    down: usize,
    labels: &'static [&'static str],
}

const CASES: [Case; 8] = [
    Case {
        name: "bend",
        values: [0, 200, 200],
        tied: false,
        up: 1,
        down: 0,
        labels: &["full"],
    },
    Case {
        name: "bend and release",
        values: [0, 200, 0],
        tied: false,
        up: 1,
        down: 1,
        labels: &["full"],
    },
    Case {
        name: "release of a held bend",
        values: [200, 200, 0],
        tied: true,
        up: 0,
        down: 1,
        labels: &[],
    },
    Case {
        name: "pre-bend",
        values: [200, 200, 200],
        tied: false,
        up: 1,
        down: 0,
        labels: &["full"],
    },
    Case {
        name: "pre-bend then bend",
        values: [100, 200, 200],
        tied: false,
        up: 2,
        down: 0,
        labels: &["\u{bd}", "full"],
    },
    Case {
        name: "pre-bend and release",
        values: [200, 200, 0],
        tied: false,
        up: 1,
        down: 1,
        labels: &["full"],
    },
    Case {
        name: "partial release",
        values: [0, 200, 100],
        tied: false,
        up: 1,
        down: 1,
        labels: &["full", "\u{bd}"],
    },
    Case {
        name: "negative dip",
        values: [0, -100, 0],
        tied: false,
        up: 1,
        down: 1,
        labels: &["\u{2212}\u{bd}"],
    },
];

fn bend_labels(items: &[opentab_layout::Primitive]) -> Vec<TextRun> {
    texts(items)
        .into_iter()
        .filter(|run| run.em == BEND_EM)
        .collect()
}

fn text_box(run: &TextRun) -> Rect {
    let width = text_font().measure(&run.text, run.em);
    Rect::new(
        opentab_layout::Point {
            x: run.origin.x - width.half(),
            y: run.origin.y - run.em.scaled(3, std::num::NonZeroI64::new(4).unwrap()),
        },
        opentab_layout::Point {
            x: run.origin.x + width.half(),
            y: run.origin.y,
        },
    )
}

fn count(names: &[&str], wanted: &str) -> usize {
    names.iter().filter(|name| **name == wanted).count()
}

fn shaped(case: Case) -> Score {
    let mut score = bars_of_quarters(&[&[(2, 7), (2, 7), (2, 7), (2, 7)]]);
    let [origin, middle, destination] = case.values;
    note_at(&mut score, 0, 0, 1, 0).effects.bend = Some(bend([
        (0, origin),
        (30, middle),
        (60, middle),
        (100, destination),
    ]));
    if case.tied {
        note_at(&mut score, 0, 0, 0, 0).effects.tie_origin = true;
        note_at(&mut score, 0, 0, 1, 0).effects.tie_destination = true;
        note_at(&mut score, 0, 0, 0, 0).effects.bend =
            Some(bend([(0, 0), (30, origin), (60, origin), (100, origin)]));
    }
    score
}

#[test]
fn every_bend_shape_draws_its_segments_with_their_directions_and_amounts() {
    for case in CASES {
        let score = shaped(case);
        for mode in [StaffMode::Both, StaffMode::Tab] {
            let list = render(&score, &options(mode));
            let (staff, bar) = tab_bar(&list, 0);
            let group = beat_group(bar, &score, 0, 1);
            let names = glyph_names(&group.items);
            assert_eq!(count(&names, "arrowheadBlackUp"), case.up, "{}", case.name);
            assert_eq!(
                count(&names, "arrowheadBlackDown"),
                case.down,
                "{}",
                case.name
            );
            let labels: Vec<String> = bend_labels(&group.items)
                .into_iter()
                .map(|run| run.text)
                .collect();
            assert_eq!(labels, case.labels, "{}", case.name);
            let line = staff.line_y(2);
            for glyph in glyphs(&group.items)
                .into_iter()
                .filter(|glyph| crate::support::glyph_name(glyph.codepoint) == "arrowheadBlackDown")
            {
                let apex = ink(&glyph).max.y;
                if case.values.iter().any(|value| *value < 0) {
                    assert!(
                        apex > line,
                        "{}: a negative bend points below its note",
                        case.name
                    );
                } else {
                    assert!(
                        apex <= line,
                        "{}: a release falls back to its note",
                        case.name
                    );
                }
            }
        }
    }
}

#[test]
fn a_pre_bend_rises_straight_from_its_fret_and_a_bend_curves_beside_it() {
    let pre = shaped(CASES[3]);
    let list = render(&pre, &options(StaffMode::Both));
    let (staff, bar) = tab_bar(&list, 0);
    let group = beat_group(bar, &pre, 0, 1);
    let shaft: Vec<_> = lines(&group.items)
        .into_iter()
        .filter(|line| line.to.y < staff.top)
        .collect();
    assert_eq!(shaft.len(), 1);
    assert_eq!(shaft[0].from.x, shaft[0].to.x);
    let both = shaped(CASES[4]);
    let list = render(&both, &options(StaffMode::Both));
    let (_, bar) = tab_bar(&list, 0);
    let labels = bend_labels(&beat_group(bar, &both, 0, 1).items);
    assert!(labels[1].origin.y < labels[0].origin.y, "full sits above ½");
    assert!(
        labels[1].origin.x > labels[0].origin.x,
        "the bend continues to the right"
    );
}

fn chord(amounts: [i16; 2]) -> Score {
    let mut score = build(&[measure(
        (4, 4),
        0,
        vec![(0, vec![play(NoteValue::Whole, &[(1, 12), (2, 12)])])],
    )]);
    for (note, amount) in amounts.into_iter().enumerate() {
        note_at(&mut score, 0, 0, 0, note).effects.bend =
            Some(bend([(0, 0), (30, amount), (60, amount), (100, amount)]));
    }
    score
}

#[test]
fn bends_in_one_chord_share_an_arrow_when_equal_and_stack_their_labels_otherwise() {
    let equal = chord([200, 200]);
    let list = render(&equal, &options(StaffMode::Both));
    let group = beat_group(tab_bar(&list, 0).1, &equal, 0, 0);
    assert_eq!(count(&glyph_names(&group.items), "arrowheadBlackUp"), 1);
    assert_eq!(bend_labels(&group.items).len(), 1);
    let different = chord([100, 200]);
    let list = render(&different, &options(StaffMode::Both));
    let group = beat_group(tab_bar(&list, 0).1, &different, 0, 0);
    assert_eq!(count(&glyph_names(&group.items), "arrowheadBlackUp"), 2);
    let labels = bend_labels(&group.items);
    let words: Vec<&str> = labels.iter().map(|run| run.text.as_str()).collect();
    assert_eq!(words, vec!["\u{bd}", "full"]);
    assert!(!text_box(&labels[0]).overlaps(&text_box(&labels[1])));
    let heads: Vec<Rect> = glyphs(&group.items).iter().map(ink).collect();
    for label in &labels {
        for head in &heads {
            assert!(
                !text_box(label).overlaps(head),
                "a label covers an arrowhead"
            );
        }
    }
}
