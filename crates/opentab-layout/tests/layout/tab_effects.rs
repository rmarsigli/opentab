use opentab_layout::{
    DisplayList, Group, GroupId, Line, PathStyle, PlacedGlyph, Primitive, Shape, StaffLayout,
    StaffMode, TextRun, Units, baseline_below_centre,
};
use opentab_model::{
    BeatEffects, Bend, BendPoint, Finger, Fraction, Harmonic, HarmonicKind, NoteKind, NoteValue,
    Score, Slide, Stroke, StrokeDirection, StrokeTicks, Tap, TremoloPicking, Vibrato,
};

use crate::music::{
    Event, build, find_group, glyphs, measure, named, note_at, note_id, options, play, render,
};
use crate::support::{dump, ink, quarter_bars, systems};
use crate::tab::{note_groups, tab_rows};

pub fn quarters(notes: &[(u8, u8)]) -> Vec<Event> {
    notes
        .iter()
        .map(|note| play(NoteValue::Quarter, &[*note]))
        .collect()
}

fn bar_of_quarters(notes: &[(u8, u8)]) -> Score {
    build(&[measure((4, 4), 0, vec![(0, quarters(notes))])])
}

fn bars_of_quarters(bars: &[&[(u8, u8)]]) -> Score {
    let measures: Vec<_> = bars
        .iter()
        .map(|notes| measure((4, 4), 0, vec![(0, quarters(notes))]))
        .collect();
    build(&measures)
}

pub fn tab_bar(list: &DisplayList, index: usize) -> (&StaffLayout, &Group) {
    tab_rows(list)
        .into_iter()
        .find_map(|(_, _, row)| {
            row.staff
                .bars
                .iter()
                .position(|bar| bar.index == index)
                .map(|position| (row.staff, row.bars[position]))
        })
        .expect("a tab bar")
}

pub fn flat(items: &[Primitive]) -> Vec<&Primitive> {
    let mut out = Vec::new();
    for item in items {
        match item {
            Primitive::Group(group) => out.extend(flat(&group.items)),
            other => out.push(other),
        }
    }
    out
}

pub fn texts(items: &[Primitive]) -> Vec<TextRun> {
    flat(items)
        .into_iter()
        .filter_map(|item| match item {
            Primitive::Text(run) => Some(run.clone()),
            _ => None,
        })
        .collect()
}

pub fn words(items: &[Primitive]) -> Vec<String> {
    texts(items).into_iter().map(|run| run.text).collect()
}

pub fn lines(items: &[Primitive]) -> Vec<Line> {
    flat(items)
        .into_iter()
        .filter_map(|item| match item {
            Primitive::Line(line) => Some(*line),
            _ => None,
        })
        .collect()
}

pub fn shapes(items: &[Primitive]) -> Vec<Shape> {
    flat(items)
        .into_iter()
        .filter_map(|item| match item {
            Primitive::Shape(shape) => Some(shape.clone()),
            _ => None,
        })
        .collect()
}

pub fn glyph_names(items: &[Primitive]) -> Vec<&'static str> {
    named(&glyphs(items))
}

pub fn beat_group<'a>(bar: &'a Group, score: &Score, bar_index: usize, beat: usize) -> &'a Group {
    let id = score.tracks[0].staves[0].bars[bar_index].voices[0].beats[beat].id;
    find_group(&bar.items, GroupId(id.0.0)).expect("a beat group")
}

fn bend(values: [(u8, i16); 4]) -> Bend {
    let point = |(offset, value): (u8, i16)| BendPoint::new(offset, value).unwrap();
    Bend::new(
        point(values[0]),
        point(values[1]),
        point(values[2]),
        point(values[3]),
    )
    .unwrap()
}

fn plain_bend(cents: i16) -> Bend {
    bend([(0, 0), (20, cents), (40, cents), (60, cents)])
}

fn beat_effects(score: &mut Score, bar: usize, beat: usize) -> &mut BeatEffects {
    &mut score.tracks[0].staves[0].bars[bar].voices[0].beats[beat].effects
}

pub fn focused(score: &Score, mode: StaffMode) -> String {
    dump(&render(score, &options(mode)))
}

fn labels_score() -> Score {
    let mut score = build(&[measure(
        (4, 4),
        0,
        vec![(
            0,
            vec![
                play(NoteValue::Quarter, &[(0, 0), (1, 12), (5, 3)]),
                play(NoteValue::Quarter, &[(2, 5)]),
                play(NoteValue::Quarter, &[(2, 5), (3, 7)]),
                play(NoteValue::Quarter, &[(0, 12), (2, 3), (4, 5)]),
            ],
        )],
    )]);
    note_at(&mut score, 0, 0, 1, 0).effects.tie_origin = true;
    note_at(&mut score, 0, 0, 2, 0).effects.tie_destination = true;
    note_at(&mut score, 0, 0, 2, 1).effects.dead = true;
    note_at(&mut score, 0, 0, 0, 2).effects.ghost = true;
    note_at(&mut score, 0, 0, 3, 0).effects.harmonic = Some(Harmonic {
        kind: HarmonicKind::Natural,
        fret: Fraction::new(12, 1),
    });
    note_at(&mut score, 0, 0, 3, 1).effects.harmonic = Some(Harmonic {
        kind: HarmonicKind::Natural,
        fret: Fraction::new(16, 5),
    });
    note_at(&mut score, 0, 0, 3, 2).effects.harmonic = Some(Harmonic {
        kind: HarmonicKind::Artificial,
        fret: Fraction::new(17, 1),
    });
    score
}

#[test]
fn fret_labels_sit_on_their_strings_and_show_the_note_effects() {
    let score = labels_score();
    let list = render(&score, &options(StaffMode::Tab));
    let (staff, bar) = tab_bar(&list, 0);
    let found = note_groups(bar);
    let expected = [
        ((0, 0), "0", 0),
        ((0, 1), "12", 1),
        ((0, 2), "(3)", 5),
        ((1, 0), "5", 2),
        ((2, 0), "(5)", 2),
        ((2, 1), "x", 3),
        ((3, 0), "<12>", 0),
        ((3, 1), "<3.2>", 2),
        ((3, 2), "[5]", 4),
    ];
    for ((beat, note), text, line) in expected {
        let id = note_id(&score, 0, 0, beat, note);
        let labels = &found[&id.0.0];
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].text.text, text);
        assert_eq!(
            labels[0].text.origin.y - baseline_below_centre(labels[0].text.em),
            staff.line_y(line)
        );
    }
    let marks = words(&bar.items);
    assert!(marks.contains(&"A.H.".to_string()));
    let ties = shapes(&bar.items);
    assert_eq!(
        ties.len(),
        1,
        "a tab tie arc when the standard staff is hidden"
    );
    let both = render(&score, &options(StaffMode::Both));
    assert!(shapes(&tab_bar(&both, 0).1.items).is_empty());
    insta::assert_snapshot!(focused(&score, StaffMode::Both));
}

#[test]
fn string_one_is_the_top_line_and_each_string_keeps_its_own_line() {
    let score = bar_of_quarters(&[(0, 1), (1, 2), (4, 3), (5, 4)]);
    let list = render(&score, &options(StaffMode::Tab));
    let (staff, bar) = tab_bar(&list, 0);
    let found = note_groups(bar);
    for (beat, line) in [(0, 0), (1, 1), (2, 4), (3, 5)] {
        let id = note_id(&score, 0, 0, beat, 0);
        let label = &found[&id.0.0][0];
        assert_eq!(
            label.text.origin.y - baseline_below_centre(label.text.em),
            staff.line_y(line)
        );
    }
    assert!(staff.line_y(0) < staff.line_y(5));
}

fn bends_score() -> Score {
    let mut score = bars_of_quarters(&[
        &[(2, 7), (2, 7), (2, 7), (2, 7)],
        &[(2, 7), (2, 7), (2, 7), (2, 7)],
    ]);
    let plans = [
        (0, 0, plain_bend(200)),
        (0, 1, plain_bend(100)),
        (0, 2, plain_bend(50)),
        (0, 3, plain_bend(300)),
        (1, 0, bend([(0, 0), (20, 200), (60, 200), (100, 0)])),
        (1, 1, bend([(0, 200), (30, 200), (60, 200), (100, 200)])),
        (1, 2, bend([(0, 200), (30, 200), (60, 200), (100, 0)])),
        (1, 3, plain_bend(400)),
    ];
    for (bar, beat, shape) in plans {
        note_at(&mut score, bar, 0, beat, 0).effects.bend = Some(shape);
    }
    score
}

#[test]
fn bends_draw_an_arrow_with_their_amount_in_whole_steps() {
    let score = bends_score();
    let list = render(&score, &options(StaffMode::Tab));
    let (staff, first) = tab_bar(&list, 0);
    let labels = ["full", "\u{bd}", "\u{bc}", "1\u{bd}"];
    for (beat, label) in labels.iter().enumerate() {
        let group = beat_group(first, &score, 0, beat);
        assert_eq!(glyph_names(&group.items), vec!["arrowheadBlackUp"]);
        assert_eq!(shapes(&group.items).len(), 1, "a curved shaft");
        let found = words(&group.items);
        assert!(found.contains(&label.to_string()), "{found:?}");
        let head = glyphs(&group.items)[0];
        assert!(ink(&head).min.y < staff.top);
    }
    let (_, second) = tab_bar(&list, 1);
    let release = beat_group(second, &score, 1, 0);
    assert_eq!(
        glyph_names(&release.items),
        vec!["arrowheadBlackUp", "arrowheadBlackDown"]
    );
    assert_eq!(shapes(&release.items).len(), 2);
    let prebend = beat_group(second, &score, 1, 1);
    let shaft: Vec<Line> = lines(&prebend.items)
        .into_iter()
        .filter(|line| line.to.y < staff.top)
        .collect();
    assert_eq!(shaft.len(), 1);
    assert_eq!(
        shaft[0].from.x, shaft[0].to.x,
        "a pre-bend rises straight up"
    );
    assert!(shapes(&prebend.items).is_empty());
    let prebend_release = beat_group(second, &score, 1, 2);
    assert_eq!(
        glyph_names(&prebend_release.items),
        vec!["arrowheadBlackUp", "arrowheadBlackDown"]
    );
    assert!(words(&beat_group(second, &score, 1, 3).items).contains(&"2".to_string()));
    insta::assert_snapshot!(focused(&score, StaffMode::Tab));
}

#[test]
fn a_bend_above_the_tab_opens_its_system_and_pushes_the_next_one_down() {
    let measures: Vec<_> = quarter_bars(16)
        .iter()
        .map(|_| measure((4, 4), 0, vec![(0, quarters(&[(2, 5); 4]))]))
        .collect();
    let plain = build(&measures);
    let mut bent = plain.clone();
    note_at(&mut bent, 0, 0, 1, 0).effects.bend = Some(plain_bend(200));
    let before = render(&plain, &options(StaffMode::Both));
    let after = render(&bent, &options(StaffMode::Both));
    let (old, new) = (systems(&before), systems(&after));
    assert!(old.len() >= 2 && old.len() == new.len());
    let grown = new[0].rect.height() - old[0].rect.height();
    assert!(grown > Units::ZERO, "the bent system grows");
    let offset = |system: &opentab_layout::SystemLayout, row: usize| {
        system.staves[row].top - system.rect.min.y
    };
    assert_eq!(
        offset(new[0], 0),
        offset(old[0], 0),
        "the standard staff stays"
    );
    assert_eq!(
        offset(new[0], 1) - offset(old[0], 1),
        grown,
        "the tab moves down inside its system"
    );
    assert_eq!(
        new[1].rect.min.y - old[1].rect.min.y,
        grown,
        "the next system starts lower by the same amount"
    );
    assert_eq!(new[1].rect.height(), old[1].rect.height());
    let (staff, bar) = tab_bar(&after, 0);
    let top = flat(&bar.items)
        .into_iter()
        .filter_map(|item| match item {
            Primitive::Text(run) => Some(run.origin.y - run.em),
            _ => None,
        })
        .min()
        .unwrap();
    assert!(top >= new[0].rect.min.y && top < staff.top);
}

fn links_score() -> Score {
    let mut score = bars_of_quarters(&[
        &[(3, 5), (3, 7), (3, 7), (3, 5)],
        &[(2, 5), (2, 7), (2, 7), (2, 5)],
        &[(2, 5), (2, 5), (2, 5), (2, 9)],
        &[(2, 12), (2, 5), (2, 5), (2, 5)],
    ]);
    note_at(&mut score, 0, 0, 0, 0).effects.hammer_on_pull_off = true;
    note_at(&mut score, 0, 0, 2, 0).effects.hammer_on_pull_off = true;
    note_at(&mut score, 1, 0, 0, 0).effects.slide = Some(Slide::Shift);
    note_at(&mut score, 1, 0, 2, 0).effects.slide = Some(Slide::Legato);
    note_at(&mut score, 2, 0, 0, 0).effects.slide = Some(Slide::IntoFromBelow);
    note_at(&mut score, 2, 0, 1, 0).effects.slide = Some(Slide::IntoFromAbove);
    note_at(&mut score, 2, 0, 2, 0).effects.slide = Some(Slide::OutDownwards);
    note_at(&mut score, 2, 0, 3, 0).effects.hammer_on_pull_off = true;
    note_at(&mut score, 3, 0, 1, 0).effects.slide = Some(Slide::OutUpwards);
    score
}

fn rising(line: &Line) -> bool {
    line.to.x > line.from.x && line.to.y < line.from.y
}

fn falling(line: &Line) -> bool {
    line.to.x > line.from.x && line.to.y > line.from.y
}

#[test]
fn hammer_ons_and_pull_offs_arc_over_both_frets_with_their_letter() {
    let score = links_score();
    let list = render(&score, &options(StaffMode::Both));
    let (_, bar) = tab_bar(&list, 0);
    let arcs: Vec<Shape> = shapes(&bar.items)
        .into_iter()
        .filter(|shape| shape.style == PathStyle::Fill)
        .collect();
    assert_eq!(arcs.len(), 2);
    let letters: Vec<String> = words(&bar.items)
        .into_iter()
        .filter(|word| word == "H" || word == "P")
        .collect();
    assert_eq!(letters, vec!["H", "P"]);
    let (_, third) = tab_bar(&list, 2);
    assert!(
        words(&third.items).contains(&"H".to_string()),
        "a hammer-on across the barline 9 to 12"
    );
    insta::assert_snapshot!(focused(&score, StaffMode::Both));
}

#[test]
fn slides_join_frets_with_a_line_and_slides_in_or_out_are_short_strokes() {
    let score = links_score();
    let list = render(&score, &options(StaffMode::Both));
    let (staff, second) = tab_bar(&list, 1);
    let joined: Vec<Line> = lines(&second.items)
        .into_iter()
        .filter(|line| line.from.y != line.to.y && line.from.x != line.to.x)
        .collect();
    assert_eq!(joined.len(), 2);
    assert!(rising(&joined[0]), "5 to 7 rises");
    assert!(falling(&joined[1]), "7 to 5 falls");
    let (_, third) = tab_bar(&list, 2);
    let label_x = |beat: usize| {
        let id = note_id(&score, 2, 0, beat, 0);
        note_groups(third)[&id.0.0][0].text.origin.x
    };
    let slide = |beat: usize| lines(&beat_group(third, &score, 2, beat).items)[0];
    assert!(
        rising(&slide(0)) && slide(0).to.x < label_x(0),
        "in from below"
    );
    assert!(
        falling(&slide(1)) && slide(1).to.x < label_x(1),
        "in from above"
    );
    assert!(
        falling(&slide(2)) && slide(2).from.x > label_x(2),
        "out downwards"
    );
    let (_, fourth) = tab_bar(&list, 3);
    let out = lines(&beat_group(fourth, &score, 3, 1).items)[0];
    assert!(rising(&out), "out upwards");
    for line in joined {
        assert!((line.from.y - staff.line_y(2)).0.abs() < 512);
    }
}

fn lanes_score() -> Score {
    let mut score = bars_of_quarters(&[
        &[(1, 7), (1, 7), (1, 7), (1, 7)],
        &[(1, 7), (1, 7), (1, 7), (1, 7)],
    ]);
    for beat in 0..4 {
        note_at(&mut score, 0, 0, beat, 0).effects.let_ring = true;
    }
    note_at(&mut score, 1, 0, 0, 0).effects.let_ring = true;
    note_at(&mut score, 0, 0, 1, 0).effects.palm_mute = true;
    note_at(&mut score, 0, 0, 2, 0).effects.palm_mute = true;
    note_at(&mut score, 0, 0, 3, 0).effects.vibrato = Some(Vibrato::Slight);
    note_at(&mut score, 1, 0, 1, 0).effects.vibrato = Some(Vibrato::Wide);
    note_at(&mut score, 1, 0, 2, 0).effects.tap = Some(Tap::Right);
    note_at(&mut score, 1, 0, 3, 0).effects.tap = Some(Tap::Left);
    note_at(&mut score, 1, 0, 3, 0).effects.harmonic = Some(Harmonic {
        kind: HarmonicKind::Pinch,
        fret: Fraction::new(12, 1),
    });
    score
}

#[test]
fn let_ring_palm_mute_vibrato_and_taps_sit_in_lanes_above_the_tab() {
    let score = lanes_score();
    let list = render(&score, &options(StaffMode::Both));
    let (staff, first) = tab_bar(&list, 0);
    let (_, second) = tab_bar(&list, 1);
    let first_words = words(&first.items);
    assert_eq!(
        first_words
            .iter()
            .filter(|word| *word == "let ring")
            .count(),
        1
    );
    assert_eq!(first_words.iter().filter(|word| *word == "P.M.").count(), 1);
    assert!(!words(&second.items).contains(&"let ring".to_string()));
    let dashed: Vec<Line> = lines(&first.items)
        .into_iter()
        .filter(|line| line.dash.is_some())
        .collect();
    assert_eq!(dashed.len(), 2);
    let ring = dashed.iter().max_by_key(|line| line.to.x).copied().unwrap();
    assert!(
        ring.to.x > tab_bar(&list, 1).0.bars[1].columns[0].x,
        "let ring runs into the next bar"
    );
    assert!(dashed.iter().all(|line| line.from.y < staff.top));
    let slight = glyph_names(&first.items)
        .into_iter()
        .filter(|name| *name == "wiggleVibrato")
        .count();
    let wide = glyph_names(&second.items)
        .into_iter()
        .filter(|name| *name == "wiggleVibratoWide")
        .count();
    assert!(slight >= 2 && wide >= 2, "{slight} {wide}");
    for glyph in glyphs(&first.items)
        .into_iter()
        .chain(glyphs(&second.items))
        .filter(|glyph| crate::support::glyph_name(glyph.codepoint).starts_with("wiggle"))
    {
        assert!(ink(&glyph).max.y < staff.top);
    }
    let marks = words(&second.items);
    for mark in ["T", "LT P.H."] {
        assert!(marks.contains(&mark.to_string()), "{marks:?}");
    }
    insta::assert_snapshot!(focused(&score, StaffMode::Both));
}

#[test]
fn a_let_ring_run_repeats_its_label_at_every_system_it_reaches() {
    let measures: Vec<_> = (0..16)
        .map(|_| measure((4, 4), 0, vec![(0, quarters(&[(1, 7); 4]))]))
        .collect();
    let mut score = build(&measures);
    for bar in 0..16 {
        for beat in 0..4 {
            note_at(&mut score, bar, 0, beat, 0).effects.let_ring = true;
        }
    }
    let list = render(&score, &options(StaffMode::Tab));
    let systems = systems(&list).len();
    assert!(systems >= 2);
    let rows = tab_rows(&list);
    let mut labels = 0;
    let mut hooks = 0;
    for (_, _, row) in &rows {
        for bar in &row.bars {
            labels += words(&bar.items)
                .iter()
                .filter(|word| *word == "let ring")
                .count();
            hooks += lines(&bar.items)
                .iter()
                .filter(|line| {
                    line.dash.is_none()
                        && line.from.x == line.to.x
                        && line.to.y > line.from.y
                        && line.from.y < row.staff.top
                })
                .count();
        }
    }
    assert_eq!(labels, systems);
    assert_eq!(hooks, 1, "only the end of the run is hooked");
}

fn beat_effects_score() -> Score {
    let mut score = bars_of_quarters(&[
        &[(2, 2), (2, 2), (2, 2), (2, 2)],
        &[(2, 2), (2, 2), (2, 2), (2, 2)],
    ]);
    let values = [
        NoteValue::Eighth,
        NoteValue::Sixteenth,
        NoteValue::ThirtySecond,
    ];
    for (beat, value) in values.into_iter().enumerate() {
        beat_effects(&mut score, 0, beat).tremolo_picking =
            Some(TremoloPicking::new(value).unwrap());
    }
    for beat in 0..2 {
        for string in [0_u8, 1, 3] {
            let id = score.alloc().unwrap();
            let notes = &mut score.tracks[0].staves[0].bars[1].voices[0].beats[beat].notes;
            let mut note = notes[0].clone();
            note.kind = NoteKind::Fretted { string, fret: 0 };
            note.id = id;
            notes.push(note);
        }
    }
    beat_effects(&mut score, 1, 0).stroke = Some(Stroke {
        direction: StrokeDirection::Down,
        duration: StrokeTicks::default(),
    });
    beat_effects(&mut score, 1, 1).stroke = Some(Stroke {
        direction: StrokeDirection::Up,
        duration: StrokeTicks::default(),
    });
    note_at(&mut score, 1, 0, 2, 0).effects.left_hand_finger = Some(Finger::Index);
    note_at(&mut score, 1, 0, 3, 0).effects.left_hand_finger = Some(Finger::Thumb);
    score
}

#[test]
fn tremolo_picking_slashes_the_stem_of_whichever_staff_carries_the_rhythm() {
    let score = beat_effects_score();
    let both = render(&score, &options(StaffMode::Both));
    let names = ["tremolo1", "tremolo2", "tremolo3"];
    let standard_bar = crate::music::standard_rows(&both)[0].2.bars[0];
    for (beat, name) in names.iter().enumerate() {
        let id = score.tracks[0].staves[0].bars[0].voices[0].beats[beat].id;
        let group = find_group(&standard_bar.items, GroupId(id.0.0)).unwrap();
        assert!(glyph_names(&group.items).contains(name));
        let tab = beat_group(tab_bar(&both, 0).1, &score, 0, beat);
        assert!(
            !glyph_names(&tab.items)
                .iter()
                .any(|found| found.starts_with("tremolo"))
        );
    }
    let tab_only = render(&score, &options(StaffMode::Tab));
    let (staff, bar) = tab_bar(&tab_only, 0);
    for (beat, name) in names.iter().enumerate() {
        let group = beat_group(bar, &score, 0, beat);
        let slashes: Vec<PlacedGlyph> = glyphs(&group.items)
            .into_iter()
            .filter(|glyph| crate::support::glyph_name(glyph.codepoint) == *name)
            .collect();
        assert_eq!(slashes.len(), 1);
        assert!(
            slashes[0].origin.y > staff.bottom(),
            "on the stem under the tab"
        );
    }
    insta::assert_snapshot!(focused(&score, StaffMode::Both));
}

#[test]
fn strokes_point_their_arrow_along_the_strum() {
    let score = beat_effects_score();
    let list = render(&score, &options(StaffMode::Both));
    let (staff, bar) = tab_bar(&list, 1);
    let down = beat_group(bar, &score, 1, 0);
    let up = beat_group(bar, &score, 1, 1);
    assert_eq!(glyph_names(&down.items), vec!["arrowheadBlackUp"]);
    assert_eq!(glyph_names(&up.items), vec!["arrowheadBlackDown"]);
    let arrow = glyphs(&down.items)[0];
    assert!(ink(&arrow).min.y < staff.line_y(0));
    let shaft = lines(&down.items)[0];
    assert!(shaft.to.y > staff.line_y(3));
    let label = note_groups(bar)[&note_id(&score, 1, 0, 0, 0).0.0][0].white;
    assert!(
        ink(&arrow).max.x <= label.min.x,
        "beside the chord, on its left"
    );
}

#[test]
fn fingering_goes_left_of_the_head_or_above_the_fret_when_only_tab_is_shown() {
    let score = beat_effects_score();
    let both = render(&score, &options(StaffMode::Both));
    let standard_bar = crate::music::standard_rows(&both)[0].2.bars[1];
    let id = score.tracks[0].staves[0].bars[1].voices[0].beats[2].id;
    let group = find_group(&standard_bar.items, GroupId(id.0.0)).unwrap();
    let finger = texts(&group.items)
        .into_iter()
        .find(|run| run.text == "1")
        .expect("a fingering number");
    let head = crate::music::head_of(&both, note_id(&score, 1, 0, 2, 0));
    assert!(finger.origin.x < head.origin.x);
    let thumb = texts(
        &find_group(
            &standard_bar.items,
            GroupId(score.tracks[0].staves[0].bars[1].voices[0].beats[3].id.0.0),
        )
        .unwrap()
        .items,
    );
    assert!(thumb.iter().any(|run| run.text == "T"));
    let (_, tab) = tab_bar(&both, 1);
    assert!(!words(&beat_group(tab, &score, 1, 2).items).contains(&"1".to_string()));
    let alone = render(&score, &options(StaffMode::Tab));
    let (_, tab) = tab_bar(&alone, 1);
    let label = &note_groups(tab)[&note_id(&score, 1, 0, 2, 0).0.0][0];
    let number = texts(&beat_group(tab, &score, 1, 2).items)
        .into_iter()
        .find(|run| run.text == "1" && run.em < label.text.em)
        .expect("a small fingering number on the tab");
    assert!(number.origin.x <= label.white.min.x);
    assert!(number.origin.y < label.text.origin.y);
    insta::assert_snapshot!(focused(&score, StaffMode::Tab));
}
