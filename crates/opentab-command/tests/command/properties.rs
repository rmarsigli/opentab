use std::collections::BTreeMap;

use opentab_command::{Command, CommandError, History, apply};
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::TestRunner;

use crate::generator::{Case, arb_case, resolve};

const CASES: u32 = 256;
const COVERAGE_SAMPLES: usize = 256;

fn execute_all(case: &Case) -> (opentab_model::Score, History, usize) {
    let mut score = case.score.clone();
    let mut history = History::new();
    let mut executed = 0;
    for step in &case.steps {
        let command = resolve(step, &score);
        if history.execute(&mut score, command).is_ok() {
            executed += 1;
        }
    }
    (score, history, executed)
}

fn op_name(command: &Command) -> String {
    let value = serde_json::to_value(command).expect("commands serialise");
    value["op"].as_str().unwrap_or_default().to_owned()
}

fn error_kind(error: &CommandError) -> &'static str {
    match error {
        CommandError::NotFound { .. } => "not_found",
        CommandError::IndexOutOfRange { .. } => "index_out_of_range",
        CommandError::InvalidValue { .. } => "invalid_value",
        CommandError::WrongInstrument { .. } => "wrong_instrument",
    }
}

fn has_zero_term(command: &Command) -> bool {
    matches!(
        command,
        Command::InsertMasterBar { time_signature, .. }
            if time_signature.numerator == 0 || time_signature.denominator == 0
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn p3_undo_all_restores_the_original_score(case in arb_case()) {
        let (mut score, mut history, executed) = execute_all(&case);
        let mut undone = 0;
        while history.undo(&mut score).expect("undo never fails") {
            undone += 1;
        }
        prop_assert_eq!(undone, executed);
        prop_assert_eq!(&score, &case.score);
    }

    #[test]
    fn p3_raw_inverses_restore_the_original_score(case in arb_case()) {
        let mut score = case.score.clone();
        let mut inverses = Vec::new();
        for step in &case.steps {
            let command = resolve(step, &score);
            if let Ok(inverse) = apply(&mut score, &command) {
                inverses.push(inverse);
            }
        }
        for inverse in inverses.iter().rev() {
            apply(&mut score, inverse).expect("inverse applies");
        }
        prop_assert_eq!(&score, &case.score);
    }

    #[test]
    fn p4_redo_all_reproduces_the_edited_score(case in arb_case()) {
        let (mut score, mut history, executed) = execute_all(&case);
        let edited = score.clone();
        while history.undo(&mut score).expect("undo never fails") {}
        let mut redone = 0;
        while history.redo(&mut score).expect("redo never fails") {
            redone += 1;
        }
        prop_assert_eq!(redone, executed);
        prop_assert_eq!(&score, &edited);
        prop_assert!(!history.can_redo());
        prop_assert_eq!(history.can_undo(), executed > 0);
    }

    #[test]
    fn p5_a_failing_command_leaves_the_score_unchanged(case in arb_case()) {
        let mut score = case.score.clone();
        for step in &case.steps {
            let command = resolve(step, &score);
            let before = score.clone();
            if apply(&mut score, &command).is_err() {
                prop_assert_eq!(&score, &before);
            }
        }
    }

    #[test]
    fn commands_and_inverses_round_trip_through_json(case in arb_case()) {
        let mut score = case.score.clone();
        for step in &case.steps {
            let command = resolve(step, &score);
            let text = serde_json::to_string(&command).expect("serialises");
            match serde_json::from_str::<Command>(&text) {
                Ok(back) => prop_assert_eq!(&back, &command),
                Err(_) => prop_assert!(has_zero_term(&command), "{text}"),
            }
            if let Ok(inverse) = apply(&mut score, &command) {
                let text = serde_json::to_string(&inverse).expect("serialises");
                let back: Command = serde_json::from_str(&text).expect("parses");
                prop_assert_eq!(&back, &inverse);
            }
        }
    }
}

#[test]
fn the_sequence_generator_mixes_successes_and_every_failure() {
    let mut runner = TestRunner::deterministic();
    let strategy = arb_case();
    let mut successes: BTreeMap<String, usize> = BTreeMap::new();
    let mut failures: BTreeMap<String, usize> = BTreeMap::new();
    let mut kinds: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut lengths: BTreeMap<usize, usize> = BTreeMap::new();
    let mut staff_mismatch_inserts = 0;
    for _ in 0..COVERAGE_SAMPLES {
        let case = strategy.new_tree(&mut runner).expect("case").current();
        *lengths.entry(case.steps.len()).or_default() += 1;
        let mut score = case.score.clone();
        for step in &case.steps {
            let command = resolve(step, &score);
            let name = op_name(&command);
            match apply(&mut score, &command) {
                Ok(_) => *successes.entry(name).or_default() += 1,
                Err(error) => {
                    if name == "insert_master_bar"
                        && matches!(&error, CommandError::InvalidValue { message } if message.contains("bars but the score has"))
                    {
                        staff_mismatch_inserts += 1;
                    }
                    *kinds.entry(error_kind(&error)).or_default() += 1;
                    *failures.entry(name).or_default() += 1;
                }
            }
        }
    }
    let ops = [
        "set_meta",
        "insert_master_bar",
        "remove_master_bar",
        "insert_beat",
        "remove_beat",
        "set_beat_duration",
        "add_note",
        "remove_note",
        "set_fret",
        "set_tempo",
    ];
    for op in ops {
        assert!(
            successes.get(op).copied().unwrap_or(0) > 0,
            "{op} never succeeded: {successes:?}"
        );
        if op != "set_meta" {
            assert!(
                failures.get(op).copied().unwrap_or(0) > 0,
                "{op} never failed: {failures:?}"
            );
        }
    }
    for kind in [
        "not_found",
        "index_out_of_range",
        "invalid_value",
        "wrong_instrument",
    ] {
        assert!(
            kinds.get(kind).copied().unwrap_or(0) > 0,
            "{kind} never produced: {kinds:?}"
        );
    }
    assert!(
        staff_mismatch_inserts >= 5,
        "only {staff_mismatch_inserts} staff mismatch inserts"
    );
    assert!(lengths.contains_key(&0));
    assert!(lengths.contains_key(&20));
}
