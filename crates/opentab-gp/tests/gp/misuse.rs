use std::path::Path;

use opentab_command::{Command, CommandError, History, apply, parse_jsonl};
use opentab_gp::{read_gp, write_gp};
use opentab_model::{AutomationValue, Fraction, Score, diagnose};

const BAD_RESTORE: &str = include_str!("misuse/bad.jsonl");
const BAD_OVERRIDES: &str = include_str!("misuse/badov.jsonl");
const OFF_GRID: &str = include_str!("misuse/grid.jsonl");

fn sound_change() -> Score {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/gp/sound-change.gp");
    read_gp(&std::fs::read(path).expect("fixture")).expect("reads")
}

fn refused(score: &mut Score, command: &Command) -> CommandError {
    let before = score.clone();
    let error = apply(score, command).expect_err("the command is refused");
    assert_eq!(*score, before);
    assert_eq!(diagnose(score), Vec::new());
    assert!(write_gp(score).is_ok());
    error
}

#[test]
fn a_restore_master_bar_with_a_foreign_automation_is_refused() {
    let commands = parse_jsonl(BAD_RESTORE).expect("parses");
    let [remove, restore] = commands.as_slice() else {
        panic!("two commands expected");
    };
    let mut score = sound_change();
    let original = score.clone();
    let inverse = apply(&mut score, remove).expect("the bar is removed");
    refused(&mut score, restore);
    let Command::RestoreMasterBar { automations, .. } = &inverse else {
        panic!("restore_master_bar expected");
    };
    assert_eq!(automations.len(), 1);
    for position in [Fraction::new(1, 7), Fraction::from_integer(2)] {
        let mut broken = inverse.clone();
        if let Command::RestoreMasterBar { automations, .. } = &mut broken {
            automations[0].automation.position = position;
            if let AutomationValue::Sound(reference) = &mut automations[0].automation.value {
                reference.0 = 0;
            }
        }
        refused(&mut score, &broken);
    }
    apply(&mut score, &inverse).expect("the true inverse restores");
    assert_eq!(score, original);
}

#[test]
fn restore_sound_overrides_refuses_unknown_and_duplicate_tracks() {
    let commands = parse_jsonl(BAD_OVERRIDES).expect("parses");
    let mut score = sound_change();
    for command in &commands {
        refused(&mut score, command);
    }
    let track = score.tracks[0].id;
    let mut history = History::new();
    history
        .execute(
            &mut score,
            Command::SetSoundOverride {
                track,
                bank: Some("a".to_owned()),
            },
        )
        .expect("set");
    let duplicate = Command::RestoreSoundOverrides {
        sound_overrides: vec![score.sound_overrides[0].clone(); 2],
    };
    refused(&mut score, &duplicate);
    assert!(history.undo(&mut score).expect("undo"));
    assert!(score.sound_overrides.is_empty());
}

#[test]
fn an_off_grid_automation_is_refused_so_diagnose_clean_stays_writable() {
    let commands = parse_jsonl(OFF_GRID).expect("parses");
    let mut score = sound_change();
    for command in &commands {
        let CommandError::InvalidValue { message } = refused(&mut score, command) else {
            panic!("invalid value expected");
        };
        assert!(message.contains("grid"), "{message}");
    }
}
