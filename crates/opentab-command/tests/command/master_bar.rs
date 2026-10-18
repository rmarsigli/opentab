use opentab_command::{Command, CommandError, ExtensionEntry, apply};
use opentab_model::{ElementId, KeySignature, TimeSignature};

use crate::checks::{rejected, round_trip};
use crate::support::{fixture, four_four, payload};

fn three_four() -> TimeSignature {
    TimeSignature::new(3, 4).unwrap()
}

#[test]
fn insert_master_bar_adds_an_empty_bar_to_every_staff() {
    let mut fx = fixture();
    let first_id = fx.score.ids.peek().0;
    let before_ids = fx.score.ids.clone();
    let command = Command::InsertMasterBar {
        index: 1,
        time_signature: three_four(),
    };
    let inverse = round_trip(&mut fx.score, &command);
    assert_eq!(
        inverse,
        Command::RestoreIds {
            ids: before_ids,
            then: Box::new(Command::RemoveMasterBar { index: 1 }),
        }
    );
    apply(&mut fx.score, &command).unwrap();
    let inserted = &fx.score.master_bars[1];
    assert_eq!(inserted.id.0, ElementId(first_id));
    assert_eq!(inserted.time_signature, three_four());
    assert_eq!(
        inserted.key_signature,
        KeySignature {
            accidentals: 2,
            minor: false
        }
    );
    assert!(inserted.tempo_changes.is_empty());
    assert_eq!(fx.score.master_bars.len(), 3);
    let mut expected = first_id + 1;
    for track in &fx.score.tracks {
        let staff = &track.staves[0];
        assert_eq!(staff.bars.len(), 3);
        let bar = &staff.bars[1];
        assert_eq!(bar.id.0, ElementId(expected));
        assert_eq!(bar.voices.len(), 1);
        assert_eq!(bar.voices[0].id.0, ElementId(expected + 1));
        assert!(bar.voices[0].beats.is_empty());
        expected += 2;
    }
    assert_eq!(fx.score.ids.peek(), ElementId(expected));
}

#[test]
fn insert_master_bar_at_the_start_uses_the_default_key() {
    let mut fx = fixture();
    let command = Command::InsertMasterBar {
        index: 0,
        time_signature: four_four(),
    };
    round_trip(&mut fx.score, &command);
    apply(&mut fx.score, &command).unwrap();
    assert_eq!(
        fx.score.master_bars[0].key_signature,
        KeySignature {
            accidentals: 0,
            minor: false
        }
    );
}

#[test]
fn insert_master_bar_appends_at_the_end() {
    let mut fx = fixture();
    let command = Command::InsertMasterBar {
        index: 2,
        time_signature: four_four(),
    };
    round_trip(&mut fx.score, &command);
}

#[test]
fn insert_master_bar_past_the_end_is_out_of_range() {
    let mut fx = fixture();
    let command = Command::InsertMasterBar {
        index: 3,
        time_signature: four_four(),
    };
    assert_eq!(
        rejected(&mut fx.score, &command),
        CommandError::IndexOutOfRange {
            what: "master bar",
            index: 3,
            len: 2
        }
    );
}

#[test]
fn insert_master_bar_rejects_a_zero_time_signature() {
    let mut fx = fixture();
    let command = Command::InsertMasterBar {
        index: 0,
        time_signature: TimeSignature {
            numerator: 0,
            denominator: 4,
        },
    };
    assert!(matches!(
        rejected(&mut fx.score, &command),
        CommandError::InvalidValue { .. }
    ));
}

#[test]
fn insert_master_bar_fails_atomically_when_the_second_staff_is_short() {
    let mut fx = fixture();
    fx.score.tracks[1].staves[0].bars.pop();
    let command = Command::InsertMasterBar {
        index: 2,
        time_signature: four_four(),
    };
    assert!(matches!(
        rejected(&mut fx.score, &command),
        CommandError::InvalidValue { .. }
    ));
}

#[test]
fn remove_master_bar_takes_its_bars_and_extensions() {
    let mut fx = fixture();
    let master_id = ElementId::from(fx.score.master_bars[1].id);
    let command = Command::RemoveMasterBar { index: 1 };
    let inverse = round_trip(&mut fx.score, &command);
    let Command::RestoreMasterBar {
        index,
        master_bar,
        bars,
        extensions,
    } = &inverse
    else {
        panic!("inverse of remove_master_bar is restore_master_bar");
    };
    assert_eq!(*index, 1);
    assert_eq!(ElementId::from(master_bar.id), master_id);
    assert_eq!(bars.len(), 2);
    assert_eq!(
        extensions,
        &vec![ExtensionEntry {
            id: master_id,
            payloads: payload("master")
        }]
    );
    apply(&mut fx.score, &command).unwrap();
    assert_eq!(fx.score.master_bars.len(), 1);
    assert!(fx.score.tracks.iter().all(|t| t.staves[0].bars.len() == 1));
    assert!(!fx.score.extensions.0.contains_key(&master_id));
    assert_eq!(fx.score.extensions.0.len(), 2);
}

#[test]
fn remove_master_bar_out_of_range() {
    let mut fx = fixture();
    assert_eq!(
        rejected(&mut fx.score, &Command::RemoveMasterBar { index: 2 }),
        CommandError::IndexOutOfRange {
            what: "master bar",
            index: 2,
            len: 2
        }
    );
}

#[test]
fn remove_master_bar_rejects_a_staff_with_missing_bars() {
    let mut fx = fixture();
    fx.score.tracks[1].staves[0].bars.pop();
    assert!(matches!(
        rejected(&mut fx.score, &Command::RemoveMasterBar { index: 0 }),
        CommandError::InvalidValue { .. }
    ));
}

fn removed_second_bar() -> (crate::support::Fixture, Command) {
    let mut fx = fixture();
    let inverse = apply(&mut fx.score, &Command::RemoveMasterBar { index: 1 }).unwrap();
    (fx, inverse)
}

#[test]
fn restore_master_bar_rejects_a_bar_count_that_does_not_match_the_staves() {
    let (mut fx, inverse) = removed_second_bar();
    let Command::RestoreMasterBar {
        index,
        master_bar,
        mut bars,
        extensions,
    } = inverse
    else {
        panic!("restore expected");
    };
    bars.pop();
    let command = Command::RestoreMasterBar {
        index,
        master_bar,
        bars,
        extensions,
    };
    assert!(matches!(
        rejected(&mut fx.score, &command),
        CommandError::InvalidValue { .. }
    ));
}

#[test]
fn restore_master_bar_rejects_ids_already_in_the_score() {
    let (mut fx, inverse) = removed_second_bar();
    let Command::RestoreMasterBar {
        index,
        mut master_bar,
        bars,
        extensions,
    } = inverse
    else {
        panic!("restore expected");
    };
    master_bar.id = fx.score.master_bars[0].id;
    let command = Command::RestoreMasterBar {
        index,
        master_bar,
        bars,
        extensions,
    };
    assert!(matches!(
        rejected(&mut fx.score, &command),
        CommandError::InvalidValue { .. }
    ));
}

#[test]
fn restore_master_bar_rejects_foreign_extensions() {
    let (mut fx, inverse) = removed_second_bar();
    let Command::RestoreMasterBar {
        index,
        master_bar,
        bars,
        ..
    } = inverse
    else {
        panic!("restore expected");
    };
    let extensions = vec![ExtensionEntry {
        id: ElementId::from(fx.score.master_bars[0].id),
        payloads: payload("x"),
    }];
    let command = Command::RestoreMasterBar {
        index,
        master_bar,
        bars,
        extensions,
    };
    assert!(matches!(
        rejected(&mut fx.score, &command),
        CommandError::InvalidValue { .. }
    ));
}

#[test]
fn restore_master_bar_out_of_range() {
    let (mut fx, inverse) = removed_second_bar();
    let Command::RestoreMasterBar {
        master_bar,
        bars,
        extensions,
        ..
    } = inverse
    else {
        panic!("restore expected");
    };
    let command = Command::RestoreMasterBar {
        index: 5,
        master_bar,
        bars,
        extensions,
    };
    assert_eq!(
        rejected(&mut fx.score, &command),
        CommandError::IndexOutOfRange {
            what: "master bar",
            index: 5,
            len: 1
        }
    );
}
