use opentab_command::{Command, CommandError, apply};
use opentab_model::{ElementId, IdAllocator, NoteValue};

use crate::checks::rejected;
use crate::support::{duration, fixture, four_four};

fn allocator(next: u64) -> IdAllocator {
    serde_json::from_value(serde_json::json!({ "next": next })).unwrap()
}

#[test]
fn inserts_refuse_an_allocator_behind_existing_ids() {
    let mut fx = fixture();
    fx.score.ids = allocator(3);
    let commands = [
        Command::InsertBeat {
            voice: fx.voice,
            index: 0,
            duration: duration(NoteValue::Quarter),
        },
        Command::AddNote {
            beat: fx.beats[1],
            string: 0,
            fret: 0,
        },
        Command::InsertMasterBar {
            index: 0,
            time_signature: four_four(),
        },
    ];
    for command in &commands {
        assert!(matches!(
            rejected(&mut fx.score, command),
            CommandError::InvalidValue { .. }
        ));
    }
}

#[test]
fn inserts_refuse_an_exhausted_id_space() {
    let mut fx = fixture();
    fx.score.ids = allocator(u64::MAX);
    let command = Command::InsertBeat {
        voice: fx.voice,
        index: 0,
        duration: duration(NoteValue::Quarter),
    };
    assert!(matches!(
        rejected(&mut fx.score, &command),
        CommandError::InvalidValue { .. }
    ));
}

#[test]
fn undoing_an_insert_keeps_the_allocator() {
    let mut fx = fixture();
    let before = fx.score.ids.clone();
    let inverse = apply(
        &mut fx.score,
        &Command::InsertMasterBar {
            index: 0,
            time_signature: four_four(),
        },
    )
    .unwrap();
    let advanced = fx.score.ids.clone();
    assert!(advanced.peek() > before.peek());
    apply(&mut fx.score, &inverse).unwrap();
    assert_eq!(fx.score.ids, advanced);
    assert_eq!(fx.score.ids.peek(), ElementId(before.peek().0 + 5));
}

#[test]
fn a_failed_insert_consumes_no_ids() {
    let mut fx = fixture();
    let command = Command::AddNote {
        beat: fx.beats[0],
        string: 0,
        fret: 1,
    };
    rejected(&mut fx.score, &command);
}

fn inserts_still_work(fx: &mut crate::support::Fixture) {
    let voice = fx.voice;
    apply(
        &mut fx.score,
        &Command::InsertBeat {
            voice,
            index: 0,
            duration: duration(NoteValue::Quarter),
        },
    )
    .expect("insert beat still works");
    let beat = fx.beats[1];
    apply(
        &mut fx.score,
        &Command::AddNote {
            beat,
            string: 0,
            fret: 0,
        },
    )
    .expect("add note still works");
    apply(
        &mut fx.score,
        &Command::InsertMasterBar {
            index: 0,
            time_signature: four_four(),
        },
    )
    .expect("insert master bar still works");
}

fn above_allocator(error: CommandError) {
    let CommandError::InvalidValue { message } = error else {
        panic!("expected invalid value, got {error:?}");
    };
    assert!(message.contains("allocator"), "{message}");
}

#[test]
fn restore_master_bar_rejects_ids_at_or_above_the_allocator() {
    let mut fx = fixture();
    let inverse = apply(&mut fx.score, &Command::RemoveMasterBar { index: 1 }).unwrap();
    let Command::RestoreMasterBar {
        index,
        master_bar,
        bars,
        extensions,
    } = inverse
    else {
        panic!("restore_master_bar expected");
    };
    let mut high_master = master_bar.clone();
    high_master.id = opentab_model::MasterBarId(fx.score.ids.peek());
    let mut high_voice = bars.clone();
    high_voice[1].voices[0].id = opentab_model::VoiceId(ElementId(9999));
    let cases = [
        (high_master, bars.clone(), Vec::new()),
        (master_bar, high_voice, extensions),
    ];
    for (master_bar, bars, extensions) in cases {
        let command = Command::RestoreMasterBar {
            index,
            master_bar,
            bars,
            extensions,
        };
        above_allocator(rejected(&mut fx.score, &command));
    }
    inserts_still_work(&mut fx);
}

#[test]
fn restore_beat_rejects_ids_at_or_above_the_allocator() {
    let mut fx = fixture();
    let inverse = apply(&mut fx.score, &Command::RemoveBeat { beat: fx.beats[0] }).unwrap();
    let Command::RestoreBeat {
        voice,
        index,
        beat,
        extensions,
    } = inverse
    else {
        panic!("restore_beat expected");
    };
    let mut high_beat = beat.clone();
    high_beat.id = opentab_model::BeatId(fx.score.ids.peek());
    let mut high_note = beat.clone();
    high_note.notes[0].id = opentab_model::NoteId(ElementId(9999));
    for beat in [high_beat, high_note] {
        let command = Command::RestoreBeat {
            voice,
            index,
            beat,
            extensions: extensions.clone(),
        };
        above_allocator(rejected(&mut fx.score, &command));
    }
    inserts_still_work(&mut fx);
}

#[test]
fn restore_note_rejects_ids_at_or_above_the_allocator() {
    let mut fx = fixture();
    let inverse = apply(&mut fx.score, &Command::RemoveNote { note: fx.notes[0] }).unwrap();
    let Command::RestoreNote {
        beat,
        index,
        note,
        extensions,
    } = inverse
    else {
        panic!("restore_note expected");
    };
    for id in [fx.score.ids.peek(), ElementId(9999)] {
        let mut high = note.clone();
        high.id = opentab_model::NoteId(id);
        let command = Command::RestoreNote {
            beat,
            index,
            note: high,
            extensions: extensions.clone(),
        };
        above_allocator(rejected(&mut fx.score, &command));
    }
    inserts_still_work(&mut fx);
}
