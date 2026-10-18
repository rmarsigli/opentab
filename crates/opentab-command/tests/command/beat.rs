use opentab_command::{Command, CommandError, ExtensionEntry, apply};
use opentab_model::{BeatEffects, BeatId, Duration, ElementId, NoteValue, Tuplet, VoiceId};

use crate::checks::{rejected, round_trip};
use crate::support::{duration, fixture, guitar_voice, payload};

#[test]
fn insert_beat_adds_a_rest_with_a_fresh_id() {
    let mut fx = fixture();
    let before_ids = fx.score.ids.clone();
    let fresh = BeatId(fx.score.ids.peek());
    let triplet = Duration::new(NoteValue::Eighth, 1, Some(Tuplet { num: 3, den: 2 })).unwrap();
    let command = Command::InsertBeat {
        voice: fx.voice,
        index: 2,
        duration: triplet,
    };
    let inverse = round_trip(&mut fx.score, &command);
    assert_eq!(
        inverse,
        Command::RestoreIds {
            ids: before_ids,
            then: Box::new(Command::RemoveBeat { beat: fresh }),
        }
    );
    apply(&mut fx.score, &command).unwrap();
    let voice = guitar_voice(&fx.score, 0);
    assert_eq!(voice.beats.len(), 5);
    let inserted = &voice.beats[2];
    assert_eq!(inserted.id, fresh);
    assert_eq!(inserted.duration, triplet);
    assert!(inserted.notes.is_empty());
    assert_eq!(inserted.effects, BeatEffects::default());
    assert_eq!(fx.score.ids.peek(), ElementId(fresh.0.0 + 1));
}

#[test]
fn insert_beat_at_the_end_and_into_an_empty_voice() {
    let mut fx = fixture();
    let end = Command::InsertBeat {
        voice: fx.voice,
        index: 4,
        duration: duration(NoteValue::Half),
    };
    round_trip(&mut fx.score, &end);
    apply(&mut fx.score, &Command::RemoveMasterBar { index: 1 }).unwrap();
    apply(
        &mut fx.score,
        &Command::InsertMasterBar {
            index: 1,
            time_signature: crate::support::four_four(),
        },
    )
    .unwrap();
    let empty = guitar_voice(&fx.score, 1).id;
    let command = Command::InsertBeat {
        voice: empty,
        index: 0,
        duration: duration(NoteValue::Whole),
    };
    round_trip(&mut fx.score, &command);
}

#[test]
fn insert_beat_past_the_end_is_out_of_range() {
    let mut fx = fixture();
    let command = Command::InsertBeat {
        voice: fx.voice,
        index: 5,
        duration: duration(NoteValue::Quarter),
    };
    assert_eq!(
        rejected(&mut fx.score, &command),
        CommandError::IndexOutOfRange {
            what: "beat",
            index: 5,
            len: 4
        }
    );
}

#[test]
fn insert_beat_into_an_unknown_voice_is_not_found() {
    let mut fx = fixture();
    let command = Command::InsertBeat {
        voice: VoiceId(ElementId(9999)),
        index: 0,
        duration: duration(NoteValue::Quarter),
    };
    assert_eq!(
        rejected(&mut fx.score, &command),
        CommandError::NotFound {
            what: "voice",
            id: 9999
        }
    );
}

#[test]
fn remove_beat_takes_the_beat_and_its_extensions() {
    let mut fx = fixture();
    let target = fx.beats[2];
    let original = guitar_voice(&fx.score, 0).beats[2].clone();
    let command = Command::RemoveBeat { beat: target };
    let inverse = round_trip(&mut fx.score, &command);
    let extensions = vec![ExtensionEntry {
        id: ElementId::from(target),
        payloads: payload("beat"),
    }];
    assert_eq!(
        inverse,
        Command::RestoreBeat {
            voice: fx.voice,
            index: 2,
            beat: original,
            extensions,
        }
    );
    apply(&mut fx.score, &command).unwrap();
    assert_eq!(guitar_voice(&fx.score, 0).beats.len(), 3);
    assert!(!fx.score.extensions.0.contains_key(&ElementId::from(target)));
}

#[test]
fn remove_beat_takes_the_extensions_of_its_notes() {
    let mut fx = fixture();
    let note = ElementId::from(fx.notes[1]);
    let inverse = round_trip(&mut fx.score, &Command::RemoveBeat { beat: fx.beats[0] });
    let Command::RestoreBeat { extensions, .. } = inverse else {
        panic!("restore_beat expected");
    };
    assert_eq!(
        extensions,
        vec![ExtensionEntry {
            id: note,
            payloads: payload("note")
        }]
    );
}

#[test]
fn remove_beat_unknown_is_not_found() {
    let mut fx = fixture();
    assert_eq!(
        rejected(
            &mut fx.score,
            &Command::RemoveBeat {
                beat: BeatId(ElementId(4242))
            }
        ),
        CommandError::NotFound {
            what: "beat",
            id: 4242
        }
    );
}

fn removed_beat() -> (crate::support::Fixture, Command) {
    let mut fx = fixture();
    let inverse = apply(&mut fx.score, &Command::RemoveBeat { beat: fx.beats[1] }).unwrap();
    (fx, inverse)
}

#[test]
fn restore_beat_rejects_an_id_already_in_the_score() {
    let (mut fx, inverse) = removed_beat();
    let Command::RestoreBeat {
        voice,
        index,
        mut beat,
        extensions,
    } = inverse
    else {
        panic!("restore_beat expected");
    };
    beat.id = fx.beats[3];
    let command = Command::RestoreBeat {
        voice,
        index,
        beat,
        extensions,
    };
    assert!(matches!(
        rejected(&mut fx.score, &command),
        CommandError::InvalidValue { .. }
    ));
}

#[test]
fn restore_beat_rejects_a_bad_index_and_an_unknown_voice() {
    let (mut fx, inverse) = removed_beat();
    let Command::RestoreBeat {
        voice,
        beat,
        extensions,
        ..
    } = inverse
    else {
        panic!("restore_beat expected");
    };
    let far = Command::RestoreBeat {
        voice,
        index: 9,
        beat: beat.clone(),
        extensions: extensions.clone(),
    };
    assert_eq!(
        rejected(&mut fx.score, &far),
        CommandError::IndexOutOfRange {
            what: "beat",
            index: 9,
            len: 3
        }
    );
    let lost = Command::RestoreBeat {
        voice: VoiceId(ElementId(777)),
        index: 0,
        beat,
        extensions,
    };
    assert_eq!(
        rejected(&mut fx.score, &lost),
        CommandError::NotFound {
            what: "voice",
            id: 777
        }
    );
}

#[test]
fn set_beat_duration_replaces_and_inverts() {
    let mut fx = fixture();
    let command = Command::SetBeatDuration {
        beat: fx.beats[1],
        duration: duration(NoteValue::Sixteenth),
    };
    let inverse = round_trip(&mut fx.score, &command);
    assert_eq!(
        inverse,
        Command::SetBeatDuration {
            beat: fx.beats[1],
            duration: duration(NoteValue::Quarter),
        }
    );
    apply(&mut fx.score, &command).unwrap();
    assert_eq!(
        guitar_voice(&fx.score, 0).beats[1].duration,
        duration(NoteValue::Sixteenth)
    );
}

#[test]
fn set_beat_duration_unknown_is_not_found() {
    let mut fx = fixture();
    let command = Command::SetBeatDuration {
        beat: BeatId(ElementId(31337)),
        duration: duration(NoteValue::Half),
    };
    assert_eq!(
        rejected(&mut fx.score, &command),
        CommandError::NotFound {
            what: "beat",
            id: 31337
        }
    );
}
