use opentab_command::{Command, CommandError, History, apply, parse_jsonl};
use opentab_model::{
    Diagnostic, DrumKit, ElementId, MAX_DRUM_BEAT_NOTES, NoteKind, TrackId, diagnose, gm_drum_kit,
};
use serde_json::json;

use crate::support::{Fixture, fixture};

fn small_kit() -> DrumKit {
    let kit = gm_drum_kit();
    DrumKit::new(
        kit.elements()[..2].to_vec(),
        kit.articulations()[..2].to_vec(),
    )
    .unwrap()
}

fn rejected(fx: &mut Fixture, command: &Command) -> CommandError {
    let before = fx.score.clone();
    let error = apply(&mut fx.score, command).expect_err("the command is refused");
    assert_eq!(fx.score, before);
    error
}

#[test]
fn set_drum_kit_returns_the_old_kit_and_undoes_exactly() {
    let mut fx = fixture();
    let original = fx.score.clone();
    let mut history = History::new();
    history
        .execute(
            &mut fx.score,
            Command::SetDrumKit {
                track: fx.drums,
                drum_kit: Some(gm_drum_kit()),
            },
        )
        .unwrap();
    let drums = fx.score.find_track(fx.drums).unwrap();
    assert_eq!(drums.drum_kit, Some(gm_drum_kit()));
    assert_eq!(diagnose(&fx.score), Vec::new());
    let mut wide = gm_drum_kit().articulations().to_vec();
    wide.truncate(39);
    let wide_elements = gm_drum_kit().elements()[..39].to_vec();
    let fitting = DrumKit::new(wide_elements, wide).unwrap();
    let inverse = apply(
        &mut fx.score,
        &Command::SetDrumKit {
            track: fx.drums,
            drum_kit: Some(fitting),
        },
    )
    .unwrap();
    assert_eq!(
        inverse,
        Command::SetDrumKit {
            track: fx.drums,
            drum_kit: Some(gm_drum_kit()),
        }
    );
    assert_eq!(diagnose(&fx.score), Vec::new());
    apply(&mut fx.score, &inverse).unwrap();
    assert!(history.undo(&mut fx.score).unwrap());
    assert_eq!(fx.score, original);
    assert!(history.redo(&mut fx.score).unwrap());
    assert_eq!(
        fx.score.find_track(fx.drums).unwrap().drum_kit,
        Some(gm_drum_kit())
    );
}

#[test]
fn a_kit_that_leaves_a_note_outside_it_is_refused() {
    let mut fx = fixture();
    for kit in [small_kit(), DrumKit::new(Vec::new(), Vec::new()).unwrap()] {
        let shrink = Command::SetDrumKit {
            track: fx.drums,
            drum_kit: Some(kit),
        };
        let CommandError::InvalidValue { message } = rejected(&mut fx, &shrink) else {
            panic!("invalid value expected");
        };
        assert!(message.contains("articulation 38"), "{message}");
        assert!(!message.contains("run diagnose"), "{message}");
    }
    let drums = fx
        .score
        .tracks
        .iter_mut()
        .find(|track| track.id == fx.drums)
        .unwrap();
    drums.drum_kit = Some(small_kit());
    assert_eq!(
        diagnose(&fx.score),
        vec![Diagnostic::ArticulationOutsideKit {
            note: fx.drum_note,
            articulation: 38
        }]
    );
    let back = Command::SetDrumKit {
        track: fx.drums,
        drum_kit: None,
    };
    let CommandError::InvalidValue { message } = rejected(&mut fx, &back) else {
        panic!("invalid value expected");
    };
    assert!(message.contains("run diagnose"), "{message}");
}

#[test]
fn a_restored_drum_note_outside_the_kit_is_refused() {
    let mut fx = fixture();
    let inverse = apply(&mut fx.score, &Command::RemoveNote { note: fx.drum_note }).unwrap();
    let Command::RestoreNote {
        beat,
        index,
        mut note,
        extensions,
    } = inverse.clone()
    else {
        panic!("restore_note expected");
    };
    note.kind = NoteKind::Percussion { articulation: 47 };
    let outside = Command::RestoreNote {
        beat,
        index,
        note,
        extensions,
    };
    let CommandError::InvalidValue { message } = rejected(&mut fx, &outside) else {
        panic!("invalid value expected");
    };
    assert!(message.contains("articulation 47"), "{message}");
    apply(&mut fx.score, &inverse).unwrap();
}

#[test]
fn a_kit_is_refused_on_a_fretted_track_and_a_missing_track() {
    let mut fx = fixture();
    let on_guitar = Command::SetDrumKit {
        track: fx.guitar,
        drum_kit: Some(gm_drum_kit()),
    };
    let CommandError::InvalidValue { message } = rejected(&mut fx, &on_guitar) else {
        panic!("invalid value expected");
    };
    assert!(message.contains("not a percussion track"), "{message}");
    assert!(!message.contains("run diagnose"), "{message}");
    let cleared = Command::SetDrumKit {
        track: fx.guitar,
        drum_kit: None,
    };
    assert_eq!(apply(&mut fx.score, &cleared).unwrap(), cleared);
    let missing = Command::SetDrumKit {
        track: TrackId(ElementId(9999)),
        drum_kit: None,
    };
    assert!(matches!(
        rejected(&mut fx, &missing),
        CommandError::NotFound { what: "track", .. }
    ));
}

#[test]
fn a_fretted_track_already_carrying_a_kit_is_reported_as_broken() {
    let mut fx = fixture();
    let guitar = fx
        .score
        .tracks
        .iter_mut()
        .find(|track| track.id == fx.guitar)
        .unwrap();
    guitar.drum_kit = Some(gm_drum_kit());
    assert!(!diagnose(&fx.score).is_empty());
    let clear = Command::SetDrumKit {
        track: fx.guitar,
        drum_kit: None,
    };
    let CommandError::InvalidValue { message } = rejected(&mut fx, &clear) else {
        panic!("invalid value expected");
    };
    assert!(message.contains("run diagnose"), "{message}");
}

#[test]
fn set_drum_kit_has_the_documented_json_shape_and_validates_the_kit() {
    let track = TrackId(ElementId(4));
    let command = Command::SetDrumKit {
        track,
        drum_kit: None,
    };
    assert_eq!(
        serde_json::to_value(&command).unwrap(),
        json!({"op": "set_drum_kit", "track": 4, "drum_kit": null})
    );
    let kit = Command::SetDrumKit {
        track,
        drum_kit: Some(small_kit()),
    };
    let value = serde_json::to_value(&kit).unwrap();
    assert_eq!(
        value["drum_kit"]["articulations"][1]["output_key"],
        json!(36)
    );
    assert_eq!(value["drum_kit"]["elements"][0]["count"], json!(1));
    let back: Command = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(back, kit);
    let mut broken = value;
    broken["drum_kit"]["elements"][0]["count"] = json!(3);
    assert!(serde_json::from_value::<Command>(broken.clone()).is_err());
    assert!(parse_jsonl(&broken.to_string()).is_err());
}

fn fill_drum_beat(fx: &mut Fixture, notes: usize) {
    let mut extra = Vec::new();
    for articulation in 39..39 + notes - 1 {
        extra.push(opentab_model::Note {
            id: fx.score.alloc().unwrap(),
            kind: NoteKind::Percussion {
                articulation: u16::try_from(articulation).unwrap(),
            },
            effects: opentab_model::NoteEffects::default(),
        });
    }
    let path = fx.score.find_beat(fx.drum_beat).unwrap();
    let beat = &mut fx.score.tracks[path.track].staves[path.staff].bars[path.bar].voices
        [path.voice]
        .beats[path.beat];
    beat.notes.extend(extra);
}

#[test]
fn a_seventh_note_in_a_drum_beat_is_diagnosed_and_refused() {
    let mut fx = fixture();
    fill_drum_beat(&mut fx, MAX_DRUM_BEAT_NOTES);
    assert_eq!(diagnose(&fx.score), Vec::new());
    let inverse = apply(&mut fx.score, &Command::RemoveNote { note: fx.drum_note }).unwrap();
    let Command::RestoreNote {
        beat,
        index,
        mut note,
        extensions,
    } = inverse.clone()
    else {
        panic!("restore_note expected");
    };
    apply(&mut fx.score, &inverse).unwrap();
    note.id = fx.score.alloc().unwrap();
    let seventh = Command::RestoreNote {
        beat,
        index,
        note,
        extensions,
    };
    let CommandError::InvalidValue { message } = rejected(&mut fx, &seventh) else {
        panic!("invalid value expected");
    };
    assert!(message.contains("at most 6"), "{message}");
    fill_drum_beat(&mut fx, 2);
    assert_eq!(
        diagnose(&fx.score),
        vec![Diagnostic::TooManyDrumNotes {
            beat: fx.drum_beat,
            notes: MAX_DRUM_BEAT_NOTES + 1
        }]
    );
    let remove = Command::RemoveBeat { beat: fx.drum_beat };
    let CommandError::InvalidValue { message } = rejected(&mut fx, &remove) else {
        panic!("invalid value expected");
    };
    assert!(message.contains("run diagnose"), "{message}");
}
