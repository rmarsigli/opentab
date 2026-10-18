use std::cell::RefCell;

use opentab_layout::marks::{
    BLOCK_GAP, CHORD_EM, ELLIPSIS, MarkKind, SECTION_EM, TEXT_EM, TITLE_EM, clean, credits,
    ink as visible_ink, line_height, tab_credit,
};
use opentab_layout::text::text_font;
use opentab_layout::{
    DisplayList, LayoutOptions, MIN_STAFF_SPACE, Micrometres, NAME_EM, Page, PageSize, PathSegment,
    Point, Primitive, Rect, StaffMode, TextAnchor, TextRun, Units, layout,
};
use opentab_model::{
    Fraction, InstrumentKind, Marker, Score, ScoreMeta, Tempo, TempoChange, TempoUnit, VoiceSlot,
};
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::select;
use proptest::test_runner::TestRunner;

use crate::support::{
    add_track, check, content, dump, flatten, guitar, ink, quarter_bars, systems,
};

pub fn text_ink(run: &TextRun) -> Option<Rect> {
    let font = text_font();
    let positions = font.pen_positions(&run.text, run.em);
    let width = positions.last().copied().unwrap_or_default();
    let shift = match run.anchor {
        TextAnchor::Start => Units::ZERO,
        TextAnchor::Middle => width.half(),
        TextAnchor::End => width,
    };
    let mut found: Option<Rect> = None;
    for (character, pen) in run.text.chars().zip(positions) {
        let origin = Point {
            x: run.origin.x + pen - shift,
            y: run.origin.y,
        };
        let Some(rect) = shape_box(&font.outline(character).placed(origin, run.em)) else {
            continue;
        };
        found = Some(match found {
            Some(known) => known.including(rect.min).including(rect.max),
            None => rect,
        });
    }
    found
}

pub fn shape_box(segments: &[PathSegment]) -> Option<Rect> {
    let mut found: Option<Rect> = None;
    for segment in segments {
        let points = match *segment {
            PathSegment::MoveTo(to) | PathSegment::LineTo(to) => vec![to],
            PathSegment::QuadTo(control, to) => vec![control, to],
            PathSegment::CubicTo(first, second, to) => vec![first, second, to],
            PathSegment::Close => Vec::new(),
        };
        for point in points {
            found = Some(match found {
                Some(known) => known.including(point),
                None => Rect::new(point, point),
            });
        }
    }
    found
}

pub fn top_texts(page: &Page) -> Vec<&TextRun> {
    page.items
        .iter()
        .filter_map(|item| match item {
            Primitive::Text(run) => Some(run),
            _ => None,
        })
        .collect()
}

pub fn block_texts(page: &Page) -> Vec<&TextRun> {
    top_texts(page)
        .into_iter()
        .filter(|run| !run.text.starts_with("= "))
        .collect()
}

pub fn inside(outer: Rect, inner: Rect) -> bool {
    outer.min.x <= inner.min.x
        && outer.min.y <= inner.min.y
        && inner.max.x <= outer.max.x
        && inner.max.y <= outer.max.y
}

pub fn assert_ink_inside_content(list: &DisplayList) -> usize {
    let mut checked = 0;
    for page in &list.pages {
        let area = content(page);
        for item in flatten(&page.items) {
            let rect = match item {
                Primitive::Text(run) => text_ink(run),
                Primitive::Glyph(glyph) => Some(ink(glyph)),
                Primitive::Shape(shape) => shape_box(&shape.segments),
                _ => None,
            };
            if let Some(rect) = rect {
                assert!(inside(area, rect), "{item:?} ink {rect:?} outside {area:?}");
                checked += 1;
            }
        }
    }
    checked
}

fn meta(title: &str) -> ScoreMeta {
    ScoreMeta {
        title: title.to_string(),
        ..ScoreMeta::default()
    }
}

fn full_meta() -> ScoreMeta {
    ScoreMeta {
        title: "Song Title".to_string(),
        subtitle: "A Subtitle".to_string(),
        artist: "The Artist".to_string(),
        album: "The Album".to_string(),
        words: "W. Writer".to_string(),
        music: "M. Maker".to_string(),
        tab_author: "T. Tabber".to_string(),
        ..ScoreMeta::default()
    }
}

fn named<'a>(page: &'a Page, text: &str) -> &'a TextRun {
    top_texts(page)
        .into_iter()
        .find(|run| run.text == text)
        .unwrap_or_else(|| panic!("no text {text:?}"))
}

fn render(score: &Score) -> DisplayList {
    let options = LayoutOptions::default();
    let list = layout(score, &options).unwrap();
    check(score, &options, &list);
    list
}

#[test]
fn the_title_block_centres_titles_and_places_the_credits_at_the_sides() {
    let mut score = guitar(&quarter_bars(2));
    score.meta = full_meta();
    let list = render(&score);
    let page = &list.pages[0];
    let area = content(page);
    let centre = area.min.x + (area.max.x - area.min.x).half();
    let mut previous = area.min.y;
    for text in ["Song Title", "A Subtitle", "The Artist", "The Album"] {
        let run = named(page, text);
        let rect = text_ink(run).unwrap();
        let middle = rect.min.x + rect.width().half();
        assert!((middle - centre).0.abs() <= 1, "{text} centred");
        assert!(run.origin.y > previous, "{text} below the line before");
        previous = run.origin.y;
    }
    let title = text_ink(named(page, "Song Title")).unwrap();
    let artist = text_ink(named(page, "The Artist")).unwrap();
    assert!(
        title.height().0 > artist.height().0 * 2,
        "the title is large"
    );
    let tab = text_ink(named(page, "Tab by T. Tabber")).unwrap();
    assert_eq!(tab.min.x, area.min.x);
    let words = text_ink(named(page, "Words by W. Writer")).unwrap();
    let music = text_ink(named(page, "Music by M. Maker")).unwrap();
    assert_eq!(words.max.x, area.max.x);
    assert_eq!(music.max.x, area.max.x);
    assert!(music.min.y > words.max.y);
    let lowest = block_texts(page)
        .into_iter()
        .filter_map(text_ink)
        .map(|rect| rect.max.y)
        .max()
        .unwrap();
    assert!(
        systems(&list)[0].rect.min.y > lowest,
        "the system starts below the block"
    );
    insta::assert_snapshot!(dump(&list));
}

#[test]
fn equal_words_and_music_share_one_credit_and_empty_fields_take_no_space() {
    let mut score = guitar(&quarter_bars(2));
    score.meta = ScoreMeta {
        words: "Same Person".to_string(),
        music: "Same Person".to_string(),
        ..ScoreMeta::default()
    };
    let list = render(&score);
    let texts: Vec<&str> = block_texts(&list.pages[0])
        .into_iter()
        .map(|run| run.text.as_str())
        .collect();
    assert_eq!(texts, vec!["Words & Music by Same Person"]);
    let bare = render(&guitar(&quarter_bars(2)));
    let tempo: Vec<&str> = top_texts(&bare.pages[0])
        .into_iter()
        .map(|run| run.text.as_str())
        .collect();
    assert_eq!(tempo, vec!["= 120"]);
    let mut titled = guitar(&quarter_bars(2));
    titled.meta = meta("Only a title");
    let with_title = render(&titled);
    assert_eq!(
        systems(&with_title)[0].rect.min.y - systems(&bare)[0].rect.min.y,
        line_height(TITLE_EM) + BLOCK_GAP
    );
}

#[test]
fn the_title_block_moves_systems_to_later_pages() {
    let mut score = guitar(&quarter_bars(80));
    let plain = render(&score);
    score.meta = ScoreMeta {
        title: "word ".repeat(60),
        subtitle: "other ".repeat(40),
        ..full_meta()
    };
    let titled = render(&score);
    assert!(titled.pages[0].systems.len() < plain.pages[0].systems.len());
    assert_eq!(systems(&titled).len(), systems(&plain).len());
    for (number, page) in titled.pages.iter().enumerate().skip(1) {
        assert!(
            top_texts(page).iter().all(|run| run.em != TITLE_EM),
            "page {number}"
        );
    }
}

#[test]
fn a_300_character_title_wraps_at_words_within_the_margins() {
    let words = [
        "lorem",
        "ipsum",
        "dolor",
        "sit",
        "amet",
        "consectetur",
        "adipiscing",
        "elit",
    ];
    let mut title = String::new();
    let mut index = 0;
    while title.chars().count() < 300 {
        title.push_str(words[index % words.len()]);
        title.push(' ');
        index += 1;
    }
    let title: String = title.chars().take(300).collect();
    let mut score = guitar(&quarter_bars(2));
    score.meta = meta(&title);
    let list = render(&score);
    let lines: Vec<&TextRun> = top_texts(&list.pages[0])
        .into_iter()
        .filter(|run| run.em == TITLE_EM)
        .collect();
    assert_eq!(lines.len(), 3);
    for (number, line) in lines.iter().enumerate() {
        let text = line.text.trim_end_matches(ELLIPSIS);
        for word in text.split(' ') {
            let last = number + 1 == lines.len() && text.ends_with(word);
            assert!(words.contains(&word) || last, "broken word {word:?}");
        }
    }
    assert!(lines[2].text.ends_with(ELLIPSIS));
    assert!(lines[0].origin.y < lines[1].origin.y && lines[1].origin.y < lines[2].origin.y);
    assert!(assert_ink_inside_content(&list) > 3);
}

#[test]
fn a_2000_character_word_breaks_by_characters_within_the_margins() {
    let word = "W".repeat(2000);
    let mut score = guitar(&quarter_bars(2));
    score.meta = ScoreMeta {
        title: word.clone(),
        subtitle: word.clone(),
        artist: word.clone(),
        words: word.clone(),
        music: format!("{word}x"),
        tab_author: word.clone(),
        ..ScoreMeta::default()
    };
    let list = render(&score);
    let lines: Vec<&TextRun> = top_texts(&list.pages[0])
        .into_iter()
        .filter(|run| run.em == TITLE_EM)
        .collect();
    assert_eq!(lines.len(), 3);
    assert!(lines[0].text.chars().all(|character| character == 'W'));
    assert!(lines[0].text.chars().count() > 10);
    assert_eq!(lines[0].text, lines[1].text);
    assert!(lines[2].text.ends_with(ELLIPSIS));
    let area = content(&list.pages[0]);
    let first = text_ink(lines[0]).unwrap();
    let wider = TextRun {
        text: format!("{}W", lines[0].text),
        ..lines[0].clone()
    };
    assert!(
        text_ink(&wider).unwrap().width() > area.width(),
        "the break is maximal"
    );
    assert!(first.width() <= area.width());
    assert!(assert_ink_inside_content(&list) > 8);
}

#[test]
fn a_title_and_a_tall_first_system_share_page_one_at_a_smaller_staff_space() {
    let mut score = guitar(&quarter_bars(2));
    for index in 0..4 {
        add_track(
            &mut score,
            &format!("Guitar {index}"),
            6,
            InstrumentKind::Fretted,
        );
    }
    let plain = render(&score);
    score.meta = ScoreMeta {
        title: "word ".repeat(60),
        subtitle: "other ".repeat(40),
        ..full_meta()
    };
    let titled = render(&score);
    assert_eq!(plain.pages.len(), 1);
    assert_eq!(
        titled.pages.len(),
        1,
        "the title and the system share page 1"
    );
    assert!(!titled.title_page);
    assert!(
        titled.staff_space < plain.staff_space,
        "{:?}",
        titled.staff_space
    );
    let larger = LayoutOptions {
        staff_space: Micrometres(titled.staff_space.micrometres().0 + 1),
        ..LayoutOptions::default()
    };
    let asked = layout(&score, &larger).unwrap();
    assert_eq!(
        asked.staff_space, titled.staff_space,
        "the chosen staff space is the largest that fits"
    );
    assert_ink_inside_content(&titled);
}

#[test]
fn a_title_page_alone_is_reported_only_when_even_the_floor_cannot_hold_both() {
    let mut score = guitar(&quarter_bars(2));
    add_track(&mut score, "Second", 6, InstrumentKind::Fretted);
    score.meta = ScoreMeta {
        title: "word ".repeat(60),
        subtitle: "other ".repeat(40),
        ..full_meta()
    };
    let options = LayoutOptions {
        page: PageSize {
            width: Micrometres::from_mm(120),
            height: Micrometres::from_mm(100),
        },
        ..LayoutOptions::default()
    };
    let list = layout(&score, &options).unwrap();
    check(&score, &options, &list);
    assert!(list.title_page);
    assert!(list.pages[0].systems.is_empty());
    assert!(!list.pages[0].items.is_empty());
    assert_eq!(list.pages.len(), 2);
    assert!(
        list.staff_space.micrometres() > MIN_STAFF_SPACE,
        "systems keep the largest fitting space"
    );
    assert_ink_inside_content(&list);
    score.meta = ScoreMeta::default();
    let untitled = layout(&score, &options).unwrap();
    assert!(!untitled.title_page);
    assert_eq!(untitled.pages.len(), 1);
}

#[test]
fn a_credit_already_starting_with_its_prefix_is_not_prefixed_again() {
    let meta = ScoreMeta {
        words: "Words by A. Writer".to_string(),
        music: "MUSIC BY B. Composer".to_string(),
        tab_author: "Tabbed by C. Tabber".to_string(),
        ..ScoreMeta::default()
    };
    assert_eq!(
        credits(&meta),
        vec!["Words by A. Writer", "MUSIC BY B. Composer"]
    );
    assert_eq!(tab_credit(&meta).unwrap(), "Tab by Tabbed by C. Tabber");
    let shared = ScoreMeta {
        words: "words & music by Someone".to_string(),
        music: "words & music by Someone".to_string(),
        ..ScoreMeta::default()
    };
    assert_eq!(credits(&shared), vec!["words & music by Someone"]);
    let plain = ScoreMeta {
        words: "Wordsworth".to_string(),
        ..ScoreMeta::default()
    };
    assert_eq!(credits(&plain), vec!["Words by Wordsworth"]);
}

fn arb_char() -> impl Strategy<Value = char> {
    prop_oneof![
        8 => proptest::char::range('a', 'z'),
        2 => Just(' '),
        2 => Just('W'),
        1 => Just('\n'),
        1 => Just('\t'),
        1 => Just('\u{301}'),
        1 => Just('\u{200B}'),
        2 => select(vec!['é', 'Ω', 'ж', 'ß', '—', '…', '|', 'Ǻ']),
        2 => select(vec!['日', '語', '한', '🎸', '😀', '\u{E000}', '\u{10FFFF}']),
        2 => any::<char>(),
    ]
}

fn arb_long() -> impl Strategy<Value = String> {
    prop_oneof![
        3 => vec(arb_char(), 0..=40).prop_map(|chars| chars.into_iter().collect()),
        2 => vec(arb_char(), 0..=400).prop_map(|chars| chars.into_iter().collect()),
        1 => (arb_char(), 500_usize..=3000)
            .prop_map(|(character, count)| std::iter::repeat_n(character, count).collect()),
    ]
}

#[derive(Debug, Clone)]
struct LongCase {
    meta: [String; 7],
    names: Vec<String>,
    marks: Vec<(String, String)>,
    chords: Vec<String>,
    texts: Vec<String>,
    tempi: Vec<(u16, u8)>,
    bars: usize,
    mode: Option<StaffMode>,
    small: bool,
}

const UNITS: [TempoUnit; 5] = [
    TempoUnit::Eighth,
    TempoUnit::Quarter,
    TempoUnit::DottedQuarter,
    TempoUnit::Half,
    TempoUnit::DottedHalf,
];

fn arb_long_case() -> impl Strategy<Value = LongCase> {
    (
        [
            arb_long(),
            arb_long(),
            arb_long(),
            arb_long(),
            arb_long(),
            arb_long(),
            arb_long(),
        ],
        vec(arb_long(), 1..=3),
        vec((arb_long(), arb_long()), 1..=6),
        vec(arb_long(), 1..=12),
        vec(arb_long(), 1..=12),
        vec((10_u16..=10_000, 0_u8..5), 1..=6),
        3_usize..=10,
        proptest::option::of(prop_oneof![
            Just(StaffMode::Standard),
            Just(StaffMode::Tab),
            Just(StaffMode::Both),
        ]),
        any::<bool>(),
    )
        .prop_map(
            |(meta, names, marks, chords, texts, tempi, bars, mode, small)| LongCase {
                meta,
                names,
                marks,
                chords,
                texts,
                tempi,
                bars,
                mode,
                small,
            },
        )
}

fn long_score(case: &LongCase) -> Score {
    let mut score = guitar(&quarter_bars(case.bars));
    score.tracks[0].name = case.names[0].clone();
    score.tracks[0].short_name = String::new();
    for name in &case.names[1..] {
        let index = add_track(&mut score, "x", 6, InstrumentKind::Fretted);
        score.tracks[index].name = name.clone();
        score.tracks[index].short_name = String::new();
    }
    let [title, subtitle, artist, album, words, music, tab_author] = case.meta.clone();
    score.meta = ScoreMeta {
        title,
        subtitle,
        artist,
        album,
        words,
        music,
        tab_author,
        ..ScoreMeta::default()
    };
    for (index, (letter, text)) in case.marks.iter().enumerate() {
        score.master_bars[index * 2 % case.bars].marker = Some(Marker {
            text: text.clone(),
            letter: letter.clone(),
        });
    }
    for (index, (tenths, unit)) in case.tempi.iter().enumerate() {
        let bar = index * 3 % case.bars;
        score.master_bars[bar].tempo_changes.push(TempoChange {
            position: Fraction::new(i64::try_from(index % 5).unwrap(), 4),
            bpm: Tempo::new(f64::from(*tenths) / 10.0).unwrap(),
            unit: UNITS[usize::from(*unit)],
        });
    }
    let beats = score.tracks[0].staves[0]
        .bars
        .iter_mut()
        .flat_map(|bar| &mut bar.voices)
        .filter(|voice| voice.slot == VoiceSlot::default())
        .flat_map(|voice| &mut voice.beats);
    for (index, beat) in beats.enumerate() {
        beat.effects.chord_name = Some(case.chords[index % case.chords.len()].clone());
        beat.effects.text = Some(case.texts[index % case.texts.len()].clone());
    }
    score
}

#[derive(Debug, Clone, Copy, Default)]
struct MarkTally {
    cases: usize,
    titles: usize,
    names: usize,
    sections: usize,
    chords: usize,
    texts: usize,
    tempi: usize,
    dropped: usize,
    many_systems: usize,
    many_pages: usize,
}

fn spaced(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn stripped(text: &str) -> &str {
    text.strip_suffix(ELLIPSIS).unwrap_or(text)
}

fn drawn_from(list: &DisplayList, sources: &[String], em: Option<Units>) -> usize {
    let cleaned: Vec<String> = sources
        .iter()
        .map(|source| spaced(&clean(source)))
        .filter(|source| !source.is_empty())
        .collect();
    list.pages
        .iter()
        .flat_map(|page| top_texts(page))
        .filter(|run| em.is_none_or(|wanted| run.em == wanted))
        .filter(|run| {
            let kept = spaced(stripped(&run.text));
            let whole = spaced(&run.text);
            cleaned
                .iter()
                .any(|source| *source == whole || (!kept.is_empty() && source.contains(&kept)))
        })
        .count()
}

fn wanted(sources: &[String]) -> bool {
    sources
        .iter()
        .any(|source| visible_ink(&clean(source), TEXT_EM).is_some())
}

fn dropped_of(list: &DisplayList, kind: MarkKind) -> usize {
    list.dropped.iter().filter(|mark| mark.kind == kind).count()
}

fn tally_case(case: &LongCase, list: &DisplayList, tally: &mut MarkTally) {
    tally.cases += 1;
    let title = [case.meta[0].clone()];
    let titles = drawn_from(list, &title, Some(TITLE_EM));
    assert!(titles > 0 || !wanted(&title), "the title was not drawn");
    tally.titles += titles;
    if case.names.len() > 1 {
        let names = drawn_from(list, &case.names, Some(NAME_EM));
        assert!(names > 0 || !wanted(&case.names), "no track name was drawn");
        tally.names += names;
    }
    let sections: Vec<String> = case
        .marks
        .iter()
        .flat_map(|(letter, text)| [letter.clone(), text.clone()])
        .collect();
    let drawn = drawn_from(list, &sections, Some(SECTION_EM));
    assert!(
        drawn + dropped_of(list, MarkKind::Section) > 0 || !wanted(&sections),
        "no section was drawn or reported"
    );
    tally.sections += drawn;
    let chords = drawn_from(list, &case.chords, Some(CHORD_EM));
    assert!(
        chords + dropped_of(list, MarkKind::Chord) > 0 || !wanted(&case.chords),
        "no chord name was drawn or reported"
    );
    tally.chords += chords;
    let texts = drawn_from(list, &case.texts, Some(TEXT_EM));
    assert!(
        texts + dropped_of(list, MarkKind::Text) > 0 || !wanted(&case.texts),
        "no free text was drawn or reported"
    );
    tally.texts += texts;
    let tempi = list
        .pages
        .iter()
        .flat_map(|page| &page.items)
        .filter(|item| matches!(item, Primitive::Glyph(glyph) if ('\u{ECA0}'..='\u{ECAF}').contains(&glyph.codepoint)))
        .count();
    assert!(
        tempi + dropped_of(list, MarkKind::Tempo) > 0,
        "no tempo was drawn or reported"
    );
    tally.tempi += tempi;
    tally.dropped += list.dropped.len();
    if systems(list).len() > 1 {
        tally.many_systems += 1;
    }
    if list.pages.len() > 1 {
        tally.many_pages += 1;
    }
}

const MIN_CASES: usize = 200;

#[test]
fn every_text_and_mark_stays_inside_the_page_content_whatever_its_length() {
    let mut runner = TestRunner::new(ProptestConfig {
        failure_persistence: None,
        ..ProptestConfig::default()
    });
    let total = RefCell::new(MarkTally::default());
    runner
        .run(&arb_long_case(), |case| {
            let score = long_score(&case);
            let options = LayoutOptions {
                staff_mode: case.mode,
                page: if case.small {
                    PageSize {
                        width: Micrometres::from_mm(150),
                        height: Micrometres::from_mm(220),
                    }
                } else {
                    LayoutOptions::default().page
                },
                ..LayoutOptions::default()
            };
            let list = layout(&score, &options).unwrap();
            check(&score, &options, &list);
            assert_ink_inside_content(&list);
            tally_case(&case, &list, &mut total.borrow_mut());
            Ok(())
        })
        .unwrap();
    let total = total.into_inner();
    assert!(total.cases >= MIN_CASES, "{total:?}");
    assert!(total.titles >= 200, "{total:?}");
    assert!(total.names >= 200, "{total:?}");
    assert!(total.sections >= 700, "{total:?}");
    assert!(total.chords >= 3000, "{total:?}");
    assert!(total.texts >= 1800, "{total:?}");
    assert!(total.tempi >= 400, "{total:?}");
    assert!(total.dropped >= 1000, "{total:?}");
    assert!(total.many_systems >= 200, "{total:?}");
    assert!(total.many_pages >= 100, "{total:?}");
}
