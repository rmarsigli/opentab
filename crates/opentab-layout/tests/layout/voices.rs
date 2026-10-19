use std::cell::RefCell;
use std::collections::BTreeMap;

use opentab_layout::{DisplayList, Group, GroupId, Primitive, Rect, StaffMode, layout};
use opentab_model::{NoteValue, Score};
use opentab_testkit::arb_score;
use proptest::prelude::*;
use proptest::test_runner::TestRunner;

use crate::music::{
    Event, beat_id, build, dotted, glyphs, group_in, is_notehead, measure, options, overlaps,
    plain, play, render, standard_rows, tall,
};
use crate::support::glyph_name;

const MIN_SHARED_BARS: usize = 150;
const MIN_CHECKED_PAIRS: usize = 20_000;
const G3: (u8, u8) = (5, 3);
const E4: (u8, u8) = (3, 2);
const A4: (u8, u8) = (2, 2);
const B4: (u8, u8) = (1, 0);
const C5: (u8, u8) = (1, 1);
const E5: (u8, u8) = (0, 0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InkKind {
    Head,
    Rest,
    Dot,
    Stem,
}

#[derive(Debug, Clone, Copy)]
pub struct VoiceMark {
    pub rect: Rect,
    pub kind: InkKind,
    pub beat: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct VoiceTally {
    pub shared_bars: usize,
    pub checked_pairs: usize,
}

pub fn voice_of_beats(score: &Score) -> BTreeMap<u64, usize> {
    let mut out = BTreeMap::new();
    for track in &score.tracks {
        for staff in &track.staves {
            for bar in &staff.bars {
                for (index, voice) in bar.voices.iter().enumerate() {
                    for beat in &voice.beats {
                        out.insert(beat.id.0.0, index);
                    }
                }
            }
        }
    }
    out
}

fn stem_rect(line: &opentab_layout::Line) -> Rect {
    let half = line.thickness.half();
    Rect::new(
        opentab_layout::Point {
            x: line.from.x - half,
            y: line.from.y.min(line.to.y),
        },
        opentab_layout::Point {
            x: line.from.x + half,
            y: line.from.y.max(line.to.y),
        },
    )
}

pub fn voice_marks(bar: &Group) -> Vec<VoiceMark> {
    let mut out = Vec::new();
    for item in &bar.items {
        let Primitive::Group(beat) = item else {
            continue;
        };
        for inner in &beat.items {
            match inner {
                Primitive::Group(note) => {
                    for glyph in glyphs(&note.items)
                        .iter()
                        .filter(|glyph| is_notehead(glyph))
                    {
                        out.push(VoiceMark {
                            rect: crate::support::ink(glyph),
                            kind: InkKind::Head,
                            beat: beat.id.0,
                        });
                    }
                }
                Primitive::Glyph(glyph) => {
                    let name = glyph_name(glyph.codepoint);
                    let kind = if name.starts_with("rest") {
                        InkKind::Rest
                    } else if name == "augmentationDot" {
                        InkKind::Dot
                    } else {
                        continue;
                    };
                    out.push(VoiceMark {
                        rect: crate::support::ink(glyph),
                        kind,
                        beat: beat.id.0,
                    });
                }
                Primitive::Line(line) if line.from.x == line.to.x && line.from.y != line.to.y => {
                    out.push(VoiceMark {
                        rect: stem_rect(line),
                        kind: InkKind::Stem,
                        beat: beat.id.0,
                    });
                }
                _ => {}
            }
        }
    }
    out
}

fn intrudes(mover: InkKind, target: InkKind) -> bool {
    matches!(mover, InkKind::Rest | InkKind::Dot | InkKind::Stem)
        && matches!(target, InkKind::Head | InkKind::Rest | InkKind::Dot)
}

pub fn check_voices(score: &Score, list: &DisplayList) -> VoiceTally {
    let voices = voice_of_beats(score);
    let mut tally = VoiceTally::default();
    for (_, _, row) in standard_rows(list) {
        for (bar, group) in row.staff.bars.iter().zip(&row.bars) {
            if list.crowded.contains(&bar.index) {
                continue;
            }
            let marks = voice_marks(group);
            let mut shared = false;
            for first in &marks {
                for second in &marks {
                    let (Some(one), Some(other)) =
                        (voices.get(&first.beat), voices.get(&second.beat))
                    else {
                        continue;
                    };
                    if one == other {
                        continue;
                    }
                    shared = true;
                    if intrudes(first.kind, second.kind) {
                        tally.checked_pairs += 1;
                        assert!(
                            !overlaps(first.rect, second.rect),
                            "bar {}: {first:?} of voice {one} hits {second:?} of voice {other}",
                            bar.index
                        );
                    }
                }
            }
            if shared {
                tally.shared_bars += 1;
            }
        }
    }
    tally
}

#[test]
fn no_voice_draws_over_another_voice() {
    let mut runner = TestRunner::new(ProptestConfig {
        failure_persistence: None,
        ..crate::cases::config()
    });
    let total = RefCell::new(VoiceTally::default());
    let strategy = (
        arb_score(),
        proptest::option::of(prop_oneof![
            Just(StaffMode::Standard),
            Just(StaffMode::Both)
        ]),
    );
    runner
        .run(&strategy, |(score, mode)| {
            if let Ok(list) = layout(&score, &tall(mode)) {
                let tally = check_voices(&score, &list);
                let mut sum = total.borrow_mut();
                sum.shared_bars += tally.shared_bars;
                sum.checked_pairs += tally.checked_pairs;
            }
            Ok(())
        })
        .unwrap();
    let total = total.into_inner();
    assert!(
        total.shared_bars >= crate::cases::scaled(MIN_SHARED_BARS),
        "{total:?}"
    );
    assert!(
        total.checked_pairs >= crate::cases::scaled(MIN_CHECKED_PAIRS),
        "{total:?}"
    );
}

fn rest_glyph(list: &DisplayList, id: opentab_model::BeatId) -> opentab_layout::PlacedGlyph {
    glyphs(&group_in(list, GroupId(id.0.0)).items)
        .into_iter()
        .find(|glyph| glyph_name(glyph.codepoint).starts_with("rest"))
        .unwrap()
}

fn ledger_at(list: &DisplayList, id: opentab_model::BeatId, y: opentab_layout::Units) -> bool {
    group_in(list, GroupId(id.0.0))
        .items
        .iter()
        .any(|item| matches!(item, Primitive::Line(line) if line.from.y == y && line.to.y == y))
}

#[test]
fn four_voices_with_rests_keep_clear_of_each_other() {
    let half_rest = Event::Rest(plain(NoteValue::Half));
    let score = build(&[measure(
        (4, 4),
        0,
        vec![
            (0, vec![play(NoteValue::Whole, &[E5])]),
            (1, vec![play(NoteValue::Half, &[A4]), half_rest.clone()]),
            (2, vec![half_rest, play(NoteValue::Half, &[E4])]),
            (3, vec![play(NoteValue::Whole, &[G3])]),
        ],
    )]);
    let list = render(&score, &options(StaffMode::Standard));
    let tally = check_voices(&score, &list);
    assert!(tally.checked_pairs >= 12, "{tally:?}");
    let staff = &crate::support::systems(&list)[0].staves[0];
    let lowered = rest_glyph(&list, beat_id(&score, 0, 1, 1));
    let raised = rest_glyph(&list, beat_id(&score, 0, 2, 0));
    assert!(lowered.origin.y > staff.bottom());
    assert!(raised.origin.y < staff.top);
    assert!(ledger_at(&list, beat_id(&score, 0, 1, 1), lowered.origin.y));
    assert!(ledger_at(&list, beat_id(&score, 0, 2, 0), raised.origin.y));
}

#[test]
fn a_whole_bar_rest_and_a_dot_make_room_for_the_other_voice() {
    let eighths = vec![play(NoteValue::Eighth, &[E4]); 8];
    let score = build(&[
        measure(
            (4, 4),
            0,
            vec![
                (0, eighths),
                (1, vec![Event::Rest(plain(NoteValue::Whole))]),
            ],
        ),
        measure(
            (4, 4),
            0,
            vec![
                (
                    0,
                    vec![play(NoteValue::Half, &[B4]), play(NoteValue::Half, &[B4])],
                ),
                (
                    1,
                    vec![
                        Event::Play(dotted(NoteValue::Quarter, 1), vec![C5]),
                        play(NoteValue::Eighth, &[C5]),
                        play(NoteValue::Half, &[C5]),
                    ],
                ),
            ],
        ),
    ]);
    let list = render(&score, &options(StaffMode::Standard));
    check_voices(&score, &list);
    let staff = &crate::support::systems(&list)[0].staves[0];
    let whole = rest_glyph(&list, beat_id(&score, 0, 1, 0));
    assert_eq!(glyph_name(whole.codepoint), "restWhole");
    assert!(whole.origin.y > staff.middle());
    let lowest_head =
        glyphs(&group_in(&list, GroupId(score.tracks[0].staves[0].bars[0].id.0.0)).items)
            .into_iter()
            .filter(is_notehead)
            .map(|glyph| crate::support::ink(&glyph).max.y)
            .max()
            .unwrap();
    assert!(crate::support::ink(&whole).min.y > lowest_head);
    let shifted = glyphs(&group_in(&list, GroupId(beat_id(&score, 1, 0, 0).0.0)).items)
        .into_iter()
        .find(is_notehead)
        .unwrap();
    let dot = glyphs(&group_in(&list, GroupId(beat_id(&score, 1, 1, 0).0.0)).items)
        .into_iter()
        .find(|glyph| glyph_name(glyph.codepoint) == "augmentationDot")
        .unwrap();
    assert!(crate::support::ink(&dot).min.x > crate::support::ink(&shifted).max.x);
}
