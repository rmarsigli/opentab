use opentab_command::{Command, CommandError, apply};
use opentab_model::Score;

pub fn round_trip(score: &mut Score, command: &Command) -> Command {
    let before = score.clone();
    let inverse = apply(score, command).expect("command applies");
    let after = score.clone();
    let forward = apply(score, &inverse).expect("inverse applies");
    assert_eq!(*score, before, "inverse restores the score");
    let again = apply(score, &forward).expect("redo applies");
    assert_eq!(*score, after, "redo reproduces the edit");
    assert_eq!(again, inverse, "redo yields the same inverse");
    apply(score, &again).expect("second undo applies");
    assert_eq!(*score, before, "second undo restores the score");
    inverse
}

pub fn rejected(score: &mut Score, command: &Command) -> CommandError {
    let before = score.clone();
    let error = apply(score, command).expect_err("command is rejected");
    assert_eq!(
        *score, before,
        "a failing command leaves the score unchanged"
    );
    error
}
