use std::collections::BTreeMap;

use opentab_command::{Command, CommandError, History, apply};
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::TestRunner;

use crate::checks::without_ids;
use crate::generator::{CORRUPTIONS, Case, Corruption, arb_case, resolve};

const CASES: u32 = 256;
const COVERAGE_SAMPLES: usize = 256;
const SHIFTS: [usize; 4] = [0, 1, 2, usize::MAX];
const MIN_SUCCESS_RATIO: f64 = 0.2;
const FORWARD_OPS: [&str; 14] = [
    "set_meta",
    "set_anacrusis",
    "insert_master_bar",
    "remove_master_bar",
    "insert_beat",
    "remove_beat",
    "remove_voice",
    "set_beat_duration",
    "add_note",
    "remove_note",
    "set_fret",
    "set_tempo",
    "pad_staff_bars",
    "trim_staff_bars",
];

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

fn undo_all(
    history: &mut History,
    score: &mut opentab_model::Score,
    executed: usize,
) -> Result<(), TestCaseError> {
    for _ in 0..executed {
        prop_assert!(history.undo(score).expect("undo never fails"));
    }
    prop_assert!(!history.undo(score).expect("undo never fails"));
    Ok(())
}

fn redo_all(
    history: &mut History,
    score: &mut opentab_model::Score,
    executed: usize,
) -> Result<(), TestCaseError> {
    for _ in 0..executed {
        prop_assert!(history.redo(score).expect("redo never fails"));
    }
    prop_assert!(!history.redo(score).expect("redo never fails"));
    Ok(())
}

#[derive(Default)]
struct RestoreTally {
    applied: BTreeMap<String, usize>,
    rejected: BTreeMap<String, usize>,
}

fn is_restore(command: &Command) -> bool {
    matches!(
        command,
        Command::RestoreMasterBar { .. }
            | Command::RestoreBeat { .. }
            | Command::RestoreVoice { .. }
            | Command::RestoreNote { .. }
            | Command::RestoreTempoChanges { .. }
            | Command::RestoreStaffBars { .. }
    )
}

fn shifted(command: &Command, shift: usize) -> Command {
    let mut moved = command.clone();
    match &mut moved {
        Command::RestoreMasterBar { index, .. }
        | Command::RestoreBeat { index, .. }
        | Command::RestoreVoice { index, .. }
        | Command::RestoreNote { index, .. } => *index = index.saturating_add(shift),
        Command::RestoreTempoChanges { master_bar, .. } => {
            *master_bar = master_bar.saturating_add(shift);
        }
        _ => {}
    }
    moved
}

fn collect_inverses(case: &Case) -> (opentab_model::Score, Vec<Command>) {
    let mut score = case.score.clone();
    let mut inverses = Vec::new();
    for step in &case.steps {
        let command = resolve(step, &score);
        if let Ok(inverse) = apply(&mut score, &command) {
            inverses.push(inverse);
        }
    }
    (score, inverses)
}

fn replay_restores(first: &Case, second: &Case) -> Result<RestoreTally, TestCaseError> {
    let (edited, inverses) = collect_inverses(first);
    let restores: Vec<Command> = inverses.into_iter().filter(is_restore).collect();
    let mut variants = Vec::new();
    for restore in &restores {
        for shift in SHIFTS {
            variants.push(shifted(restore, shift));
        }
    }
    let mut tally = RestoreTally::default();
    for target in [&second.score, &edited, &first.score] {
        let mut score = target.clone();
        for command in &variants {
            let before = score.clone();
            let name = op_name(command);
            match apply(&mut score, command) {
                Ok(_) => *tally.applied.entry(name).or_default() += 1,
                Err(_) => {
                    prop_assert_eq!(&score, &before);
                    *tally.rejected.entry(name).or_default() += 1;
                }
            }
        }
    }
    Ok(tally)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn p5_restores_on_foreign_or_shifted_scores_never_panic_and_stay_atomic(
        first in arb_case(),
        second in arb_case(),
    ) {
        replay_restores(&first, &second)?;
    }

    #[test]
    fn p3_undo_all_restores_the_original_score(case in arb_case()) {
        let (mut score, mut history, executed) = execute_all(&case);
        undo_all(&mut history, &mut score, executed)?;
        prop_assert_eq!(without_ids(&score), without_ids(&case.score));
        prop_assert!(score.ids.peek() >= case.score.ids.peek());
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
        prop_assert_eq!(without_ids(&score), without_ids(&case.score));
        prop_assert!(score.ids.peek() >= case.score.ids.peek());
    }

    #[test]
    fn p4_redo_all_reproduces_the_edited_score(case in arb_case()) {
        let (mut score, mut history, executed) = execute_all(&case);
        let edited = score.clone();
        undo_all(&mut history, &mut score, executed)?;
        redo_all(&mut history, &mut score, executed)?;
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
            if let Err(error) = apply(&mut score, &command) {
                prop_assert_eq!(&score, &before);
                if let CommandError::InvalidValue { message } = &error
                    && message.contains("run diagnose")
                {
                    prop_assert!(
                        !opentab_model::diagnose(&before).is_empty(),
                        "{command:?} refused with {message:?} but diagnose reports nothing"
                    );
                }
            }
        }
    }

    #[test]
    fn commands_and_inverses_round_trip_through_json(case in arb_case()) {
        let mut score = case.score.clone();
        for step in &case.steps {
            let command = resolve(step, &score);
            let text = serde_json::to_string(&command).expect("serialises");
            let back: Command = serde_json::from_str(&text).expect("parses");
            prop_assert_eq!(&back, &command);
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
    let mut corruptions: BTreeMap<Corruption, usize> = BTreeMap::new();
    for _ in 0..COVERAGE_SAMPLES {
        let case = strategy.new_tree(&mut runner).expect("case").current();
        *lengths.entry(case.steps.len()).or_default() += 1;
        if let Some(corruption) = case.corruption {
            *corruptions.entry(corruption).or_default() += 1;
        }
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
    let mut rare = Vec::new();
    for op in FORWARD_OPS {
        let succeeded = successes.get(op).copied().unwrap_or(0);
        let failed = failures.get(op).copied().unwrap_or(0);
        let attempts = succeeded + failed;
        if (succeeded as f64) < MIN_SUCCESS_RATIO * attempts.max(1) as f64 {
            rare.push(format!("{op}: {succeeded} of {attempts}"));
        }
        if op != "set_meta" && op != "set_anacrusis" {
            assert!(failed > 0, "{op} never failed: {failures:?}");
        }
    }
    assert!(rare.is_empty(), "success ratio below 20%: {rare:?}");
    for corruption in CORRUPTIONS {
        assert!(
            corruptions.get(&corruption).copied().unwrap_or(0) >= 3,
            "{corruption:?} rarely generated: {corruptions:?}"
        );
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

#[test]
fn restore_replay_exercises_applied_and_rejected_restores_of_every_kind() {
    let mut runner = TestRunner::deterministic();
    let strategy = (arb_case(), arb_case());
    let mut total = RestoreTally::default();
    for _ in 0..COVERAGE_SAMPLES {
        let (first, second) = strategy.new_tree(&mut runner).expect("pair").current();
        let tally = replay_restores(&first, &second).expect("replay holds");
        for (name, count) in tally.applied {
            *total.applied.entry(name).or_default() += count;
        }
        for (name, count) in tally.rejected {
            *total.rejected.entry(name).or_default() += count;
        }
    }
    for op in [
        "restore_master_bar",
        "restore_beat",
        "restore_voice",
        "restore_note",
        "restore_tempo_changes",
        "restore_staff_bars",
    ] {
        assert!(
            total.applied.get(op).copied().unwrap_or(0) > 0,
            "{op} never applied: {:?}",
            total.applied
        );
        assert!(
            total.rejected.get(op).copied().unwrap_or(0) > 0,
            "{op} never rejected: {:?}",
            total.rejected
        );
    }
}
