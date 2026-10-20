use opentab_command::{Command, CommandError, apply};
use opentab_model::{Fraction, Tempo, TempoChange, TempoUnit};

use crate::checks::{rejected, round_trip};
use crate::support::fixture;

fn change(position: Fraction, bpm: f64) -> TempoChange {
    TempoChange {
        position,
        bpm: Tempo::new(bpm).unwrap(),
        unit: TempoUnit::Quarter,
        linear: false,
    }
}

#[test]
fn set_tempo_inserts_a_change_at_position_zero() {
    let mut fx = fixture();
    let command = Command::SetTempo {
        master_bar: 0,
        bpm: 90.5,
    };
    let inverse = round_trip(&mut fx.score, &command);
    assert_eq!(
        inverse,
        Command::RestoreTempoChanges {
            master_bar: 0,
            tempo_changes: Vec::new(),
        }
    );
    apply(&mut fx.score, &command).unwrap();
    assert_eq!(
        fx.score.master_bars[0].tempo_changes,
        vec![change(Fraction::from_integer(0), 90.5)]
    );
}

#[test]
fn set_tempo_replaces_the_change_at_position_zero() {
    let mut fx = fixture();
    let command = Command::SetTempo {
        master_bar: 1,
        bpm: 140.0,
    };
    let inverse = round_trip(&mut fx.score, &command);
    assert_eq!(
        inverse,
        Command::RestoreTempoChanges {
            master_bar: 1,
            tempo_changes: vec![change(Fraction::from_integer(0), 120.0)],
        }
    );
    apply(&mut fx.score, &command).unwrap();
    assert_eq!(
        fx.score.master_bars[1].tempo_changes,
        vec![change(Fraction::from_integer(0), 140.0)]
    );
}

#[test]
fn set_tempo_keeps_later_changes() {
    let mut fx = fixture();
    let later = change(Fraction::new(1, 2), 60.0);
    fx.score.master_bars[0].tempo_changes = vec![later.clone()];
    let command = Command::SetTempo {
        master_bar: 0,
        bpm: 100.0,
    };
    round_trip(&mut fx.score, &command);
    apply(&mut fx.score, &command).unwrap();
    assert_eq!(
        fx.score.master_bars[0].tempo_changes,
        vec![change(Fraction::from_integer(0), 100.0), later]
    );
}

#[test]
fn set_tempo_rejects_invalid_bpm() {
    let mut fx = fixture();
    for bpm in [0.0, 0.99, 1000.5, -5.0, f64::NAN, f64::INFINITY] {
        let command = Command::SetTempo { master_bar: 0, bpm };
        assert!(matches!(
            rejected(&mut fx.score, &command),
            CommandError::InvalidValue { .. }
        ));
    }
}

#[test]
fn set_tempo_out_of_range() {
    let mut fx = fixture();
    let command = Command::SetTempo {
        master_bar: 2,
        bpm: 120.0,
    };
    assert_eq!(
        rejected(&mut fx.score, &command),
        CommandError::IndexOutOfRange {
            what: "master bar",
            index: 2,
            len: 2
        }
    );
}

#[test]
fn restore_tempo_changes_validates_index_and_positions() {
    let mut fx = fixture();
    let far = Command::RestoreTempoChanges {
        master_bar: 7,
        tempo_changes: Vec::new(),
    };
    assert_eq!(
        rejected(&mut fx.score, &far),
        CommandError::IndexOutOfRange {
            what: "master bar",
            index: 7,
            len: 2
        }
    );
    let negative = Command::RestoreTempoChanges {
        master_bar: 0,
        tempo_changes: vec![change(Fraction::new(-1, 4), 80.0)],
    };
    assert!(matches!(
        rejected(&mut fx.score, &negative),
        CommandError::InvalidValue { .. }
    ));
    let valid = Command::RestoreTempoChanges {
        master_bar: 0,
        tempo_changes: vec![change(Fraction::new(1, 4), 80.0)],
    };
    round_trip(&mut fx.score, &valid);
}

#[test]
fn set_tempo_linear_flips_one_change_and_undoes() {
    let mut fx = fixture();
    let first = change(Fraction::from_integer(0), 60.0);
    let second = change(Fraction::new(1, 2), 90.0);
    fx.score.master_bars[0].tempo_changes = vec![first.clone(), second.clone()];
    let command = Command::SetTempoLinear {
        master_bar: 0,
        index: 1,
        linear: true,
    };
    let inverse = round_trip(&mut fx.score, &command);
    assert_eq!(
        inverse,
        Command::SetTempoLinear {
            master_bar: 0,
            index: 1,
            linear: false,
        }
    );
    apply(&mut fx.score, &command).unwrap();
    let mut ramp = second;
    ramp.linear = true;
    assert_eq!(fx.score.master_bars[0].tempo_changes, vec![first, ramp]);
}

#[test]
fn set_tempo_linear_needs_an_existing_change() {
    let mut fx = fixture();
    let far_bar = Command::SetTempoLinear {
        master_bar: 9,
        index: 0,
        linear: true,
    };
    assert_eq!(
        rejected(&mut fx.score, &far_bar),
        CommandError::IndexOutOfRange {
            what: "master bar",
            index: 9,
            len: 2
        }
    );
    fx.score.master_bars[0].tempo_changes.clear();
    let missing = Command::SetTempoLinear {
        master_bar: 0,
        index: 0,
        linear: true,
    };
    assert_eq!(
        rejected(&mut fx.score, &missing),
        CommandError::IndexOutOfRange {
            what: "tempo change",
            index: 0,
            len: 0
        }
    );
}

#[test]
fn set_tempo_linear_reads_from_json() {
    let command: Command = serde_json::from_str(
        r#"{"op": "set_tempo_linear", "master_bar": 1, "index": 0, "linear": true}"#,
    )
    .unwrap();
    assert_eq!(
        command,
        Command::SetTempoLinear {
            master_bar: 1,
            index: 0,
            linear: true,
        }
    );
    assert!(
        serde_json::from_str::<Command>(
            r#"{"op": "set_tempo_linear", "master_bar": 1, "index": 0, "linear": true, "x": 1}"#
        )
        .is_err()
    );
}

#[test]
fn tempo_commands_refuse_a_tempo_without_an_exact_value() {
    let mut fx = fixture();
    let seven = Command::SetTempo {
        master_bar: 0,
        bpm: 120.123_456_7,
    };
    match rejected(&mut fx.score, &seven) {
        CommandError::InvalidValue { message } => {
            assert!(message.contains("six decimals"), "{message}")
        }
        other => panic!("{other:?}"),
    }
    round_trip(
        &mut fx.score,
        &Command::SetTempo {
            master_bar: 0,
            bpm: 120.123_456,
        },
    );
    let restore = Command::RestoreTempoChanges {
        master_bar: 0,
        tempo_changes: vec![change(Fraction::new(1, 4), 128.488_062_080_115_01)],
    };
    assert!(matches!(
        rejected(&mut fx.score, &restore),
        CommandError::InvalidValue { .. }
    ));
}

#[test]
fn a_master_bar_holding_an_inexact_tempo_is_removed_and_restored() {
    let mut fx = fixture();
    fx.score.master_bars[1].tempo_changes = vec![
        change(Fraction::from_integer(0), 128.488_062_080_115_01),
        change(Fraction::new(1, 2), 120.123_456_7),
    ];
    assert!(
        fx.score.master_bars[1].tempo_changes[0]
            .bpm
            .exact()
            .is_none()
    );
    let original = fx.score.clone();
    let inverse = apply(&mut fx.score, &Command::RemoveMasterBar { index: 1 }).unwrap();
    assert_eq!(fx.score.master_bars.len(), 1);
    apply(&mut fx.score, &inverse).unwrap();
    assert_eq!(fx.score, original);
    let mut late = inverse.clone();
    let Command::RestoreMasterBar { master_bar, .. } = &mut late else {
        panic!("restore_master_bar expected");
    };
    master_bar.tempo_changes[1].position = Fraction::from_integer(1);
    apply(&mut fx.score, &Command::RemoveMasterBar { index: 1 }).unwrap();
    assert!(matches!(
        rejected(&mut fx.score, &late),
        CommandError::InvalidValue { .. }
    ));
}
