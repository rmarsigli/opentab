use num_rational::Ratio;
use opentab_model::{Fraction, Score, TempoChange, TempoUnit, TimeSignature};
use opentab_playback::{
    PlaybackError, RampGrid, ScoreTime, Seconds, Speed, TempoMap, play_order, quarters_per_minute,
};
use opentab_testkit::arb_score;
use proptest::prelude::*;
use proptest::sample::select;
use proptest::test_runner::FileFailurePersistence;

use crate::support::{master_bar, tempo};

type Wide = Ratio<i128>;

const RAMP_BARS: usize = 500;
const MAX_SAMPLED_BARS: usize = 64;
const POSITION_GRID: i64 = 3840;
const TICKS: i64 = 3840;
const LARGE_PRIMES: [i64; 3] = [999_999_999_989, 2_147_483_647, 1_000_000_007];
const UNITS: [TempoUnit; 5] = [
    TempoUnit::Eighth,
    TempoUnit::Quarter,
    TempoUnit::DottedQuarter,
    TempoUnit::Half,
    TempoUnit::DottedHalf,
];

fn config() -> ProptestConfig {
    ProptestConfig {
        failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
        ..ProptestConfig::default()
    }
}

fn wide(value: Fraction) -> Wide {
    Wide::new(i128::from(*value.numer()), i128::from(*value.denom()))
}

fn oracle_seconds(map: &TempoMap, t: Fraction, speed: Speed) -> Wide {
    let segments = map.segments();
    let mut total = Wide::from_integer(0);
    for (index, segment) in segments.iter().enumerate() {
        let start = segment.start.whole_notes();
        if start > t {
            break;
        }
        let end = segments
            .get(index + 1)
            .map(|next| next.start.whole_notes())
            .filter(|next| *next <= t)
            .unwrap_or(t);
        total +=
            (wide(end) - wide(start)) * Wide::from_integer(240) / wide(segment.quarters_per_minute);
    }
    total * Wide::from_integer(100) / Wide::from_integer(i128::from(speed.percent()))
}

fn check_point(map: &TempoMap, t: Fraction, speed: Speed) -> Result<(), TestCaseError> {
    let time = ScoreTime::new(t).expect("non-negative");
    let seconds = match map.seconds_at(time, speed) {
        Ok(seconds) => seconds,
        Err(error) => return Err(TestCaseError::fail(format!("{error:?} at {t}"))),
    };
    let expected = oracle_seconds(map, t, speed);
    prop_assert_eq!(Wide::new(seconds.numer(), seconds.denom()), expected);
    prop_assert_eq!(map.time_at(seconds, speed), Ok(time));
    let ticks = (wide(t) * Wide::from_integer(i128::from(TICKS))).floor();
    let floored = map.time_at_floor(seconds, speed, Fraction::new(1, TICKS));
    prop_assert_eq!(
        floored.map(|time| wide(time.whole_notes())),
        Ok(ticks / Wide::from_integer(i128::from(TICKS)))
    );
    Ok(())
}

fn check_monotonic(map: &TempoMap, speed: Speed) -> Result<(), TestCaseError> {
    let segments = map.segments();
    prop_assert_eq!(segments[0].start, ScoreTime::ZERO);
    let mut previous: Option<Seconds> = None;
    for pair in segments.windows(2) {
        prop_assert!(pair[0].start < pair[1].start);
        prop_assert!(pair[0].quarters_per_minute != pair[1].quarters_per_minute);
    }
    for segment in segments {
        prop_assert!(segment.quarters_per_minute > Fraction::new(0, 1));
        let seconds = map.seconds_at(segment.start, speed);
        prop_assert!(seconds.is_ok(), "{seconds:?}");
        let seconds = seconds.unwrap_or(Seconds::ZERO);
        prop_assert_eq!(map.time_at(seconds, speed), Ok(segment.start));
        if let Some(previous) = previous {
            prop_assert!(previous < seconds);
        }
        previous = Some(seconds);
    }
    Ok(())
}

fn arb_speed() -> impl Strategy<Value = Speed> {
    prop_oneof![Just(25u16), Just(100u16), Just(200u16), 25u16..=200]
        .prop_map(|percent| Speed::new(percent).expect("speed"))
}

fn arb_grid() -> impl Strategy<Value = RampGrid> {
    prop_oneof![Just(RampGrid::MUSICAL), Just(RampGrid::GP)]
}

#[derive(Debug, Clone)]
struct RampCase {
    signatures: Vec<(u8, u8)>,
    first: (u16, TempoUnit),
    last: (u16, TempoUnit),
    start_position: i64,
    end_position: i64,
    prime: i64,
}

fn arb_ramp_case() -> impl Strategy<Value = RampCase> {
    (
        proptest::collection::vec(
            (1u8..=12, select(vec![1u8, 2, 4, 8, 16, 32, 64])),
            RAMP_BARS,
        ),
        (
            prop_oneof![Just(1u16), Just(1000u16), 1u16..=1000],
            select(UNITS.to_vec()),
        ),
        (
            prop_oneof![Just(1u16), Just(1000u16), 1u16..=1000],
            select(UNITS.to_vec()),
        ),
        0..POSITION_GRID,
        0..POSITION_GRID,
        select(LARGE_PRIMES.to_vec()),
    )
        .prop_map(
            |(signatures, first, last, start_position, end_position, prime)| RampCase {
                signatures,
                first,
                last,
                start_position,
                end_position,
                prime,
            },
        )
}

fn ramp_change(
    position: i64,
    (bpm, unit): (u16, TempoUnit),
    length: Fraction,
    linear: bool,
) -> TempoChange {
    tempo(
        Fraction::new(position, POSITION_GRID) * length,
        f64::from(bpm),
        unit,
        linear,
    )
}

fn ramp_score(case: &RampCase) -> Score {
    let mut score = Score::new();
    score.master_bars = case
        .signatures
        .iter()
        .enumerate()
        .map(|(index, (numer, denom))| {
            let mut bar = master_bar(index, 4, 4);
            bar.time_signature = TimeSignature::new(*numer, *denom).expect("signature");
            bar
        })
        .collect();
    let first = score.master_bars[0].time_signature.bar_length();
    score.master_bars[0].tempo_changes.push(ramp_change(
        case.start_position,
        case.first,
        first,
        true,
    ));
    let last = RAMP_BARS - 1;
    let length = score.master_bars[last].time_signature.bar_length();
    score.master_bars[last].tempo_changes.push(ramp_change(
        case.end_position,
        case.last,
        length,
        false,
    ));
    score
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn the_tempo_map_of_any_score_is_monotonic_and_exact_or_overflows(
        score in arb_score(),
        grid in arb_grid(),
        speed in arb_speed(),
    ) {
        let order = play_order(&score);
        let built = TempoMap::build(&score, &order, grid);
        prop_assert_eq!(&built, &TempoMap::build(&score, &order, grid));
        let map = match built {
            Ok(map) => map,
            Err(error) => {
                prop_assert_eq!(error, PlaybackError::TimeOverflow);
                return Ok(());
            }
        };
        for bar in order.bars.iter().take(MAX_SAMPLED_BARS) {
            let start = bar.start.whole_notes();
            check_point(&map, start, speed)?;
            check_point(&map, start + bar.length / 3, speed)?;
        }
        if map.segments().len() <= MAX_SAMPLED_BARS * MAX_SAMPLED_BARS {
            check_monotonic(&map, speed)?;
        }
    }

    #[test]
    fn a_ramp_across_500_bars_never_overflows_and_stays_monotonic(
        case in arb_ramp_case(),
        grid in arb_grid(),
        speed in arb_speed(),
    ) {
        let score = ramp_score(&case);
        let order = play_order(&score);
        let map = match TempoMap::build(&score, &order, grid) {
            Ok(map) => map,
            Err(error) => return Err(TestCaseError::fail(format!("{error:?}"))),
        };
        check_monotonic(&map, speed)?;
        check_point(&map, order.end().whole_notes(), speed)?;
        let late = order.bars[RAMP_BARS - 50].start.whole_notes();
        check_point(&map, late + Fraction::new(1, case.prime), speed)?;
        let first = quarters_per_minute(&ramp_change(0, case.first, Fraction::new(1, 1), true));
        prop_assert!(
            map.segments()
                .iter()
                .any(|segment| Some(segment.quarters_per_minute) == first)
        );
    }
}

#[test]
fn most_generated_scores_build_a_tempo_map() {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    let mut runner = TestRunner::deterministic();
    let (mut built, mut ramps, mut overflowed) = (0, 0, 0);
    for _ in 0..128 {
        let score = arb_score().new_tree(&mut runner).expect("tree").current();
        let order = play_order(&score);
        match TempoMap::build(&score, &order, RampGrid::MUSICAL) {
            Ok(_) => built += 1,
            Err(PlaybackError::TimeOverflow) => overflowed += 1,
            Err(error) => panic!("{error:?}"),
        }
        let linear = score
            .master_bars
            .iter()
            .flat_map(|bar| bar.tempo_changes.iter())
            .filter(|change| change.linear)
            .count();
        if linear > 0 {
            ramps += 1;
        }
    }
    eprintln!("built {built}, overflowed {overflowed}, with ramps {ramps}");
    assert!(built >= 120, "built {built}, overflowed {overflowed}");
    assert!(ramps > 16);
}
