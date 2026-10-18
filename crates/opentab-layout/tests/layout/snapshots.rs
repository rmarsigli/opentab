use std::path::Path;

use opentab_layout::{LayoutOptions, layout};
use opentab_model::{Clef, InstrumentKind, KeySignature, NoteValue, Score};

use crate::support::{BarSpec, Item, add_track, check, dump, guitar, note};

fn fixture(name: &str) -> Score {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/gp")
        .join(name);
    opentab_gp::read_gp(&std::fs::read(path).unwrap()).unwrap()
}

fn dumped(score: &Score) -> String {
    let options = LayoutOptions::default();
    let list = layout(score, &options).unwrap();
    check(score, &options, &list);
    dump(&list)
}

pub fn furniture_score() -> Score {
    let mut specs: Vec<BarSpec> = Vec::new();
    specs.push(BarSpec {
        time: (4, 4),
        voices: vec![vec![note(NoteValue::Quarter)]],
    });
    for index in 0..22 {
        let voices = match index % 4 {
            0 => vec![vec![note(NoteValue::Quarter); 4]],
            1 => vec![
                vec![
                    Item::Grace(NoteValue::Sixteenth),
                    note(NoteValue::Half),
                    note(NoteValue::Half),
                ],
                vec![Item::Note(NoteValue::Eighth, 0, Some((3, 2))); 12],
            ],
            2 => Vec::new(),
            _ => vec![vec![
                Item::Note(NoteValue::Quarter, 1, None),
                note(NoteValue::Eighth),
                note(NoteValue::Half),
            ]],
        };
        specs.push(BarSpec {
            time: (4, 4),
            voices,
        });
    }
    specs[9].time = (3, 4);
    specs[9].voices = vec![vec![note(NoteValue::Half), note(NoteValue::Quarter)]];
    specs[10].time = (12, 8);
    specs[10].voices = vec![vec![Item::Note(NoteValue::Quarter, 1, None); 4]];
    let mut score = guitar(&specs);
    score.anacrusis = true;
    let masters = &mut score.master_bars;
    masters[1].repeat_start = true;
    masters[2].alternate_endings = 0b011;
    masters[2].repeat_end = Some(4);
    masters[3].alternate_endings = 0b100;
    masters[3].double_bar = true;
    masters[5].key_signature = KeySignature::new(-4, false).unwrap();
    masters[6].key_signature = KeySignature::new(-4, false).unwrap();
    masters[7].key_signature = KeySignature::new(0, false).unwrap();
    masters[12].key_signature = KeySignature::new(5, true).unwrap();
    masters[13].repeat_start = true;
    masters[14].repeat_end = Some(2);
    masters[15].repeat_start = true;
    for master in masters.iter_mut().skip(13) {
        master.key_signature = KeySignature::new(5, true).unwrap();
    }
    score.tracks[0].staves[0].bars[8].clef = Clef::Bass;
    let bass = add_track(&mut score, "Bass guitar", 4, InstrumentKind::Fretted);
    score.tracks[bass].transpose = opentab_model::Transpose::new(0, -1).unwrap();
    for bar in score.tracks[bass].staves[0].bars.iter_mut() {
        bar.clef = Clef::Bass;
    }
    add_track(&mut score, "Drums", 0, InstrumentKind::Percussion);
    score
}

#[test]
fn bar_furniture_on_a_hand_built_multi_system_score_matches_the_snapshot() {
    let score = furniture_score();
    let text = dumped(&score);
    assert!(text.matches("\nsystem ").count() + usize::from(text.starts_with("system")) >= 2);
    insta::assert_snapshot!(text);
}

#[test]
fn the_repeats_fixture_matches_the_snapshot() {
    insta::assert_snapshot!(dumped(&fixture("repeats.gp")));
}

#[test]
fn the_signatures_fixture_matches_the_snapshot() {
    insta::assert_snapshot!(dumped(&fixture("signatures.gp")));
}
