use std::borrow::Cow;

use opentab_model::{
    Diagnostic, DrumArticulation, DrumElement, DrumKit, Instrument, InstrumentKind, InstrumentSet,
    ModelError, NoteKind, Score, TechniquePlacement, Tuning, diagnose, from_json, gm_drum_kit,
    to_json,
};
use serde_json::json;

use crate::support::{four_four, quarters};

const FIRST_GM_KEY: u16 = 35;
const LAST_GM_KEY: u16 = 81;

fn articulation(name: &str, output_key: u8) -> DrumArticulation {
    DrumArticulation {
        name: name.to_string(),
        staff_line: 3,
        noteheads: ["a".to_string(), "b".to_string(), "c".to_string()],
        technique_placement: TechniquePlacement::Below,
        technique_symbol: "sym".to_string(),
        input_keys: vec![output_key, 1],
        rse_sound: "rse".to_string(),
        output_key,
    }
}

fn element(name: &str, count: usize) -> DrumElement {
    DrumElement {
        name: name.to_string(),
        kind: "snare".to_string(),
        soundbank: "bank".to_string(),
        count,
    }
}

fn small_kit() -> DrumKit {
    DrumKit::new(
        vec![element("A", 2), element("Empty", 0), element("B", 1)],
        vec![
            articulation("a1", 60),
            articulation("a2", 61),
            articulation("b1", 7),
        ],
    )
    .unwrap()
}

fn drum_score(articulations: &[u16]) -> Score {
    let mut score = four_four(&quarters(articulations.len()));
    let track = &mut score.tracks[0];
    track.instrument = Instrument {
        kind: InstrumentKind::Percussion,
        midi_program: 0,
        set: Some(InstrumentSet::DrumKit),
    };
    track.tuning = Tuning {
        strings: Vec::new(),
    };
    let beats = &mut track.staves[0].bars[0].voices[0].beats;
    for (beat, articulation) in beats.iter_mut().zip(articulations) {
        beat.notes[0].kind = NoteKind::Percussion {
            articulation: *articulation,
        };
    }
    score
}

#[test]
fn the_general_midi_kit_holds_every_level_one_key_in_order() {
    let kit = gm_drum_kit();
    let keys: Vec<u16> = kit
        .articulations()
        .iter()
        .map(|found| u16::from(found.output_key))
        .collect();
    assert_eq!(keys, (FIRST_GM_KEY..=LAST_GM_KEY).collect::<Vec<u16>>());
    for found in kit.articulations() {
        assert_eq!(found.input_keys, vec![found.output_key]);
        assert_eq!(found.technique_placement, TechniquePlacement::Outside);
        assert!(found.technique_symbol.is_empty() && found.rse_sound.is_empty());
        assert!(!found.name.is_empty());
    }
    let mut names: Vec<&str> = kit
        .articulations()
        .iter()
        .map(|found| found.name.as_str())
        .collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), kit.articulations().len());
    assert_eq!(kit.elements().len(), kit.articulations().len());
    for (element, found) in kit.elements().iter().zip(kit.articulations()) {
        assert_eq!(
            (element.count, element.name.as_str()),
            (1, found.name.as_str())
        );
        assert!(element.soundbank.is_empty());
    }
    let named = |key: u8| {
        kit.articulations()
            .iter()
            .find(|found| found.output_key == key)
            .map(|found| found.name.as_str())
    };
    assert_eq!(named(35), Some("Acoustic Bass Drum"));
    assert_eq!(named(42), Some("Closed Hi-Hat"));
    assert_eq!(named(81), Some("Open Triangle"));
    assert_eq!(
        DrumKit::new(kit.elements().to_vec(), kit.articulations().to_vec()),
        Ok(kit)
    );
}

#[test]
fn key_of_maps_every_index_to_its_articulation_and_nothing_past_the_kit() {
    let kit = gm_drum_kit();
    for index in 0..=u16::MAX {
        let expected = index
            .checked_add(FIRST_GM_KEY)
            .filter(|key| *key <= LAST_GM_KEY)
            .map(|key| u8::try_from(key).unwrap());
        assert_eq!(kit.key_of(index), expected, "{index}");
    }
    let small = small_kit();
    let keys: Vec<Option<u8>> = (0..5).map(|index| small.key_of(index)).collect();
    assert_eq!(keys, vec![Some(60), Some(61), Some(7), None, None]);
}

#[test]
fn a_kit_refuses_uncounted_articulations_bad_keys_and_bad_noteheads() {
    let count = DrumKit::new(vec![element("A", 1)], vec![]);
    assert_eq!(count, Err(ModelError::DrumKitCount { articulations: 0 }));
    let overflow = DrumKit::new(
        vec![element("A", usize::MAX), element("B", 2)],
        vec![articulation("a", 1)],
    );
    assert_eq!(overflow, Err(ModelError::DrumKitCount { articulations: 1 }));
    let output = DrumKit::new(vec![element("A", 1)], vec![articulation("a", 128)]);
    assert_eq!(output, Err(ModelError::InvalidDrumKey { key: 128 }));
    let mut input = articulation("a", 1);
    input.input_keys = vec![3, 200];
    assert_eq!(
        DrumKit::new(vec![element("A", 1)], vec![input]),
        Err(ModelError::InvalidDrumKey { key: 200 })
    );
    for head in ["", "two words", "tab\there"] {
        let mut bad = articulation("a", 1);
        bad.noteheads[1] = head.to_string();
        assert_eq!(
            DrumKit::new(vec![element("A", 1)], vec![bad]),
            Err(ModelError::InvalidNotehead {
                name: head.to_string()
            })
        );
    }
    assert_eq!(
        DrumKit::new(Vec::new(), Vec::new()).map(|kit| kit.articulations().len()),
        Ok(0)
    );
}

#[test]
fn a_kit_round_trips_through_json_and_invalid_kits_are_refused() {
    let mut score = drum_score(&[0, 1, 2, 0]);
    score.tracks[0].drum_kit = Some(small_kit());
    let json = to_json(&score).unwrap();
    assert_eq!(from_json(&json).unwrap(), score);
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value["tracks"][0]["drum_kit"]["elements"][0]["count"] = json!(5);
    assert!(from_json(&value.to_string()).is_err());
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value["tracks"][0]["drum_kit"]["extra"] = json!(1);
    assert!(from_json(&value.to_string()).is_err());
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value["tracks"][0]["drum_kit"]["articulations"][0]["output_key"] = json!(128);
    assert!(from_json(&value.to_string()).is_err());
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value["tracks"][0]
        .as_object_mut()
        .unwrap()
        .remove("drum_kit");
    assert_eq!(
        from_json(&value.to_string()).unwrap().tracks[0].drum_kit,
        None
    );
}

#[test]
fn a_percussion_track_without_a_kit_plays_the_general_midi_kit() {
    let mut score = drum_score(&[0]);
    assert_eq!(score.tracks[0].kit(), Some(Cow::Owned(gm_drum_kit())));
    score.tracks[0].drum_kit = Some(small_kit());
    assert_eq!(score.tracks[0].kit(), Some(Cow::Borrowed(&small_kit())));
    score.tracks[0].instrument.kind = InstrumentKind::Fretted;
    assert_eq!(score.tracks[0].kit(), None);
}

#[test]
fn an_articulation_outside_the_kit_is_diagnosed() {
    let inside = drum_score(&[0, 46, 46, 3]);
    assert_eq!(diagnose(&inside), Vec::new());
    let outside = drum_score(&[0, 47, 46, u16::MAX]);
    let notes: Vec<_> = outside.tracks[0].staves[0].bars[0].voices[0]
        .beats
        .iter()
        .map(|beat| beat.notes[0].id)
        .collect();
    assert_eq!(
        diagnose(&outside),
        vec![
            Diagnostic::ArticulationOutsideKit {
                note: notes[1],
                articulation: 47
            },
            Diagnostic::ArticulationOutsideKit {
                note: notes[3],
                articulation: u16::MAX
            },
        ]
    );
    let mut small = drum_score(&[0, 2, 3, 1]);
    small.tracks[0].drum_kit = Some(small_kit());
    assert_eq!(
        diagnose(&small),
        vec![Diagnostic::ArticulationOutsideKit {
            note: small.tracks[0].staves[0].bars[0].voices[0].beats[2].notes[0].id,
            articulation: 3
        }]
    );
}

#[test]
fn a_kit_on_a_fretted_track_is_diagnosed() {
    let mut score = four_four(&quarters(4));
    assert_eq!(diagnose(&score), Vec::new());
    score.tracks[0].drum_kit = Some(gm_drum_kit());
    assert_eq!(
        diagnose(&score),
        vec![Diagnostic::DrumKitOnFretted {
            track: score.tracks[0].id
        }]
    );
}
