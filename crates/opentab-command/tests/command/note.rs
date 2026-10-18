use opentab_command::{Command, CommandError, ExtensionEntry, apply};
use opentab_model::{BeatId, ElementId, NoteEffects, NoteId, NoteKind};

use crate::checks::{rejected, round_trip};
use crate::support::{fixture, guitar_voice, payload};

#[test]
fn add_note_appends_a_fretted_note_with_a_fresh_id() {
    let mut fx = fixture();
    let before_ids = fx.score.ids.clone();
    let fresh = NoteId(fx.score.ids.peek());
    let command = Command::AddNote {
        beat: fx.beats[1],
        string: 5,
        fret: 9,
    };
    let inverse = round_trip(&mut fx.score, &command);
    assert_eq!(
        inverse,
        Command::RestoreIds {
            ids: before_ids,
            then: Box::new(Command::RemoveNote { note: fresh }),
        }
    );
    apply(&mut fx.score, &command).unwrap();
    let beat = &guitar_voice(&fx.score, 0).beats[1];
    assert_eq!(beat.notes.len(), 1);
    assert_eq!(beat.notes[0].id, fresh);
    assert_eq!(beat.notes[0].kind, NoteKind::Fretted { string: 5, fret: 9 });
    assert_eq!(beat.notes[0].effects, NoteEffects::default());
}

#[test]
fn add_note_beside_existing_notes_goes_last() {
    let mut fx = fixture();
    let command = Command::AddNote {
        beat: fx.beats[0],
        string: 3,
        fret: 2,
    };
    round_trip(&mut fx.score, &command);
    apply(&mut fx.score, &command).unwrap();
    let notes = &guitar_voice(&fx.score, 0).beats[0].notes;
    assert_eq!(notes.len(), 3);
    assert_eq!(notes[2].kind, NoteKind::Fretted { string: 3, fret: 2 });
}

#[test]
fn add_note_on_a_string_past_the_tuning_is_invalid() {
    let mut fx = fixture();
    let command = Command::AddNote {
        beat: fx.beats[1],
        string: 6,
        fret: 0,
    };
    assert!(matches!(
        rejected(&mut fx.score, &command),
        CommandError::InvalidValue { .. }
    ));
}

#[test]
fn add_note_on_an_occupied_string_is_invalid() {
    let mut fx = fixture();
    let command = Command::AddNote {
        beat: fx.beats[0],
        string: 2,
        fret: 1,
    };
    assert!(matches!(
        rejected(&mut fx.score, &command),
        CommandError::InvalidValue { .. }
    ));
}

#[test]
fn add_note_on_percussion_is_the_wrong_instrument() {
    let mut fx = fixture();
    let command = Command::AddNote {
        beat: fx.drum_beat,
        string: 0,
        fret: 0,
    };
    assert_eq!(
        rejected(&mut fx.score, &command),
        CommandError::WrongInstrument { track: fx.drums }
    );
}

#[test]
fn add_note_on_an_unknown_beat_is_not_found() {
    let mut fx = fixture();
    let command = Command::AddNote {
        beat: BeatId(ElementId(5000)),
        string: 0,
        fret: 0,
    };
    assert_eq!(
        rejected(&mut fx.score, &command),
        CommandError::NotFound {
            what: "beat",
            id: 5000
        }
    );
}

#[test]
fn remove_note_takes_the_note_and_its_extensions() {
    let mut fx = fixture();
    let target = fx.notes[1];
    let original = guitar_voice(&fx.score, 0).beats[0].notes[1].clone();
    let command = Command::RemoveNote { note: target };
    let inverse = round_trip(&mut fx.score, &command);
    let extensions = vec![ExtensionEntry {
        id: ElementId::from(target),
        payloads: payload("note"),
    }];
    assert_eq!(
        inverse,
        Command::RestoreNote {
            beat: fx.beats[0],
            index: 1,
            note: original,
            extensions,
        }
    );
    apply(&mut fx.score, &command).unwrap();
    assert_eq!(guitar_voice(&fx.score, 0).beats[0].notes.len(), 1);
    assert!(!fx.score.extensions.0.contains_key(&ElementId::from(target)));
}

#[test]
fn remove_note_unknown_is_not_found() {
    let mut fx = fixture();
    assert_eq!(
        rejected(
            &mut fx.score,
            &Command::RemoveNote {
                note: NoteId(ElementId(888))
            }
        ),
        CommandError::NotFound {
            what: "note",
            id: 888
        }
    );
}

#[test]
fn restore_note_validates_ids_index_and_beat() {
    let mut fx = fixture();
    let inverse = apply(&mut fx.score, &Command::RemoveNote { note: fx.notes[0] }).unwrap();
    let Command::RestoreNote {
        beat,
        note,
        extensions,
        ..
    } = inverse
    else {
        panic!("restore_note expected");
    };
    let mut clash = note.clone();
    clash.id = fx.notes[1];
    let duplicate = Command::RestoreNote {
        beat,
        index: 0,
        note: clash,
        extensions: extensions.clone(),
    };
    assert!(matches!(
        rejected(&mut fx.score, &duplicate),
        CommandError::InvalidValue { .. }
    ));
    let far = Command::RestoreNote {
        beat,
        index: 2,
        note: note.clone(),
        extensions: extensions.clone(),
    };
    assert_eq!(
        rejected(&mut fx.score, &far),
        CommandError::IndexOutOfRange {
            what: "note",
            index: 2,
            len: 1
        }
    );
    let lost = Command::RestoreNote {
        beat: BeatId(ElementId(606)),
        index: 0,
        note,
        extensions,
    };
    assert_eq!(
        rejected(&mut fx.score, &lost),
        CommandError::NotFound {
            what: "beat",
            id: 606
        }
    );
}

#[test]
fn set_fret_replaces_and_inverts() {
    let mut fx = fixture();
    let command = Command::SetFret {
        note: fx.notes[0],
        fret: 15,
    };
    let inverse = round_trip(&mut fx.score, &command);
    assert_eq!(
        inverse,
        Command::SetFret {
            note: fx.notes[0],
            fret: 3
        }
    );
    apply(&mut fx.score, &command).unwrap();
    assert_eq!(
        guitar_voice(&fx.score, 0).beats[0].notes[0].kind,
        NoteKind::Fretted {
            string: 0,
            fret: 15
        }
    );
}

#[test]
fn set_fret_on_a_drum_note_is_the_wrong_instrument() {
    let mut fx = fixture();
    let command = Command::SetFret {
        note: fx.drum_note,
        fret: 1,
    };
    assert_eq!(
        rejected(&mut fx.score, &command),
        CommandError::WrongInstrument { track: fx.drums }
    );
}

#[test]
fn set_fret_unknown_is_not_found() {
    let mut fx = fixture();
    let command = Command::SetFret {
        note: NoteId(ElementId(123456)),
        fret: 1,
    };
    assert_eq!(
        rejected(&mut fx.score, &command),
        CommandError::NotFound {
            what: "note",
            id: 123456
        }
    );
}
