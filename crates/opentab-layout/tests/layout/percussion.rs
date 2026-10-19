use opentab_layout::{
    BarLayout, ClefSign, DEFAULT_DRUM, DRUM_TABLE, DisplayList, DrumHead, DrumStaffKit, GroupId,
    LayoutOptions, StaffKind, StaffMode, Unrendered, drum_entry,
};
use opentab_model::{
    DrumArticulation, DrumElement, DrumKit, InstrumentKind, KeySignature, NoteKind, NoteValue,
    Score, TechniquePlacement, Tuning, gm_drum_kit,
};

use crate::music::{
    Event, Row, beat_id, build, glyphs, group_in, half_spaces, head_of, is_accidental, is_notehead,
    measure, note_id, options, play, points_up, render, standard_rows, stem_of,
};
use crate::support::{glyph_name, systems};

fn articulation(key: u8) -> DrumArticulation {
    DrumArticulation {
        name: format!("key {key}"),
        staff_line: 0,
        noteheads: ["noteheadBlack", "noteheadHalf", "noteheadWhole"].map(str::to_string),
        technique_placement: TechniquePlacement::Outside,
        technique_symbol: String::new(),
        input_keys: vec![key],
        rse_sound: String::new(),
        output_key: key,
    }
}

pub fn kit_of(keys: &[u8]) -> DrumKit {
    let element = DrumElement {
        name: "All".to_string(),
        kind: "unknown".to_string(),
        soundbank: String::new(),
        count: keys.len(),
    };
    DrumKit::new(
        vec![element],
        keys.iter().copied().map(articulation).collect(),
    )
    .unwrap()
}

pub fn general_kit() -> DrumKit {
    let keys: Vec<u8> = (0..=127).collect();
    kit_of(&keys)
}

pub fn drums(chords: &[Vec<u16>], voices: usize, kit: bool) -> Score {
    let events = |count: usize, notes: usize| -> Vec<Event> {
        let frets: Vec<(u8, u8)> = (0..notes)
            .map(|string| (u8::try_from(string).unwrap(), 0))
            .collect();
        (0..count)
            .map(|_| play(NoteValue::Eighth, &frets))
            .collect()
    };
    let width = chords.iter().map(Vec::len).max().unwrap_or(1);
    let bars = chords.len().div_ceil(8);
    let measures: Vec<_> = (0..bars)
        .map(|_| {
            let mut parts = vec![(0, events(8, width))];
            if voices > 1 {
                parts.push((1, events(8, width)));
            }
            measure((4, 4), 3, parts)
        })
        .collect();
    let mut score = build(&measures);
    let track = &mut score.tracks[0];
    track.instrument.kind = InstrumentKind::Percussion;
    track.tuning = Tuning {
        strings: Vec::new(),
    };
    let mut index = 0;
    for bar in &mut track.staves[0].bars {
        for voice in &mut bar.voices {
            for beat in &mut voice.beats {
                let chord = &chords[index % chords.len()];
                beat.notes.truncate(chord.len());
                for (note, articulation) in beat.notes.iter_mut().zip(chord) {
                    note.kind = NoteKind::Percussion {
                        articulation: *articulation,
                    };
                }
                index += 1;
            }
        }
    }
    for master in &mut score.master_bars {
        master.key_signature = KeySignature::new(3, false).unwrap();
    }
    if kit {
        score.tracks[0].drum_kit = Some(general_kit());
    }
    score
}

fn drum_score(keys: &[u16], voices: usize) -> Score {
    let chords: Vec<Vec<u16>> = keys.iter().map(|key| vec![*key]).collect();
    drums(&chords, voices, true)
}

fn head_position(score: &Score, list: &DisplayList, bar: usize, beat: usize) -> i64 {
    let staff = systems(list)
        .into_iter()
        .flat_map(|system| &system.staves)
        .find(|staff| staff.bars.iter().any(|found| found.index == bar))
        .unwrap();
    half_spaces(
        head_of(list, note_id(score, bar, 0, beat, 0)).origin.y,
        staff.top,
    )
}

#[test]
fn every_drum_class_sits_on_its_line_with_its_head_and_stems_up() {
    let mut keys: Vec<u16> = DRUM_TABLE
        .iter()
        .map(|entry| u16::from(entry.keys[0]))
        .collect();
    keys.extend([81, 127, 300, u16::MAX]);
    let score = drum_score(&keys, 1);
    for mode in [None, Some(StaffMode::Tab), Some(StaffMode::Both)] {
        let options = LayoutOptions {
            staff_mode: mode,
            ..LayoutOptions::default()
        };
        let list = render(&score, &options);
        for system in systems(&list) {
            assert!(
                system
                    .staves
                    .iter()
                    .all(|staff| staff.kind == StaffKind::Standard && staff.lines == 5)
            );
            assert!(
                system.staves[0]
                    .bars
                    .iter()
                    .all(|bar| bar.clef == ClefSign::Neutral)
            );
        }
    }
    let list = render(&score, &options(StaffMode::Both));
    let all = list
        .pages
        .iter()
        .flat_map(|page| glyphs(&page.items))
        .collect::<Vec<_>>();
    let names: Vec<&str> = all
        .iter()
        .map(|glyph| glyph_name(glyph.codepoint))
        .collect();
    assert!(names.contains(&"unpitchedPercussionClef1"), "{names:?}");
    assert!(
        !names.iter().any(|name| name.starts_with("accidental")),
        "no key signature or accidentals"
    );
    assert!(!all.iter().any(is_accidental));
    for (index, key) in keys.iter().enumerate() {
        let (bar, beat) = (index / 8, index % 8);
        let expected = drum_entry(u8::try_from(*key).ok().filter(|key| *key <= 127));
        assert_eq!(
            head_position(&score, &list, bar, beat),
            expected.position,
            "key {key}"
        );
        let head = glyph_name(head_of(&list, note_id(&score, bar, 0, beat, 0)).codepoint);
        let cross = expected.head == DrumHead::Cross;
        assert_eq!(head == "noteheadXBlack", cross, "key {key}: {head}");
        assert!(cross || head == "noteheadBlack", "key {key}: {head}");
        assert!(
            points_up(&stem_of(&list, beat_id(&score, bar, 0, beat))),
            "key {key}"
        );
    }
}

#[test]
fn two_drum_voices_split_their_stems_and_unknown_kits_do_not_panic() {
    let score = drum_score(&[42, 36, 38, 999], 2);
    let list = render(&score, &options(StaffMode::Both));
    for beat in 0..8 {
        assert!(points_up(&stem_of(&list, beat_id(&score, 0, 0, beat))));
        assert!(!points_up(&stem_of(&list, beat_id(&score, 0, 1, beat))));
    }
}

#[test]
fn the_typed_kit_maps_articulation_indices_to_drum_classes() {
    let mut score = drum_score(&[0, 1, 2, 3], 1);
    score.tracks[0].drum_kit = Some(kit_of(&[36, 49, 38]));
    let list = render(&score, &options(StaffMode::Both));
    let positions: Vec<i64> = (0..4)
        .map(|beat| head_position(&score, &list, 0, beat))
        .collect();
    assert_eq!(positions, vec![7, -2, 3, 4]);
    let heads: Vec<&str> = (0..4)
        .map(|beat| glyph_name(head_of(&list, note_id(&score, 0, 0, beat, 0)).codepoint))
        .collect();
    assert_eq!(
        heads,
        vec![
            "noteheadBlack",
            "noteheadXBlack",
            "noteheadBlack",
            "noteheadBlack"
        ]
    );
}

pub fn drum_heads(score: &Score, list: &DisplayList) -> usize {
    standard_rows(list)
        .iter()
        .map(|(_, _, row)| {
            row.staff
                .bars
                .iter()
                .map(|bar| check_drum_positions(score, list, row, bar))
                .sum::<usize>()
        })
        .sum()
}

pub fn check_drum_positions(
    score: &Score,
    list: &DisplayList,
    row: &Row<'_>,
    bar: &BarLayout,
) -> usize {
    let track = &score.tracks[row.staff.track];
    if track.instrument.kind != InstrumentKind::Percussion {
        return 0;
    }
    let Some(kit) = track.kit().map(|kit| DrumStaffKit::of(&kit)) else {
        return 0;
    };
    let Some(model) = track.staves[row.staff.staff].bars.get(bar.index) else {
        return 0;
    };
    let mut checked = 0;
    for note in model
        .voices
        .iter()
        .flat_map(|voice| &voice.beats)
        .flat_map(|beat| &beat.notes)
    {
        let NoteKind::Percussion { articulation } = note.kind else {
            continue;
        };
        let head = glyphs(&group_in(list, GroupId(note.id.0.0)).items)
            .into_iter()
            .find(is_notehead)
            .unwrap();
        assert_eq!(
            half_spaces(head.origin.y, row.staff.top),
            kit.entry(articulation).position,
            "drum note {} with articulation {articulation}",
            note.id.0.0
        );
        checked += 1;
    }
    checked
}

#[test]
fn drum_unisons_on_one_line_show_both_heads_side_by_side() {
    let score = drums(
        &[vec![42, 46], vec![38, 37], vec![51, 53], vec![36]],
        1,
        true,
    );
    let list = render(&score, &options(StaffMode::Both));
    for (beat, line, names) in [
        (0, -1, ["noteheadXBlack", "noteheadXBlack"]),
        (1, 3, ["noteheadBlack", "noteheadXBlack"]),
        (2, 0, ["noteheadXBlack", "noteheadXBlack"]),
    ] {
        let first = head_of(&list, note_id(&score, 0, 0, beat, 0));
        let second = head_of(&list, note_id(&score, 0, 0, beat, 1));
        assert_eq!(head_position(&score, &list, 0, beat), line);
        assert_eq!(first.origin.y, second.origin.y, "beat {beat}");
        assert_ne!(
            first.origin.x, second.origin.x,
            "beat {beat} heads are displaced"
        );
        let mut found = [glyph_name(first.codepoint), glyph_name(second.codepoint)];
        found.sort_unstable();
        let mut wanted = names;
        wanted.sort_unstable();
        assert_eq!(found, wanted, "beat {beat}");
    }
}

#[test]
fn without_a_kit_the_general_midi_kit_places_the_notes() {
    let score = drums(&[vec![3], vec![7], vec![1]], 1, false);
    assert_eq!(score.tracks[0].drum_kit, None);
    let list = render(&score, &options(StaffMode::Both));
    let positions: Vec<i64> = (0..3)
        .map(|beat| head_position(&score, &list, 0, beat))
        .collect();
    assert_eq!(positions, vec![3, -1, 7]);
    assert!(list.unrendered.is_empty());
    let beyond = drums(&[vec![47], vec![u16::MAX]], 1, false);
    let list = render(&beyond, &options(StaffMode::Both));
    for beat in 0..2 {
        assert_eq!(
            head_position(&beyond, &list, 0, beat),
            DEFAULT_DRUM.position
        );
    }
    assert!(
        list.unrendered
            .contains(&Unrendered::PercussionArticulations)
    );
}

#[test]
fn the_general_midi_kit_takes_its_lines_and_heads_from_the_drum_table() {
    let heads = |head: DrumHead| match head {
        DrumHead::Normal => ["noteheadBlack", "noteheadHalf", "noteheadWhole"],
        DrumHead::Cross => ["noteheadXBlack", "noteheadXHalf", "noteheadXWhole"],
    };
    let kit = gm_drum_kit();
    for articulation in kit.articulations() {
        let entry = drum_entry(Some(articulation.output_key));
        assert_eq!(
            i64::from(articulation.staff_line),
            entry.position,
            "{}",
            articulation.name
        );
        assert_eq!(
            articulation.noteheads.each_ref().map(String::as_str),
            heads(entry.head),
            "{}",
            articulation.name
        );
    }
    let classes: Vec<_> = DRUM_TABLE
        .iter()
        .flat_map(|entry| entry.keys.iter())
        .map(|key| {
            kit.articulations()
                .iter()
                .any(|found| found.output_key == *key)
        })
        .collect();
    assert!(classes.iter().all(|found| *found));
}
