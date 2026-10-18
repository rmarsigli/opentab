use opentab_command::{Command, CommandError, apply};
use opentab_model::{IdAllocator, Score};

pub fn without_ids(score: &Score) -> Score {
    let mut copy = score.clone();
    copy.ids = IdAllocator::default();
    copy
}

pub fn restored(score: &Score, original: &Score) {
    assert_eq!(
        without_ids(score),
        without_ids(original),
        "inverse restores the score"
    );
    assert!(
        score.ids.peek() >= original.ids.peek(),
        "the allocator never moves backwards"
    );
}

pub fn round_trip(score: &mut Score, command: &Command) -> Command {
    let before = score.clone();
    let inverse = apply(score, command).expect("command applies");
    let after = score.clone();
    let forward = apply(score, &inverse).expect("inverse applies");
    restored(score, &before);
    assert_eq!(score.ids, after.ids, "undo keeps the allocator");
    let again = apply(score, &forward).expect("redo applies");
    assert_eq!(*score, after, "redo reproduces the edit");
    assert_eq!(again, inverse, "redo yields the same inverse");
    apply(score, &again).expect("second undo applies");
    restored(score, &before);
    *score = before;
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
