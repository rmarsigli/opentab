use opentab_layout::glyphs::Metrics;
use opentab_layout::{
    Group, Line, RHYTHM_GAP, StaffKind, StaffLayout, StaffMode, Units, baseline_below_centre,
    tab_staff,
};
use opentab_model::{NoteValue, Score};

use crate::music::{Event, build, glyphs, measure, note_id, options, play, render};
use crate::support::{ink, systems};
use crate::tab::{note_groups, tab_rows};
use crate::tab_effects::{focused, glyph_names, lines, tab_bar};

fn stems_below(bar: &Group, staff: &StaffLayout) -> usize {
    let thickness = Metrics::bravura().engraving_defaults().stem_thickness;
    lines(&bar.items)
        .iter()
        .filter(|line| {
            line.from.x == line.to.x && line.thickness == thickness && line.from.y > staff.bottom()
        })
        .count()
}

fn rhythm_score() -> Score {
    build(&[
        measure(
            (4, 4),
            0,
            vec![(
                0,
                vec![
                    play(NoteValue::Half, &[(2, 2)]),
                    play(NoteValue::Eighth, &[(2, 3)]),
                    play(NoteValue::Eighth, &[(2, 4)]),
                    Event::Rest(crate::music::plain(NoteValue::Quarter)),
                ],
            )],
        ),
        measure(
            (4, 4),
            0,
            vec![(0, vec![play(NoteValue::Whole, &[(1, 0)])])],
        ),
    ])
}

#[test]
fn rhythm_hangs_under_the_tab_only_when_the_standard_staff_is_hidden() {
    let score = rhythm_score();
    let alone = render(&score, &options(StaffMode::Tab));
    let (staff, bar) = tab_bar(&alone, 0);
    assert_eq!(stems_below(bar, staff), 3);
    let thickness = Metrics::bravura().engraving_defaults().stem_thickness;
    let stems: Vec<Line> = lines(&bar.items)
        .into_iter()
        .filter(|line| line.from.x == line.to.x && line.thickness == thickness)
        .collect();
    let gap = staff.bottom() + RHYTHM_GAP;
    assert!(stems.iter().all(|line| line.from.y >= gap));
    let half = stems[0];
    assert!(half.from.y > gap, "a half note has a short stem");
    let names = glyph_names(&bar.items);
    assert!(names.contains(&"restQuarter"));
    assert!(names.iter().all(|name| !name.starts_with("notehead")));
    let (_, whole) = tab_bar(&alone, 1);
    assert_eq!(stems_below(whole, staff), 0);
    let both = render(&score, &options(StaffMode::Both));
    let (staff, bar) = tab_bar(&both, 0);
    assert_eq!(stems_below(bar, staff), 0);
    assert!(!glyph_names(&bar.items).contains(&"restQuarter"));
    insta::assert_snapshot!(focused(&score, StaffMode::Tab));
}

#[test]
fn a_tab_shaped_rhythm_staff_keeps_rests_in_the_lane_under_the_strings() {
    let score = rhythm_score();
    let list = render(&score, &options(StaffMode::Tab));
    let (staff, bar) = tab_bar(&list, 0);
    let rest = glyphs(&bar.items)
        .into_iter()
        .find(|glyph| crate::support::glyph_name(glyph.codepoint) == "restQuarter")
        .unwrap();
    let rect = ink(&rest);
    assert!(rect.min.y > staff.bottom() + RHYTHM_GAP.half());
    let geometry = tab_staff(staff.bottom() - staff.top);
    let lane_top = staff.bottom() - staff.top + RHYTHM_GAP;
    for value in [
        NoteValue::Whole,
        NoteValue::Half,
        NoteValue::Quarter,
        NoteValue::Eighth,
        NoteValue::Sixteenth,
        NoteValue::ThirtySecond,
    ] {
        let glyph = opentab_layout::rhythm::rest_glyph(value);
        let y = geometry.rest_y(value, Units::ZERO);
        let ink = opentab_layout::rhythm::glyph_box(glyph, geometry.size);
        assert!(
            y + ink.min.y >= lane_top - Units(512),
            "{value:?} rest reaches into the strings"
        );
    }
    let five_line = opentab_layout::rhythm::RhythmStaff {
        reference: staff.bottom() - staff.top + RHYTHM_GAP,
        ..geometry
    };
    let quarter = opentab_layout::rhythm::glyph_box(
        opentab_layout::rhythm::rest_glyph(NoteValue::Quarter),
        five_line.size,
    );
    assert!(
        five_line.rest_y(NoteValue::Quarter, Units::ZERO) + quarter.min.y < lane_top - Units(512),
        "a reference at the stem start would push rests into the strings"
    );
}

fn strings_score(pitches: &[u8]) -> Score {
    let frets: Vec<(u8, u8)> = (0..u8::try_from(pitches.len()).unwrap())
        .map(|string| (string, string + 1))
        .collect();
    let mut score = build(&[measure(
        (4, 4),
        0,
        vec![(
            0,
            vec![
                play(NoteValue::Half, &frets),
                play(
                    NoteValue::Half,
                    &[(u8::try_from(pitches.len() - 1).unwrap(), 12)],
                ),
            ],
        )],
    )]);
    score.tracks[0].tuning.strings = pitches.iter().copied().map(opentab_model::Pitch).collect();
    score
}

#[test]
fn seven_and_eight_string_tabs_grow_their_lines_and_keep_every_label() {
    for pitches in [
        vec![64_u8, 59, 55, 50, 45, 40, 35],
        vec![64, 59, 55, 50, 45, 40, 35, 30],
    ] {
        let score = strings_score(&pitches);
        let list = render(&score, &crate::music::tall(None));
        let kinds: Vec<StaffKind> = systems(&list)[0]
            .staves
            .iter()
            .map(|staff| staff.kind)
            .collect();
        assert_eq!(kinds, vec![StaffKind::Standard, StaffKind::Tab]);
        let (staff, bar) = tab_bar(&list, 0);
        assert_eq!(staff.lines, pitches.len());
        let found = note_groups(bar);
        for string in 0..pitches.len() {
            let id = note_id(&score, 0, 0, 0, string);
            let label = &found[&id.0.0][0];
            assert_eq!(label.text.text, (string + 1).to_string());
            assert_eq!(
                label.text.origin.y - baseline_below_centre(label.text.em),
                staff.line_y(string)
            );
        }
        let low = &found[&note_id(&score, 0, 0, 1, 0).0.0][0];
        assert_eq!(
            low.text.origin.y - baseline_below_centre(low.text.em),
            staff.line_y(pitches.len() - 1)
        );
        crate::tab::check_tab(&score, &list);
    }
    let nine = strings_score(&[64, 59, 55, 50, 45, 40, 35, 30, 25]);
    let list = render(&nine, &crate::music::tall(None));
    assert!(tab_rows(&list).is_empty());
}

#[test]
fn rhythm_under_the_tab_follows_the_first_voice_that_plays() {
    let rest = || Event::Rest(crate::music::plain(NoteValue::Quarter));
    let score = build(&[measure(
        (4, 4),
        0,
        vec![
            (0, vec![rest(), rest(), rest(), rest()]),
            (
                1,
                vec![
                    play(NoteValue::Quarter, &[(2, 2)]),
                    play(NoteValue::Quarter, &[(2, 3)]),
                    play(NoteValue::Half, &[(2, 4)]),
                ],
            ),
        ],
    )]);
    let list = render(&score, &options(StaffMode::Tab));
    let (staff, bar) = tab_bar(&list, 0);
    assert_eq!(stems_below(bar, staff), 3);
    assert!(
        glyph_names(&bar.items)
            .iter()
            .all(|name| !name.starts_with("rest")),
        "the silent first voice carries no rhythm"
    );
}

#[test]
fn beams_under_the_tab_are_horizontal() {
    let score = build(&[measure(
        (2, 4),
        0,
        vec![(
            0,
            vec![
                play(NoteValue::Eighth, &[(2, 2)]),
                play(NoteValue::Sixteenth, &[(2, 3)]),
                play(NoteValue::ThirtySecond, &[(2, 4)]),
                play(NoteValue::ThirtySecond, &[(2, 5)]),
                play(NoteValue::Quarter, &[(2, 5)]),
            ],
        )],
    )]);
    let list = render(&score, &options(StaffMode::Tab));
    let (staff, bar) = tab_bar(&list, 0);
    let thickness = Metrics::bravura().engraving_defaults().stem_thickness;
    let ends: Vec<Units> = lines(&bar.items)
        .iter()
        .filter(|line| line.from.x == line.to.x && line.thickness == thickness)
        .filter(|line| line.from.y > staff.bottom())
        .map(|line| line.to.y)
        .collect();
    assert_eq!(ends.len(), 5);
    assert!(ends[..4].iter().all(|end| *end == ends[0]), "{ends:?}");
}
