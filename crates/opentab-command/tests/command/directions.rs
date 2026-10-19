use opentab_command::{Command, CommandError, apply};
use opentab_model::{Diagnostic, DirectionJump, DirectionTarget, Directions, diagnose};
use serde_json::json;

use crate::checks::{rejected, round_trip};
use crate::support::fixture;

fn segno_al_coda() -> Directions {
    Directions {
        targets: vec![DirectionTarget::Segno, DirectionTarget::Coda],
        jumps: vec![DirectionJump::DaSegnoAlCoda, DirectionJump::DaCoda],
    }
}

#[test]
fn set_directions_replaces_them_and_returns_the_old_ones() {
    let mut fx = fixture();
    let command = Command::SetDirections {
        master_bar: 1,
        directions: segno_al_coda(),
    };
    let inverse = round_trip(&mut fx.score, &command);
    assert_eq!(
        inverse,
        Command::SetDirections {
            master_bar: 1,
            directions: Directions::default(),
        }
    );
    apply(&mut fx.score, &command).unwrap();
    assert_eq!(fx.score.master_bars[1].directions, segno_al_coda());
    assert!(fx.score.master_bars[0].directions.is_empty());
    let cleared = Command::SetDirections {
        master_bar: 1,
        directions: Directions::default(),
    };
    assert_eq!(
        round_trip(&mut fx.score, &cleared),
        Command::SetDirections {
            master_bar: 1,
            directions: segno_al_coda(),
        }
    );
}

#[test]
fn set_directions_refuses_a_missing_master_bar() {
    let mut fx = fixture();
    let command = Command::SetDirections {
        master_bar: 2,
        directions: segno_al_coda(),
    };
    assert_eq!(
        rejected(&mut fx.score, &command),
        CommandError::IndexOutOfRange {
            what: "master bar",
            index: 2,
            len: 2,
        }
    );
}

#[test]
fn dangling_directions_are_allowed_and_diagnosed() {
    let mut fx = fixture();
    let command = Command::SetDirections {
        master_bar: 0,
        directions: Directions {
            targets: Vec::new(),
            jumps: vec![DirectionJump::DaSegnoAlFine],
        },
    };
    round_trip(&mut fx.score, &command);
    apply(&mut fx.score, &command).unwrap();
    let id = fx.score.master_bars[0].id;
    assert_eq!(
        diagnose(&fx.score),
        vec![
            Diagnostic::DanglingJump {
                master_bar: id,
                jump: DirectionJump::DaSegnoAlFine,
                target: DirectionTarget::Segno,
            },
            Diagnostic::DanglingJump {
                master_bar: id,
                jump: DirectionJump::DaSegnoAlFine,
                target: DirectionTarget::Fine,
            },
        ]
    );
}

#[test]
fn removing_and_restoring_a_master_bar_keeps_its_directions() {
    let mut fx = fixture();
    fx.score.master_bars[1].directions = segno_al_coda();
    let before = fx.score.clone();
    let inverse = apply(&mut fx.score, &Command::RemoveMasterBar { index: 1 }).unwrap();
    let Command::RestoreMasterBar { master_bar, .. } = &inverse else {
        panic!("restore_master_bar expected");
    };
    assert_eq!(master_bar.directions, segno_al_coda());
    apply(&mut fx.score, &inverse).unwrap();
    assert_eq!(fx.score, before);
}

#[test]
fn inserted_master_bars_have_no_directions() {
    let mut fx = fixture();
    fx.score.master_bars[0].directions = segno_al_coda();
    let time_signature = fx.score.master_bars[0].time_signature;
    apply(
        &mut fx.score,
        &Command::InsertMasterBar {
            index: 1,
            time_signature,
        },
    )
    .unwrap();
    assert!(fx.score.master_bars[1].directions.is_empty());
}

#[test]
fn set_directions_has_the_documented_json_shape() {
    let command = Command::SetDirections {
        master_bar: 3,
        directions: segno_al_coda(),
    };
    let expected = json!({
        "op": "set_directions",
        "master_bar": 3,
        "directions": {
            "targets": ["segno", "coda"],
            "jumps": ["da_segno_al_coda", "da_coda"],
        },
    });
    assert_eq!(serde_json::to_value(&command).unwrap(), expected);
    assert_eq!(
        serde_json::from_value::<Command>(expected).unwrap(),
        command
    );
    let unknown = json!({
        "op": "set_directions",
        "master_bar": 0,
        "directions": {"targets": ["dal_niente"], "jumps": []},
    });
    assert!(serde_json::from_value::<Command>(unknown).is_err());
}
