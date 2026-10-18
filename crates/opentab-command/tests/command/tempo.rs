use opentab_command::{Command, CommandError, apply};
use opentab_model::{Fraction, Tempo, TempoChange};

use crate::checks::{rejected, round_trip};
use crate::support::fixture;

fn change(position: Fraction, bpm: f64) -> TempoChange {
    TempoChange {
        position,
        bpm: Tempo::new(bpm).unwrap(),
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
