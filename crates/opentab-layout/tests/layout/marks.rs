use std::num::NonZeroI64;

use opentab_layout::marks::{BLOCK_GAP, ELLIPSIS, TITLE_EM, line_height};
use opentab_layout::text::text_font;
use opentab_layout::{
    DisplayList, LayoutOptions, Page, PathSegment, Point, Primitive, REFERENCE_EM, Rect, StaffMode,
    TextAnchor, TextRun, Units, layout,
};
use opentab_model::{Fraction, Marker, Score, ScoreMeta, Tempo, TempoChange, TempoUnit, VoiceSlot};
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::select;

use crate::support::{check, content, dump, flatten, guitar, ink, quarter_bars, systems};

pub fn text_ink(run: &TextRun) -> Option<Rect> {
    let font = text_font();
    let positions = font.pen_positions(&run.text, run.em);
    let width = positions.last().copied().unwrap_or_default();
    let shift = match run.anchor {
        TextAnchor::Start => Units::ZERO,
        TextAnchor::Middle => width.half(),
        TextAnchor::End => width,
    };
    let divisor = NonZeroI64::new(REFERENCE_EM.0).unwrap();
    let scale = |value: Units| value.scaled(run.em.0, divisor);
    let mut found: Option<Rect> = None;
    for (character, pen) in run.text.chars().zip(positions) {
        let Some(bounds) = font.outline(character).bounds() else {
            continue;
        };
        let x = run.origin.x + pen - shift;
        let rect = Rect::new(
            Point {
                x: x + scale(bounds.min.x),
                y: run.origin.y + scale(bounds.min.y),
            },
            Point {
                x: x + scale(bounds.max.x),
                y: run.origin.y + scale(bounds.max.y),
            },
        );
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
    let lowest = top_texts(page)
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
    let texts: Vec<&str> = top_texts(&list.pages[0])
        .into_iter()
        .map(|run| run.text.as_str())
        .collect();
    assert_eq!(texts, vec!["Words & Music by Same Person"]);
    let bare = render(&guitar(&quarter_bars(2)));
    assert!(top_texts(&bare.pages[0]).is_empty());
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
    marks: Vec<(String, String)>,
    chords: Vec<String>,
    texts: Vec<String>,
    mode: Option<StaffMode>,
}

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
        vec((arb_long(), arb_long()), 3),
        vec(arb_long(), 12),
        vec(arb_long(), 12),
        proptest::option::of(prop_oneof![
            Just(StaffMode::Standard),
            Just(StaffMode::Tab),
            Just(StaffMode::Both),
        ]),
    )
        .prop_map(|(meta, marks, chords, texts, mode)| LongCase {
            meta,
            marks,
            chords,
            texts,
            mode,
        })
}

fn long_score(case: &LongCase) -> Score {
    let mut score = guitar(&quarter_bars(3));
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
        score.master_bars[index].marker = Some(Marker {
            text: text.clone(),
            letter: letter.clone(),
        });
        score.master_bars[index].tempo_changes = vec![TempoChange {
            position: Fraction::new(i64::try_from(index).unwrap(), 4),
            bpm: Tempo::new(97.5).unwrap(),
            unit: TempoUnit::DottedHalf,
        }];
    }
    let beats = score.tracks[0].staves[0]
        .bars
        .iter_mut()
        .flat_map(|bar| &mut bar.voices)
        .filter(|voice| voice.slot == VoiceSlot::default())
        .flat_map(|voice| &mut voice.beats);
    for (beat, (chord, text)) in beats.zip(case.chords.iter().zip(&case.texts)) {
        beat.effects.chord_name = Some(chord.clone());
        beat.effects.text = Some(text.clone());
    }
    score
}

proptest! {
    #[test]
    fn every_text_and_mark_stays_inside_the_page_content_whatever_its_length(case in arb_long_case()) {
        let score = long_score(&case);
        let options = LayoutOptions {
            staff_mode: case.mode,
            ..LayoutOptions::default()
        };
        let list = layout(&score, &options).unwrap();
        check(&score, &options, &list);
        prop_assert!(assert_ink_inside_content(&list) > 0);
    }
}
