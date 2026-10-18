use opentab_layout::{
    BarRange, ClefSign, DEFAULT_STAFF_SPACE, DURATION_WIDTHS, LayoutError, LayoutOptions,
    Micrometres, PageSize, Primitive, StaffKind, StaffMode, TAB_LINE_SPACING, Units, layout,
};
use opentab_model::{Clef, InstrumentKind, KeySignature, NoteValue, Score};

use crate::support::{
    BarSpec, Item, add_track, check, guitar, lines, note, quarter_bars, systems, texts, whole_bars,
};

fn laid_out(score: &Score) -> opentab_layout::DisplayList {
    let options = LayoutOptions::default();
    let list = layout(score, &options).unwrap();
    check(score, &options, &list);
    list
}

#[test]
fn every_system_but_the_last_is_justified_to_the_content_width() {
    let score = guitar(&quarter_bars(40));
    let list = laid_out(&score);
    let all = systems(&list);
    assert!(all.len() >= 3, "{}", all.len());
    for system in &all[..all.len() - 1] {
        assert_eq!(system.rect.width(), system.available);
        assert!(system.natural < system.available);
    }
}

#[test]
fn justification_adds_space_in_proportion_to_duration() {
    let mut specs = quarter_bars(30);
    specs[0].voices = vec![vec![
        note(NoteValue::Half),
        note(NoteValue::Quarter),
        note(NoteValue::Quarter),
    ]];
    let score = guitar(&specs);
    let list = laid_out(&score);
    let first = &systems(&list)[0];
    let bar = &first.staves[0].bars[0];
    let gaps: Vec<i64> = bar
        .columns
        .windows(2)
        .map(|pair| (pair[1].x - pair[0].x).0)
        .collect();
    let half = DURATION_WIDTHS[6].0;
    let quarter = DURATION_WIDTHS[5].0;
    let extra_half = gaps[0] - half;
    let extra_quarter = gaps[1] - quarter;
    assert!(extra_quarter > 0);
    let cross =
        i128::from(extra_half) * i128::from(quarter) - i128::from(extra_quarter) * i128::from(half);
    assert!(cross.abs() <= i128::from(half) * 2, "{gaps:?}");
    let other = &first.staves[0].bars[1];
    let quarter_gap = (other.columns[1].x - other.columns[0].x).0;
    assert!((quarter_gap - gaps[1]).abs() <= 1, "{quarter_gap} {gaps:?}");
}

#[test]
fn the_last_system_stretches_to_one_and_a_half_times_its_natural_width_at_most() {
    let short = guitar(&whole_bars(2));
    let list = laid_out(&short);
    let only = systems(&list)[0];
    assert_eq!(only.rect.width(), Units(only.natural.0 * 3 / 2));
    assert!(only.rect.width() < only.available);

    let mut wide = 1;
    loop {
        let score = guitar(&quarter_bars(wide));
        let list = laid_out(&score);
        let all = systems(&list);
        let last = all[all.len() - 1];
        if all.len() == 1 && last.natural.0 * 3 / 2 > last.available.0 {
            assert_eq!(last.rect.width(), last.available);
            break;
        }
        assert!(
            all.len() == 1,
            "no single system reached two thirds of the width"
        );
        wide += 1;
    }
}

#[test]
fn a_dense_bar_with_tuplets_stays_inside_the_system() {
    let sextuplets = vec![Item::Note(NoteValue::ThirtySecond, 0, Some((6, 4))); 48];
    let quintuplets = vec![Item::Note(NoteValue::SixtyFourth, 0, Some((5, 4))); 80];
    let septuplets = vec![Item::Note(NoteValue::SixtyFourth, 1, Some((7, 4))); 56];
    let mut specs = quarter_bars(3);
    specs.insert(
        1,
        BarSpec {
            time: (4, 4),
            voices: vec![sextuplets, quintuplets],
        },
    );
    specs.push(BarSpec {
        time: (3, 4),
        voices: vec![septuplets],
    });
    let score = guitar(&specs);
    laid_out(&score);
}

#[test]
fn a_bar_wider_than_the_page_gets_its_own_compressed_system() {
    let mut specs = quarter_bars(2);
    specs.insert(
        1,
        BarSpec {
            time: (16, 4),
            voices: vec![vec![note(NoteValue::OneTwentyEighth); 512]],
        },
    );
    let score = guitar(&specs);
    let list = laid_out(&score);
    let all = systems(&list);
    let holder = all
        .iter()
        .find(|system| system.staves[0].bars.iter().any(|bar| bar.index == 1))
        .unwrap();
    assert_eq!(holder.staves[0].bars.len(), 1);
    assert!(holder.natural > holder.available);
    assert_eq!(holder.rect.width(), holder.available);
    let bar = &holder.staves[0].bars[0];
    assert_eq!(bar.columns.len(), 512);
    assert!(bar.columns.last().unwrap().x < bar.rect.max.x);
}

#[test]
fn nothing_to_draw_is_an_error_not_a_panic() {
    let options = LayoutOptions::default();
    assert_eq!(layout(&Score::new(), &options), Err(LayoutError::NoBars));
    let mut no_tracks = guitar(&whole_bars(2));
    no_tracks.tracks.clear();
    assert_eq!(layout(&no_tracks, &options), Err(LayoutError::NoStaves));
    let mut no_staves = guitar(&whole_bars(2));
    no_staves.tracks[0].staves.clear();
    assert_eq!(layout(&no_staves, &options), Err(LayoutError::NoStaves));
    let mut no_bars = guitar(&whole_bars(1));
    no_bars.master_bars.clear();
    assert_eq!(layout(&no_bars, &options), Err(LayoutError::NoBars));
}

#[test]
fn empty_tracks_bars_without_voices_and_grace_only_voices_render() {
    let mut specs = whole_bars(4);
    specs[1].voices.clear();
    specs[2].voices = vec![vec![Item::Grace(NoteValue::Eighth)], Vec::new()];
    let mut score = guitar(&specs);
    let empty = add_track(&mut score, "Empty", 6, InstrumentKind::Fretted);
    score.tracks[empty].staves.clear();
    score.tracks[0].staves[0].bars.truncate(3);
    let list = laid_out(&score);
    let bars = &systems(&list)[0].staves[0].bars;
    assert!(bars[1].columns.is_empty());
    assert_eq!(bars[2].columns.len(), 1);
    assert_eq!(bars[2].columns[0].graces, 1);
    assert!(bars[3].columns.is_empty());
    assert!(bars[1].inline_width >= opentab_layout::EMPTY_BAR);
    assert!(bars[3].inline_width >= opentab_layout::EMPTY_BAR);
    assert_eq!(systems(&list)[0].staves.len(), 2);
}

#[test]
fn bad_options_are_refused() {
    let score = guitar(&whole_bars(3));
    let with = |change: &dyn Fn(&mut LayoutOptions)| {
        let mut options = LayoutOptions::default();
        change(&mut options);
        layout(&score, &options)
    };
    assert_eq!(
        with(&|options| options.staff_space = Micrometres(0)),
        Err(LayoutError::StaffSpace { micrometres: 0 })
    );
    assert_eq!(
        with(&|options| options.staff_space = Micrometres(-5)),
        Err(LayoutError::StaffSpace { micrometres: -5 })
    );
    assert_eq!(
        with(&|options| options.tracks = Some(vec![0, 1])),
        Err(LayoutError::Track {
            index: 1,
            tracks: 1
        })
    );
    assert_eq!(
        with(&|options| options.bars = Some(BarRange { first: 2, last: 3 })),
        Err(LayoutError::Bars {
            first: 2,
            last: 3,
            bars: 3
        })
    );
    assert_eq!(
        with(&|options| options.bars = Some(BarRange { first: 2, last: 1 })),
        Err(LayoutError::Bars {
            first: 2,
            last: 1,
            bars: 3
        })
    );
    assert!(matches!(
        with(&|options| {
            options.page = PageSize {
                width: Micrometres(30_000),
                height: Micrometres(200_000),
            }
        }),
        Err(LayoutError::PageSize { .. })
    ));
    assert_eq!(
        with(&|options| {
            options.page = PageSize {
                width: Micrometres(40_000),
                height: Micrometres(297_000),
            }
        }),
        Err(LayoutError::PageTooSmall)
    );
}

#[test]
fn a_selection_of_tracks_and_bars_draws_only_those() {
    let mut score = guitar(&whole_bars(6));
    add_track(&mut score, "Bass", 4, InstrumentKind::Fretted);
    add_track(&mut score, "Drums", 0, InstrumentKind::Percussion);
    let options = LayoutOptions {
        tracks: Some(vec![2, 0, 2]),
        bars: Some(BarRange { first: 2, last: 4 }),
        ..LayoutOptions::default()
    };
    let list = layout(&score, &options).unwrap();
    check(&score, &options, &list);
    let first = systems(&list)[0];
    let staves: Vec<(usize, StaffKind)> = first
        .staves
        .iter()
        .map(|staff| (staff.track, staff.kind))
        .collect();
    assert_eq!(
        staves,
        vec![
            (0, StaffKind::Standard),
            (0, StaffKind::Tab),
            (2, StaffKind::Standard)
        ]
    );
    assert_eq!(first.staves[2].bars[0].clef, ClefSign::Neutral);
    assert_eq!(first.staves[0].bars[0].clef, ClefSign::Treble8vb);
    assert_eq!(first.staves[1].bars[0].clef, ClefSign::Tab);
    assert_eq!(first.staves[0].bars[0].number, 3);
    let page = &list.pages[0];
    assert!(texts(page).contains(&"3".to_string()));
    assert!(texts(page).contains(&"Gtr".to_string()));
    assert!(texts(page).contains(&"Dru".to_string()));
}

#[test]
fn staff_modes_follow_the_string_count_unless_forced() {
    let mut score = guitar(&whole_bars(1));
    add_track(&mut score, "Nine", 9, InstrumentKind::Fretted);
    add_track(&mut score, "Three", 3, InstrumentKind::Fretted);
    add_track(&mut score, "Drums", 6, InstrumentKind::Percussion);
    let kinds = |mode: Option<StaffMode>| {
        let options = LayoutOptions {
            staff_mode: mode,
            ..LayoutOptions::default()
        };
        let list = layout(&score, &options).unwrap();
        check(&score, &options, &list);
        systems(&list)[0]
            .staves
            .iter()
            .map(|staff| (staff.track, staff.kind, staff.lines))
            .collect::<Vec<_>>()
    };
    use StaffKind::{Standard, Tab};
    assert_eq!(
        kinds(None),
        vec![
            (0, Standard, 5),
            (0, Tab, 6),
            (1, Standard, 5),
            (2, Standard, 5),
            (3, Standard, 5)
        ]
    );
    assert_eq!(
        kinds(Some(StaffMode::Tab)),
        vec![(0, Tab, 6), (1, Tab, 9), (2, Tab, 3), (3, Standard, 5)]
    );
    assert_eq!(kinds(Some(StaffMode::Both)).len(), 7);
    assert_eq!(kinds(Some(StaffMode::Standard)).len(), 4);
}

#[test]
fn tab_staves_space_their_lines_one_and_a_half_staff_spaces_apart() {
    let score = guitar(&whole_bars(1));
    let list = laid_out(&score);
    let tab = &systems(&list)[0].staves[1];
    assert_eq!(tab.line_spacing, TAB_LINE_SPACING);
    assert_eq!(TAB_LINE_SPACING, Units(1536));
    assert_eq!(tab.line_y(5) - tab.line_y(0), Units(1536 * 5));
}

#[test]
fn a_pickup_bar_is_numbered_zero_and_numbers_mark_system_starts() {
    let mut specs = quarter_bars(40);
    specs[0].voices = vec![vec![note(NoteValue::Quarter)]];
    let mut score = guitar(&specs);
    score.anacrusis = true;
    let list = laid_out(&score);
    let all = systems(&list);
    let first = &all[0].staves[0].bars;
    assert_eq!((first[0].number, first[1].number), (0, 1));
    assert!(first[0].inline_width < first[1].inline_width);
    let page = &list.pages[0];
    let numbers = texts(page);
    assert!(!numbers.contains(&"0".to_string()));
    for system in &all[1..] {
        let number = system.staves[0].bars[0].number.to_string();
        let page = list
            .pages
            .iter()
            .find(|page| page.systems.contains(system))
            .unwrap();
        assert!(texts(page).contains(&number), "{number}");
    }
}

#[test]
fn repeats_endings_and_double_bars_are_drawn() {
    let mut score = guitar(&whole_bars(5));
    score.master_bars[0].repeat_start = true;
    score.master_bars[1].repeat_end = Some(3);
    score.master_bars[1].alternate_endings = 0b011;
    score.master_bars[2].alternate_endings = 0b100;
    score.master_bars[2].double_bar = true;
    score.master_bars[3].repeat_end = Some(2);
    let list = laid_out(&score);
    let page = &list.pages[0];
    let words = texts(page);
    assert!(words.contains(&"\u{d7}3".to_string()), "{words:?}");
    assert!(!words.contains(&"\u{d7}2".to_string()));
    assert!(words.contains(&"1, 2.".to_string()));
    assert!(words.contains(&"3.".to_string()));
    let dots = opentab_layout::glyphs::REPEAT_DOT.codepoint;
    let count = crate::support::flatten(&page.items)
        .into_iter()
        .filter(|item| matches!(item, Primitive::Glyph(glyph) if glyph.codepoint == dots))
        .count();
    assert_eq!(count, 3 * 2 * 2);
    let staff = &systems(&list)[0].staves[0];
    let (top, bottom) = staff.span();
    let vertical_at = |x: Units| {
        lines(page)
            .iter()
            .filter(|line| line.from.x == x && line.from.y == top && line.to.y == bottom)
            .count()
    };
    let thin = Units(164);
    let gap = Units(410);
    let double = staff.bars[2].rect.max.x;
    assert_eq!(vertical_at(double - thin.half()), 1);
    assert_eq!(vertical_at(double - thin - gap - thin.half()), 1);
}

#[test]
fn clefs_keys_and_times_appear_at_system_starts_and_changes() {
    let mut specs = whole_bars(3);
    specs[2].time = (3, 4);
    specs[2].voices = vec![vec![Item::Note(NoteValue::Half, 1, None)]];
    let mut score = guitar(&specs);
    score.master_bars[1].key_signature = KeySignature::new(3, false).unwrap();
    score.tracks[0].staves[0].bars[2].clef = Clef::Bass;
    let list = laid_out(&score);
    let staff = &systems(&list)[0].staves[0];
    let clefs: Vec<ClefSign> = staff.bars.iter().map(|bar| bar.clef).collect();
    assert_eq!(
        clefs,
        vec![ClefSign::Treble8vb, ClefSign::Treble8vb, ClefSign::Bass]
    );
    let glyphs: Vec<char> = crate::support::flatten(&list.pages[0].items)
        .into_iter()
        .filter_map(|item| match item {
            Primitive::Glyph(glyph) => Some(glyph.codepoint),
            _ => None,
        })
        .collect();
    let count = |codepoint: char| glyphs.iter().filter(|glyph| **glyph == codepoint).count();
    assert_eq!(count('\u{E052}'), 1);
    assert_eq!(count('\u{E062}'), 1);
    assert_eq!(count('\u{E06D}'), 1);
    assert_eq!(count('\u{E262}'), 3);
    assert_eq!(count('\u{E084}'), 2 * 2 + 2);
    assert_eq!(count('\u{E083}'), 2);
    assert_eq!(count('\u{E261}'), 3);
}

#[test]
fn a_tall_system_shrinks_the_staff_space_to_fit_the_page() {
    let mut score = guitar(&whole_bars(2));
    for index in 0..12 {
        add_track(&mut score, &format!("T{index}"), 7, InstrumentKind::Fretted);
    }
    let list = laid_out(&score);
    assert!(list.pages[0].staff_space < DEFAULT_STAFF_SPACE);
    assert_eq!(systems(&list).len(), 1);
}

#[test]
fn layout_is_deterministic() {
    let mut score = guitar(&quarter_bars(30));
    add_track(&mut score, "Bass", 4, InstrumentKind::Fretted);
    let options = LayoutOptions::default();
    assert_eq!(
        layout(&score, &options).unwrap(),
        layout(&score, &options).unwrap()
    );
}
