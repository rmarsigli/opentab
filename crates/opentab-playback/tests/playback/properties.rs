use std::collections::BTreeSet;

use opentab_model::{
    DIRECTION_JUMPS, DIRECTION_TARGETS, Directions, MasterBar, Score, TimeSignature,
};
use opentab_playback::{MAX_PLAYED_BARS, play_order};
use opentab_testkit::arb_score;
use proptest::prelude::*;
use proptest::sample::select;
use proptest::test_runner::FileFailurePersistence;

use crate::reference::reference_order;
use crate::support::master_bar;

const MAX_BARS: usize = 10;
const MAX_TARGETS: usize = 2;
const MAX_JUMPS: usize = 2;
const MAX_COUNT: u8 = 4;
const MAX_ENDING_MASK: u8 = 0b111;

fn config() -> ProptestConfig {
    ProptestConfig {
        failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
        ..ProptestConfig::default()
    }
}

fn expansion_bound(bars: &[MasterBar]) -> usize {
    let names: BTreeSet<_> = bars
        .iter()
        .flat_map(|bar| bar.directions.jumps.iter().copied())
        .collect();
    let most = bars
        .iter()
        .filter_map(|bar| bar.repeat_end)
        .max()
        .map_or(1, usize::from);
    (names.len() + 1) * bars.len() * (most + 1)
}

fn check(score: &Score) -> Result<(), TestCaseError> {
    let order = play_order(score);
    prop_assert_eq!(&order, &play_order(score));
    let bound = expansion_bound(&score.master_bars);
    prop_assert!(order.bars.len() <= bound.min(MAX_PLAYED_BARS));
    let reference = reference_order(&score.master_bars, MAX_PLAYED_BARS);
    let ours: Vec<(usize, u32)> = order
        .bars
        .iter()
        .map(|bar| (bar.source, u32::from(bar.pass)))
        .collect();
    let theirs: Vec<(usize, u32)> = reference
        .iter()
        .map(|visit| (visit.bar, visit.pass))
        .collect();
    prop_assert_eq!(ours, theirs);
    let mut end = opentab_model::Fraction::new(0, 1);
    for bar in &order.bars {
        prop_assert_eq!(bar.start.whole_notes(), end);
        prop_assert_eq!(
            bar.length,
            score.master_bars[bar.source].time_signature.bar_length()
        );
        end += bar.length;
    }
    Ok(())
}

fn arb_directions() -> impl Strategy<Value = Directions> {
    (
        proptest::collection::vec(select(DIRECTION_TARGETS.to_vec()), 0..=MAX_TARGETS),
        proptest::collection::vec(select(DIRECTION_JUMPS.to_vec()), 0..=MAX_JUMPS),
    )
        .prop_map(|(targets, jumps)| Directions { targets, jumps })
}

fn arb_bar() -> impl Strategy<Value = (bool, Option<u8>, u8, Directions, (u8, u8))> {
    (
        proptest::bool::weighted(0.3),
        proptest::option::weighted(0.3, 0..=MAX_COUNT),
        prop_oneof![3 => Just(0u8), 1 => 1..=MAX_ENDING_MASK],
        prop_oneof![1 => Just(Directions::default()), 1 => arb_directions()],
        (1u8..=7, select(vec![2u8, 4, 8, 16])),
    )
}

fn arb_structure() -> impl Strategy<Value = Score> {
    proptest::collection::vec(arb_bar(), 0..=MAX_BARS).prop_map(|bars| {
        let mut score = Score::new();
        score.master_bars = bars
            .into_iter()
            .enumerate()
            .map(
                |(index, (start, end, endings, directions, (numer, denom)))| {
                    let mut bar = master_bar(index, 4, 4);
                    bar.time_signature = TimeSignature::new(numer, denom).expect("signature");
                    bar.repeat_start = start;
                    bar.repeat_end = end;
                    bar.alternate_endings = endings;
                    bar.directions = directions;
                    bar
                },
            )
            .collect();
        score
    })
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn the_order_of_any_score_matches_the_reference_interpreter(score in arb_score()) {
        check(&score)?;
    }

    #[test]
    fn the_order_of_dense_repeats_endings_and_directions_matches_the_reference(
        score in arb_structure()
    ) {
        check(&score)?;
    }
}

#[derive(Debug, Default)]
struct Coverage {
    backward: usize,
    forward: usize,
    early_end: usize,
    held_coda: usize,
}

#[test]
fn the_dense_generator_reaches_every_kind_of_jump() {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    let mut runner = TestRunner::deterministic();
    let mut coverage = Coverage::default();
    for _ in 0..512 {
        let score = arb_structure()
            .new_tree(&mut runner)
            .expect("tree")
            .current();
        let sources = play_order(&score).sources();
        let bars = score.master_bars.len();
        if sources
            .windows(2)
            .any(|pair| pair[1] < pair[0] && pair[1] != 0)
        {
            coverage.backward += 1;
        }
        if sources.windows(2).any(|pair| pair[1] > pair[0] + 1) {
            coverage.forward += 1;
        }
        if sources.last().is_some_and(|last| last + 1 < bars) {
            coverage.early_end += 1;
        }
        let coda_bars: Vec<usize> = score
            .master_bars
            .iter()
            .enumerate()
            .filter(|(_, bar)| {
                bar.directions
                    .jumps
                    .contains(&opentab_model::DirectionJump::DaCoda)
            })
            .map(|(index, _)| index)
            .collect();
        let visited_twice = coda_bars
            .iter()
            .any(|bar| sources.iter().filter(|source| *source == bar).count() > 1);
        if visited_twice && sources.windows(2).any(|pair| pair[1] > pair[0] + 1) {
            coverage.held_coda += 1;
        }
    }
    assert!(coverage.backward > 50, "{coverage:?}");
    assert!(coverage.forward > 50, "{coverage:?}");
    assert!(coverage.early_end > 20, "{coverage:?}");
    assert!(coverage.held_coda > 5, "{coverage:?}");
    eprintln!("{coverage:?}");
}
