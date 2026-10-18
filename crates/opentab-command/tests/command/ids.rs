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
