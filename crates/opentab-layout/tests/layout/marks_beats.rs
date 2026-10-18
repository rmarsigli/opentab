use opentab_layout::{
    BarLayout, DisplayList, Group, PathStyle, PlacedGlyph, Primitive, Rect, StaffKind, StaffMode,
    SystemLayout, TextRun, Units,
};
use opentab_model::{Dynamic, Fraction, Marker, NoteValue, Score, Tempo, TempoChange, TempoUnit};

use crate::marks::{assert_ink_inside_content, shape_box, text_ink};
use crate::music::{build, measure, note_at, options, play, render, rows};
use crate::support::{content, dump, flatten, ink, notehead, systems};
use crate::tab_effects::quarters;

const MET_NOTE_8TH_UP: char = '\u{ECA7}';
const MET_NOTE_QUARTER_UP: char = '\u{ECA5}';
const MET_NOTE_HALF_UP: char = '\u{ECA3}';
const MET_AUGMENTATION_DOT: char = '\u{ECB7}';
const DYNAMIC_PIANO: char = '\u{E520}';
const DYNAMIC_FORTE: char = '\u{E522}';
const DYNAMIC_MF: char = '\u{E52D}';

fn quarter_score(bars: usize) -> Score {
    let measures: Vec<_> = (0..bars)
        .map(|_| measure((4, 4), 0, vec![(0, quarters(&[(2, 5); 4]))]))
        .collect();
    build(&measures)
}

fn tempo(numer: i64, denom: i64, bpm: f64, unit: TempoUnit) -> TempoChange {
    TempoChange {
        position: Fraction::new(numer, denom),
        bpm: Tempo::new(bpm).unwrap(),
        unit,
    }
}

fn top_glyphs(list: &DisplayList) -> Vec<PlacedGlyph> {
    list.pages
        .iter()
        .flat_map(|page| &page.items)
        .filter_map(|item| match item {
            Primitive::Glyph(glyph) => Some(*glyph),
            _ => None,
        })
        .collect()
}

fn top_texts(list: &DisplayList) -> Vec<TextRun> {
    list.pages
        .iter()
        .flat_map(|page| &page.items)
        .filter_map(|item| match item {
            Primitive::Text(run) => Some(run.clone()),
            _ => None,
        })
        .collect()
}

fn texts_named(list: &DisplayList, text: &str) -> Vec<TextRun> {
    top_texts(list)
        .into_iter()
        .filter(|run| run.text == text)
        .collect()
}

fn bar_at(list: &DisplayList, row: usize, index: usize) -> (&SystemLayout, &BarLayout) {
    systems(list)
        .into_iter()
        .find_map(|system| {
            system.staves[row]
                .bars
                .iter()
                .find(|bar| bar.index == index)
                .map(|bar| (system, bar))
        })
        .expect("the bar is drawn")
}

fn is_metronome(glyph: &PlacedGlyph) -> bool {
    ('\u{ECA0}'..='\u{ECBF}').contains(&glyph.codepoint)
}

fn tempo_glyphs_in(list: &DisplayList, bar: &BarLayout) -> Vec<char> {
    let mut found: Vec<PlacedGlyph> = top_glyphs(list)
        .into_iter()
        .filter(is_metronome)
        .filter(|glyph| bar.rect.min.x <= glyph.origin.x && glyph.origin.x < bar.rect.max.x)
        .collect();
    found.sort_by_key(|glyph| glyph.origin.x);
    found.into_iter().map(|glyph| glyph.codepoint).collect()
}

#[test]
fn every_tempo_unit_draws_its_metronome_glyph_then_the_bpm() {
    let mut score = quarter_score(5);
    let units = [
        TempoUnit::Eighth,
        TempoUnit::Quarter,
        TempoUnit::DottedQuarter,
        TempoUnit::Half,
        TempoUnit::DottedHalf,
    ];
    for (index, unit) in units.iter().enumerate() {
        let bpm = f64::from(u8::try_from(60 + index * 10).unwrap());
        score.master_bars[index].tempo_changes = vec![tempo(0, 1, bpm, *unit)];
    }
    let list = render(&score, &options(StaffMode::Both));
    let expected = [
        vec![MET_NOTE_8TH_UP],
        vec![MET_NOTE_QUARTER_UP],
        vec![MET_NOTE_QUARTER_UP, MET_AUGMENTATION_DOT],
        vec![MET_NOTE_HALF_UP],
        vec![MET_NOTE_HALF_UP, MET_AUGMENTATION_DOT],
    ];
    for (index, codepoints) in expected.iter().enumerate() {
        let (system, bar) = bar_at(&list, 0, index);
        assert_eq!(&tempo_glyphs_in(&list, bar), codepoints, "bar {index}");
        let note = top_glyphs(&list)
            .into_iter()
            .find(|glyph| glyph.codepoint == codepoints[0] && bar.rect.min.x <= glyph.origin.x)
            .unwrap();
        let note_ink = ink(&note);
        assert_eq!(note_ink.min.x, bar.columns[0].x, "at the first onset");
        assert!(note_ink.max.y < system.staves[0].top, "above the top staff");
        let label = format!("= {}", 60 + index * 10);
        let run = texts_named(&list, &label).pop().expect("bpm text");
        let text = text_ink(&run).unwrap();
        assert!(text.min.x > note_ink.max.x, "{label} after the glyph");
        assert!(text.max.y >= note_ink.max.y - Units(300) && text.min.y < note_ink.max.y);
    }
    insta::assert_snapshot!(dump(&list));
}

#[test]
fn tempo_changes_at_the_bar_end_mid_bar_and_carried_over_land_on_onsets() {
    let mut score = quarter_score(4);
    score.master_bars[0].tempo_changes = vec![
        tempo(0, 1, 120.0, TempoUnit::Quarter),
        tempo(1, 1, 150.0, TempoUnit::Quarter),
    ];
    score.master_bars[1].tempo_changes = vec![tempo(1, 2, 90.0, TempoUnit::Eighth)];
    score.master_bars[3].tempo_changes = vec![tempo(1, 1, 70.0, TempoUnit::Half)];
    let list = render(&score, &options(StaffMode::Standard));
    let x_of = |label: &str| {
        let run = texts_named(&list, label)
            .pop()
            .unwrap_or_else(|| panic!("{label}"));
        text_ink(&run).unwrap()
    };
    let (_, first) = bar_at(&list, 0, 0);
    let (_, second) = bar_at(&list, 0, 1);
    let (_, last) = bar_at(&list, 0, 3);
    assert!(x_of("= 120").min.x < first.rect.max.x);
    let carried = x_of("= 150");
    assert!(second.content <= carried.min.x && carried.min.x < second.columns[1].x);
    assert!(x_of("= 90").min.x > second.columns[2].x);
    assert!(x_of("= 90").min.x < second.columns[3].x);
    let end = x_of("= 70");
    assert!(end.min.x > last.columns[3].x - Units(4096));
    assert!(end.max.x <= content(&list.pages[0]).max.x);
    let half_notes: Vec<PlacedGlyph> = top_glyphs(&list)
        .into_iter()
        .filter(|glyph| glyph.codepoint == MET_NOTE_HALF_UP)
        .collect();
    assert_eq!(half_notes.len(), 1);
    let mut overruled = score.clone();
    overruled.master_bars[1].tempo_changes = vec![tempo(0, 1, 100.0, TempoUnit::Quarter)];
    let list = render(&overruled, &options(StaffMode::Standard));
    assert!(
        texts_named(&list, "= 150").is_empty(),
        "the bar's own tempo wins"
    );
    assert_eq!(texts_named(&list, "= 100").len(), 1);
    assert_ink_inside_content(&list);
}

fn boxes(list: &DisplayList) -> Vec<Rect> {
    list.pages
        .iter()
        .flat_map(|page| &page.items)
        .filter_map(|item| match item {
            Primitive::Shape(shape) if matches!(shape.style, PathStyle::Stroke(_)) => {
                shape_box(&shape.segments)
            }
            _ => None,
        })
        .collect()
}

fn encloses(outer: Rect, inner: Rect) -> bool {
    outer.min.x <= inner.min.x
        && outer.min.y <= inner.min.y
        && inner.max.x <= outer.max.x
        && inner.max.y <= outer.max.y
}

#[test]
fn section_markers_box_their_letter_beside_the_text_at_the_bar_start() {
    let mut score = quarter_score(4);
    let marks = [("A", "Intro"), ("", "Bridge"), ("C", "")];
    for (index, (letter, text)) in marks.iter().enumerate() {
        score.master_bars[index].marker = Some(Marker {
            text: (*text).to_string(),
            letter: (*letter).to_string(),
        });
    }
    let list = render(&score, &options(StaffMode::Both));
    let found = boxes(&list);
    assert_eq!(found.len(), 2, "only lettered markers get a box");
    for (letter, rect) in ["A", "C"].iter().zip(&found) {
        let run = texts_named(&list, letter).pop().unwrap();
        assert!(encloses(*rect, text_ink(&run).unwrap()), "{letter} boxed");
    }
    let (system, first) = bar_at(&list, 0, 0);
    assert_eq!(found[0].min.x - first.rect.min.x, Units(64));
    let intro = text_ink(&texts_named(&list, "Intro").pop().unwrap()).unwrap();
    assert!(intro.min.x > found[0].max.x);
    let bridge = text_ink(&texts_named(&list, "Bridge").pop().unwrap()).unwrap();
    let (_, second) = bar_at(&list, 0, 1);
    assert_eq!(bridge.min.x, second.rect.min.x);
    for rect in [found[0], found[1], intro, bridge] {
        assert!(rect.max.y < system.staves[0].top);
    }
    insta::assert_snapshot!(dump(&list));
}

fn sixteenths_with_chords(name: &str) -> Score {
    let mut score = build(&[measure(
        (4, 4),
        0,
        vec![(0, vec![play(NoteValue::Sixteenth, &[(2, 5)]); 16])],
    )]);
    for beat in &mut score.tracks[0].staves[0].bars[0].voices[0].beats {
        beat.effects.chord_name = Some(name.to_string());
    }
    score
}

#[test]
fn chord_names_sit_above_the_staff_at_their_onsets_and_never_overlap() {
    let plain = build(&[measure(
        (4, 4),
        0,
        vec![(0, vec![play(NoteValue::Sixteenth, &[(2, 5)]); 16])],
    )]);
    let named = sixteenths_with_chords("Cmaj7");
    let list = render(&named, &options(StaffMode::Both));
    let names: Vec<Rect> = texts_named(&list, "Cmaj7")
        .iter()
        .map(|run| text_ink(run).unwrap())
        .collect();
    assert_eq!(names.len(), 16);
    let (system, bar) = bar_at(&list, 0, 0);
    for (rect, column) in names.iter().zip(&bar.columns) {
        assert_eq!(rect.min.x, column.x);
        assert!(rect.max.y < system.staves[0].top);
    }
    for pair in names.windows(2) {
        assert!(pair[0].max.x < pair[1].min.x, "names overlap");
    }
    let before = render(&plain, &options(StaffMode::Both));
    let (_, narrow) = bar_at(&before, 0, 0);
    assert!(bar.columns[15].x - bar.columns[0].x > narrow.columns[15].x - narrow.columns[0].x);
    insta::assert_snapshot!(dump(&list));
}

fn standard_bar_group(list: &DisplayList) -> &Group {
    let (_, rows) = rows(&list.pages[0]).into_iter().next().unwrap();
    let row = rows
        .into_iter()
        .find(|row| row.staff.kind == StaffKind::Standard)
        .unwrap();
    row.bars[0]
}

#[test]
fn a_chord_name_clears_notes_with_many_ledger_lines() {
    let mut score = build(&[measure(
        (4, 4),
        0,
        vec![(0, quarters(&[(0, 24), (0, 22), (0, 24), (0, 20)]))],
    )]);
    for beat in &mut score.tracks[0].staves[0].bars[0].voices[0].beats {
        beat.effects.chord_name = Some("E".to_string());
        beat.effects.text = Some("high".to_string());
    }
    let list = render(&score, &options(StaffMode::Both));
    let group = standard_bar_group(&list);
    let mut highest = Units(i64::MAX);
    let mut ledgers = 0;
    for item in flatten(&group.items) {
        match item {
            Primitive::Glyph(glyph) => highest = highest.min(ink(glyph).min.y),
            Primitive::Line(line) => {
                if line.from.y == line.to.y {
                    ledgers += 1;
                }
                highest = highest.min(line.from.y.min(line.to.y));
            }
            _ => {}
        }
    }
    let (system, _) = bar_at(&list, 0, 0);
    assert!(ledgers >= 4 * 4, "{ledgers} ledger lines");
    assert!(highest < system.staves[0].top - Units(4096));
    for label in ["E", "high"] {
        let lowest = texts_named(&list, label)
            .iter()
            .map(|run| text_ink(run).unwrap().max.y)
            .max()
            .unwrap();
        assert!(
            lowest < highest,
            "{label} at {lowest:?} inside notation at {highest:?}"
        );
    }
    let chord = text_ink(&texts_named(&list, "E")[0]).unwrap();
    let text = text_ink(&texts_named(&list, "high")[0]).unwrap();
    assert!(chord.max.y < text.min.y, "chord names above free text");
}

#[test]
fn free_text_starts_at_its_beat_and_a_long_one_is_cut_within_the_margins() {
    let mut score = quarter_score(2);
    let long = "x".repeat(2000);
    {
        let beats = &mut score.tracks[0].staves[0].bars[0].voices[0].beats;
        beats[0].effects.text = Some("Hello".to_string());
        beats[1].effects.text = Some(long.clone());
    }
    let list = render(&score, &options(StaffMode::Standard));
    let hello = text_ink(&texts_named(&list, "Hello").pop().unwrap()).unwrap();
    let (system, bar) = bar_at(&list, 0, 0);
    assert_eq!(hello.min.x, bar.columns[0].x);
    assert!(hello.max.y < system.staves[0].top);
    let cut: Vec<TextRun> = top_texts(&list)
        .into_iter()
        .filter(|run| run.text.starts_with("xx"))
        .collect();
    assert_eq!(cut.len(), 1);
    assert!(cut[0].text.ends_with('\u{2026}'));
    let hello_run = texts_named(&list, "Hello").pop().unwrap();
    assert_eq!(cut[0].origin.y, hello_run.origin.y, "one baseline per lane");
    let cut = text_ink(&cut[0]).unwrap();
    assert!(cut.min.x > hello.max.x);
    assert_eq!(cut.min.x, bar.columns[1].x, "a cut text keeps its beat");
    let slack = content(&list.pages[0]).max.x - cut.max.x;
    assert!(
        slack >= Units::ZERO && slack < Units(2048),
        "cut at the margin"
    );
    assert_ink_inside_content(&list);
    insta::assert_snapshot!(dump(&list));
}

fn dynamics_score() -> Score {
    let mut score = quarter_score(2);
    let levels = [
        [Dynamic::P, Dynamic::P, Dynamic::F, Dynamic::F],
        [Dynamic::F, Dynamic::Mf, Dynamic::Mf, Dynamic::Mf],
    ];
    for (bar, row) in levels.iter().enumerate() {
        for (beat, dynamic) in row.iter().enumerate() {
            score.tracks[0].staves[0].bars[bar].voices[0].beats[beat]
                .effects
                .dynamic = *dynamic;
        }
    }
    score
}

fn dynamic_glyphs(list: &DisplayList) -> Vec<PlacedGlyph> {
    top_glyphs(list)
        .into_iter()
        .filter(|glyph| ('\u{E520}'..='\u{E54F}').contains(&glyph.codepoint))
        .collect()
}

#[test]
fn dynamics_are_drawn_below_the_staff_only_where_they_change() {
    let score = dynamics_score();
    let list = render(&score, &options(StaffMode::Both));
    let found = dynamic_glyphs(&list);
    let codepoints: Vec<char> = found.iter().map(|glyph| glyph.codepoint).collect();
    assert_eq!(codepoints, vec![DYNAMIC_PIANO, DYNAMIC_FORTE, DYNAMIC_MF]);
    let (system, first) = bar_at(&list, 0, 0);
    let (_, second) = bar_at(&list, 0, 1);
    let at = [first.columns[0].x, first.columns[2].x, second.columns[1].x];
    let standard = &system.staves[0];
    let tab = &system.staves[1];
    for (glyph, x) in found.iter().zip(at) {
        let rect = ink(glyph);
        let centre = rect.min.x + rect.width().half();
        let head = x + notehead().half();
        assert!((centre - head).0.abs() <= 1, "centred under the notehead");
        assert!(rect.min.y > standard.bottom(), "below the standard staff");
        assert!(rect.max.y < tab.top, "above the tab");
    }
    let baselines: Vec<Units> = found.iter().map(|glyph| glyph.origin.y).collect();
    assert!(baselines.windows(2).all(|pair| pair[0] == pair[1]));
    insta::assert_snapshot!(dump(&list));
    let alone = render(&score, &options(StaffMode::Tab));
    let (system, _) = bar_at(&alone, 0, 0);
    for glyph in dynamic_glyphs(&alone) {
        assert!(
            ink(&glyph).min.y > system.staves[0].bottom(),
            "below the tab"
        );
    }
    assert_eq!(dynamic_glyphs(&alone).len(), 3);
    let quiet = render(&quarter_score(2), &options(StaffMode::Both));
    assert!(
        dynamic_glyphs(&quiet).is_empty(),
        "the default level is not drawn"
    );
}

#[test]
fn marks_above_the_tab_stack_tempo_section_chord_text_then_effect_lanes() {
    let mut score = quarter_score(2);
    score.master_bars[0].tempo_changes = vec![tempo(0, 1, 120.0, TempoUnit::Quarter)];
    score.master_bars[0].marker = Some(Marker {
        text: "Verse".to_string(),
        letter: "A".to_string(),
    });
    {
        let beat = &mut score.tracks[0].staves[0].bars[0].voices[0].beats[0];
        beat.effects.chord_name = Some("Am".to_string());
        beat.effects.text = Some("softly".to_string());
    }
    note_at(&mut score, 0, 0, 0, 0).effects.let_ring = true;
    let list = render(&score, &options(StaffMode::Tab));
    let baseline = |label: &str| {
        flatten(&list.pages[0].items)
            .into_iter()
            .find_map(|item| match item {
                Primitive::Text(run) if run.text == label => Some(run.origin.y),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{label}"))
    };
    let order = [
        baseline("= 120"),
        baseline("Verse"),
        baseline("Am"),
        baseline("softly"),
        baseline("let ring"),
    ];
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");
    let (system, _) = bar_at(&list, 0, 0);
    assert!(order[4] < system.staves[0].top);
    assert!(system.rect.min.y < order[0] - Units(1024));
    assert_ink_inside_content(&list);
}

#[test]
fn stacked_marks_open_their_system_and_push_the_next_one_down() {
    let plain = quarter_score(16);
    let mut named = plain.clone();
    {
        let beat = &mut named.tracks[0].staves[0].bars[0].voices[0].beats[0];
        beat.effects.chord_name = Some("G".to_string());
        beat.effects.text = Some("loud".to_string());
    }
    named.master_bars[0].marker = Some(Marker {
        text: "Intro".to_string(),
        letter: "A".to_string(),
    });
    let before = render(&plain, &options(StaffMode::Both));
    let after = render(&named, &options(StaffMode::Both));
    let (old, new) = (systems(&before), systems(&after));
    assert!(old.len() >= 2 && old.len() == new.len());
    let grown = new[0].rect.height() - old[0].rect.height();
    assert!(grown > Units::ZERO);
    assert_eq!(new[1].rect.min.y - old[1].rect.min.y, grown);
    assert_eq!(new[1].rect.height(), old[1].rect.height());
    let chord = text_ink(&texts_named(&after, "G")[0]).unwrap();
    assert!(chord.min.y >= new[0].rect.min.y);
}

fn lowest_ink(group: &Group) -> Units {
    flatten(&group.items)
        .into_iter()
        .filter_map(|item| match item {
            Primitive::Glyph(glyph) => Some(ink(glyph).max.y),
            Primitive::Line(line) => Some(line.from.y.max(line.to.y) + line.thickness.half()),
            Primitive::Shape(shape) => shape_box(&shape.segments).map(|rect| rect.max.y),
            Primitive::Text(run) => text_ink(run).map(|rect| rect.max.y),
            Primitive::FilledRect(rect) => Some(rect.rect.max.y),
            Primitive::Group(_) => None,
        })
        .max()
        .unwrap()
}

fn low_two_voice_score() -> Score {
    let mut score = build(&[measure(
        (4, 4),
        0,
        vec![
            (0, quarters(&[(5, 0), (5, 1), (4, 0), (5, 3)])),
            (1, quarters(&[(5, 0), (5, 0), (5, 1), (5, 0)])),
        ],
    )]);
    let levels = [Dynamic::P, Dynamic::F, Dynamic::Pp, Dynamic::Ff];
    for voice in &mut score.tracks[0].staves[0].bars[0].voices {
        for (beat, dynamic) in voice.beats.iter_mut().zip(levels) {
            beat.effects.dynamic = dynamic;
        }
    }
    score
}

#[test]
fn dynamics_clear_low_notes_ledger_lines_and_down_stems() {
    let score = low_two_voice_score();
    let list = render(&score, &options(StaffMode::Both));
    let group = standard_bar_group(&list);
    let (system, _) = bar_at(&list, 0, 0);
    let ledgers = flatten(&group.items)
        .into_iter()
        .filter(|item| {
            matches!(item, Primitive::Line(line) if line.from.y == line.to.y && line.from.y > system.staves[0].bottom())
        })
        .count();
    assert!(ledgers >= 4, "{ledgers} ledger lines below the staff");
    let notation = lowest_ink(group);
    assert!(
        notation > system.staves[0].bottom() + Units(4096),
        "stems reach down"
    );
    let found = dynamic_glyphs(&list);
    assert_eq!(found.len(), 4);
    for glyph in &found {
        let rect = ink(glyph);
        assert!(
            rect.min.y > notation,
            "{rect:?} inside notation reaching {notation:?}"
        );
        assert!(rect.max.y < system.staves[1].top, "above the tab");
    }
}

#[test]
fn dynamics_under_a_lone_tab_clear_its_rhythm() {
    let score = low_two_voice_score();
    let list = render(&score, &options(StaffMode::Tab));
    let (_, rows) = rows(&list.pages[0]).into_iter().next().unwrap();
    let rhythm = lowest_ink(rows[0].bars[0]);
    assert!(
        rhythm > rows[0].staff.bottom() + Units(2048),
        "rhythm hangs under the tab"
    );
    let found = dynamic_glyphs(&list);
    assert_eq!(found.len(), 4);
    for glyph in &found {
        assert!(ink(glyph).min.y > rhythm, "dynamics clear the rhythm");
    }
}
