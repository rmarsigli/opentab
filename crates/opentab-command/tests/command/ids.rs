use opentab_command::{Command, CommandError, MetaField, apply};
use opentab_model::{ElementId, IdAllocator, NoteValue};

use crate::checks::{rejected, round_trip};
use crate::support::{duration, fixture, four_four};

fn allocator(next: u64) -> IdAllocator {
    serde_json::from_value(serde_json::json!({ "next": next })).unwrap()
}

#[test]
fn restore_ids_applies_the_inner_command_then_sets_the_allocator() {
    let mut fx = fixture();
    let old = fx.score.ids.clone();
    let target = allocator(old.peek().0 + 10);
    let command = Command::RestoreIds {
        ids: target.clone(),
        then: Box::new(Command::SetMeta {
            field: MetaField::Artist,
            value: "x".to_owned(),
        }),
    };
    let inverse = round_trip(&mut fx.score, &command);
    assert_eq!(
        inverse,
        Command::RestoreIds {
            ids: old,
            then: Box::new(Command::SetMeta {
                field: MetaField::Artist,
                value: String::new(),
            }),
        }
    );
    apply(&mut fx.score, &command).unwrap();
    assert_eq!(fx.score.ids, target);
    assert_eq!(fx.score.meta.artist, "x");
}

#[test]
fn restore_ids_nests() {
    let mut fx = fixture();
    let command = Command::RestoreIds {
        ids: allocator(500),
        then: Box::new(Command::RestoreIds {
            ids: allocator(400),
            then: Box::new(Command::InsertBeat {
                voice: fx.voice,
                index: 0,
                duration: duration(NoteValue::Eighth),
            }),
        }),
    };
    round_trip(&mut fx.score, &command);
    apply(&mut fx.score, &command).unwrap();
    assert_eq!(fx.score.ids, allocator(500));
}

#[test]
fn restore_ids_with_a_failing_inner_command_changes_nothing() {
    let mut fx = fixture();
    let command = Command::RestoreIds {
        ids: allocator(1),
        then: Box::new(Command::RemoveMasterBar { index: 9 }),
    };
    assert!(matches!(
        rejected(&mut fx.score, &command),
        CommandError::IndexOutOfRange { .. }
    ));
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
fn undoing_an_insert_rolls_the_allocator_back() {
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
    assert_ne!(fx.score.ids, before);
    apply(&mut fx.score, &inverse).unwrap();
    assert_eq!(fx.score.ids, before);
    assert_eq!(fx.score.ids.peek(), ElementId(before.peek().0));
}
