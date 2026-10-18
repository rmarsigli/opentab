use opentab_layout::marks::{DroppedMark, MarkKind};
use opentab_layout::{DisplayList, Primitive, StaffMode};
use opentab_model::{Dynamic, Marker, NoteValue, Score};

use crate::music::{build, measure, options, play, render};
use crate::tab_effects::quarters;

fn quarter_score(bars: usize) -> Score {
    let measures: Vec<_> = (0..bars)
        .map(|_| measure((4, 4), 0, vec![(0, quarters(&[(2, 5); 4]))]))
        .collect();
    build(&measures)
}

fn dropped(list: &DisplayList, kind: MarkKind) -> Vec<Option<usize>> {
    list.dropped
        .iter()
        .filter(|mark| mark.kind == kind)
        .map(|mark| mark.bar)
        .collect()
}

fn texts(list: &DisplayList) -> Vec<String> {
    list.pages
        .iter()
        .flat_map(|page| &page.items)
        .filter_map(|item| match item {
            Primitive::Text(run) => Some(run.text.clone()),
            _ => None,
        })
        .collect()
}

fn marker(score: &mut Score, bar: usize, letter: &str, text: &str) {
    score.master_bars[bar].marker = Some(Marker {
        letter: letter.to_string(),
        text: text.to_string(),
    });
}

#[test]
fn a_section_left_without_room_by_a_long_one_before_it_is_reported() {
    let mut score = quarter_score(4);
    marker(&mut score, 0, "A", &"Intro ".repeat(200));
    marker(&mut score, 1, "B", "Verse");
    let list = render(&score, &options(StaffMode::Standard));
    assert_eq!(list.pages[0].systems.len(), 1);
    assert_eq!(dropped(&list, MarkKind::Section), vec![Some(1)]);
    let drawn = texts(&list);
    assert!(drawn.contains(&"A".to_string()), "{drawn:?}");
    assert!(!drawn.contains(&"B".to_string()), "{drawn:?}");
    assert!(!drawn.contains(&"Verse".to_string()), "{drawn:?}");
}

#[test]
fn a_section_whose_text_has_no_ink_keeps_its_box_and_is_reported() {
    let mut score = quarter_score(2);
    marker(&mut score, 1, "C", "\u{200B}\u{200B}");
    let list = render(&score, &options(StaffMode::Standard));
    assert_eq!(dropped(&list, MarkKind::Section), vec![Some(1)]);
    assert!(texts(&list).contains(&"C".to_string()));
    let mut whole = quarter_score(2);
    marker(&mut whole, 1, "C", "");
    let list = render(&whole, &options(StaffMode::Standard));
    assert!(list.dropped.is_empty(), "{:?}", list.dropped);
}

#[test]
fn free_text_that_cannot_be_drawn_is_reported_instead_of_skipped() {
    let mut score = quarter_score(2);
    score.tracks[0].staves[0].bars[1].voices[0].beats[2]
        .effects
        .text = Some("\u{200B}".to_string());
    let list = render(&score, &options(StaffMode::Standard));
    assert_eq!(dropped(&list, MarkKind::Text), vec![Some(1)]);
}

#[test]
fn dynamics_crowded_out_of_a_dense_bar_are_reported_per_bar() {
    let notes: Vec<_> = (0..128)
        .map(|_| play(NoteValue::OneTwentyEighth, &[(2, 5)]))
        .collect();
    let mut score = build(&[
        measure((4, 4), 0, vec![(0, quarters(&[(2, 5); 4]))]),
        measure((4, 4), 0, vec![(0, notes)]),
    ]);
    for (index, beat) in score.tracks[0].staves[0].bars[1].voices[0]
        .beats
        .iter_mut()
        .enumerate()
    {
        beat.effects.dynamic = if index % 2 == 0 {
            Dynamic::Ppp
        } else {
            Dynamic::Fff
        };
    }
    let list = render(&score, &options(StaffMode::Standard));
    assert_eq!(list.crowded, vec![1]);
    let bars = dropped(&list, MarkKind::Dynamic);
    assert!(!bars.is_empty());
    assert!(bars.iter().all(|bar| *bar == Some(1)), "{bars:?}");
    let drawn = list
        .pages
        .iter()
        .flat_map(|page| &page.items)
        .filter(|item| {
            matches!(item, Primitive::Glyph(glyph) if ('\u{E520}'..='\u{E54F}').contains(&glyph.codepoint))
        })
        .count();
    assert!(drawn > 0);
    assert_eq!(drawn + bars.len(), 128);
}

#[test]
fn a_title_block_cut_short_is_reported_once_without_a_bar() {
    let mut score = quarter_score(1);
    score.meta.title = "Song ".repeat(300);
    score.meta.artist = "Somebody".to_string();
    let list = render(&score, &options(StaffMode::Standard));
    assert_eq!(
        list.dropped,
        vec![DroppedMark {
            bar: None,
            kind: MarkKind::Title
        }]
    );
    let mut short = quarter_score(1);
    short.meta.title = "Song".to_string();
    short.meta.subtitle = "Live".to_string();
    short.meta.words = "A. Writer".to_string();
    short.meta.tab_author = "C. Tabber".to_string();
    let list = render(&short, &options(StaffMode::Standard));
    assert!(list.dropped.is_empty(), "{:?}", list.dropped);
    let mut credit = quarter_score(1);
    credit.meta.music = "Composer ".repeat(100);
    let list = render(&credit, &options(StaffMode::Standard));
    assert_eq!(dropped(&list, MarkKind::Title), vec![None]);
}

#[test]
fn a_section_at_a_system_start_clears_the_bracket_and_the_clef() {
    let mut score = quarter_score(2);
    crate::support::add_track(
        &mut score,
        "Bass",
        4,
        opentab_model::InstrumentKind::Fretted,
    );
    marker(&mut score, 0, "", "Intro");
    marker(&mut score, 1, "B", "Verse");
    let list = render(&score, &options(StaffMode::Standard));
    let page = &list.pages[0];
    let found = crate::music::glyphs(&page.items);
    let furniture: Vec<opentab_layout::Rect> = found
        .iter()
        .filter(|glyph| {
            matches!(
                crate::support::glyph_name(glyph.codepoint),
                "bracketTop" | "gClef" | "gClef8vb"
            )
        })
        .map(crate::support::ink)
        .collect();
    assert!(furniture.len() >= 3, "{}", furniture.len());
    let top = furniture.iter().map(|rect| rect.min.y).min().unwrap();
    for text in ["Intro", "B", "Verse"] {
        let run = page
            .items
            .iter()
            .find_map(|item| match item {
                Primitive::Text(run) if run.text == text => Some(run.clone()),
                _ => None,
            })
            .unwrap();
        let ink = crate::marks::text_ink(&run).unwrap();
        assert!(
            ink.max.y < top,
            "{text} at {ink:?} reaches the furniture at {top:?}"
        );
    }
}
