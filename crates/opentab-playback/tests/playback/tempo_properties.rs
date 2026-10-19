use num_rational::Ratio;
use opentab_model::{Fraction, Score, TempoChange, TempoUnit, TimeSignature};
use opentab_playback::{
    PlayOrder, RampGrid, RampStep, ScoreTime, Seconds, Speed, TempoMap, play_order,
    quarters_per_minute,
};
use opentab_testkit::arb_score;
use proptest::prelude::*;
use proptest::sample::select;
use proptest::test_runner::FileFailurePersistence;

use crate::support::{master_bar, tempo};

type Wide = Ratio<i128>;

const RAMP_BARS: usize = 500;
const GRADUAL_BARS: usize = 1000;
const GRADUAL_CASES: u32 = 64;
const PRECISE_BARS: usize = 200;
const LONG_BARS: usize = 2000;
const LONG_CASES: u32 = 24;
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

fn check_monotonic(map: &TempoMap, speed: Speed, stride: usize) -> Result<(), TestCaseError> {
    let segments = map.segments();
    prop_assert_eq!(segments[0].start, ScoreTime::ZERO);
    for pair in segments.windows(2) {
        prop_assert!(pair[0].start < pair[1].start);
        prop_assert!(pair[0].quarters_per_minute != pair[1].quarters_per_minute);
        prop_assert!(pair[1].quarters_per_minute > Fraction::new(0, 1));
    }
    let mut previous: Option<Seconds> = None;
    for segment in segments.iter().step_by(stride.max(1)) {
        let seconds = match map.seconds_at(segment.start, speed) {
            Ok(seconds) => seconds,
            Err(error) => return Err(TestCaseError::fail(format!("{error:?}"))),
        };
        prop_assert_eq!(map.time_at(seconds, speed), Ok(segment.start));
        if let Some(previous) = previous {
            prop_assert!(previous < seconds);
        }
        previous = Some(seconds);
    }
    Ok(())
}

fn microsecond_tempo(quarters_per_minute: Fraction) -> Fraction {
    let micros = (Fraction::from_integer(60_000_000) / quarters_per_minute).round();
    Fraction::from_integer(60_000_000) / micros
}

fn whole_microseconds(map: &TempoMap) -> Result<(), TestCaseError> {
    for segment in map.segments() {
        let micros = Fraction::from_integer(60_000_000) / segment.quarters_per_minute;
        prop_assert!(micros.is_integer(), "{micros}");
    }
    Ok(())
}

fn build(score: &Score, order: &PlayOrder, grid: RampGrid) -> Result<TempoMap, TestCaseError> {
    TempoMap::build(score, order, grid).map_err(|error| TestCaseError::fail(format!("{error:?}")))
}

fn sampled_points(map: &TempoMap, order: &PlayOrder, speed: Speed) -> Result<(), TestCaseError> {
    let stride = (order.bars.len() / 8).max(1);
    for bar in order.bars.iter().step_by(stride) {
        check_point(map, bar.start.whole_notes(), speed)?;
        check_point(
            map,
            bar.start.whole_notes() + bar.length * Fraction::new(5, 7),
            speed,
        )?;
    }
    check_point(map, order.end().whole_notes(), speed)
}

fn arb_speed() -> impl Strategy<Value = Speed> {
    prop_oneof![Just(25u16), Just(100u16), Just(200u16), 25u16..=200]
        .prop_map(|percent| Speed::new(percent).expect("speed"))
}

fn arb_grid() -> impl Strategy<Value = RampGrid> {
    prop_oneof![
        Just(RampGrid::MUSICAL),
        Just(RampGrid::GP),
        (1i64..=8, select(vec![4i64, 8, 16, 32])).prop_map(|(numer, denom)| RampGrid::Step(
            RampStep::new(Fraction::new(numer, denom)).expect("step")
        )),
    ]
}

#[derive(Debug, Clone, Copy)]
struct Meter {
    numerator: u8,
    denominator: u8,
}

#[derive(Debug, Clone, Copy)]
struct Mark {
    bpm: u16,
    unit: TempoUnit,
}

fn arb_meter() -> impl Strategy<Value = Meter> {
    (1u8..=12, select(vec![1u8, 2, 4, 8, 16, 32, 64])).prop_map(|(numerator, denominator)| Meter {
        numerator,
        denominator,
    })
}

fn arb_mark() -> impl Strategy<Value = Mark> {
    (
        prop_oneof![Just(1u16), Just(1000u16), 1u16..=1000],
        select(UNITS.to_vec()),
    )
        .prop_map(|(bpm, unit)| Mark { bpm, unit })
}

fn metered_score(meters: &[Meter]) -> Score {
    let mut score = Score::new();
    score.master_bars = meters
        .iter()
        .enumerate()
        .map(|(index, meter)| {
            let mut bar = master_bar(index, 4, 4);
            bar.time_signature =
                TimeSignature::new(meter.numerator, meter.denominator).expect("signature");
            bar
        })
        .collect();
    score
}

#[derive(Debug, Clone)]
struct RampCase {
    meters: Vec<Meter>,
    first: Mark,
    last: Mark,
    start_position: i64,
    end_position: i64,
    prime: i64,
}

fn arb_ramp_case() -> impl Strategy<Value = RampCase> {
    (
        proptest::collection::vec(arb_meter(), RAMP_BARS),
        arb_mark(),
        arb_mark(),
        0..POSITION_GRID,
        0..POSITION_GRID,
        select(LARGE_PRIMES.to_vec()),
    )
        .prop_map(
            |(meters, first, last, start_position, end_position, prime)| RampCase {
                meters,
                first,
                last,
                start_position,
                end_position,
                prime,
            },
        )
}

fn ramp_change(position: i64, mark: Mark, length: Fraction, linear: bool) -> TempoChange {
    tempo(
        Fraction::new(position, POSITION_GRID) * length,
        f64::from(mark.bpm),
        mark.unit,
        linear,
    )
}

fn ramp_score(case: &RampCase) -> Score {
    let mut score = metered_score(&case.meters);
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

#[derive(Debug, Clone)]
struct GradualCase {
    meters: Vec<Meter>,
    from_hundredths: u32,
    to_hundredths: u32,
    unit: TempoUnit,
    linear: bool,
}

fn arb_gradual_case() -> impl Strategy<Value = GradualCase> {
    (
        proptest::collection::vec(arb_meter(), GRADUAL_BARS..=GRADUAL_BARS + 100),
        100u32..=100_000,
        100u32..=100_000,
        select(UNITS.to_vec()),
        any::<bool>(),
    )
        .prop_map(
            |(meters, from_hundredths, to_hundredths, unit, linear)| GradualCase {
                meters,
                from_hundredths,
                to_hundredths,
                unit,
                linear,
            },
        )
}

fn gradual_score(case: &GradualCase) -> Score {
    let mut score = metered_score(&case.meters);
    let last = i64::try_from(case.meters.len() - 1).expect("bars");
    let from = i64::from(case.from_hundredths);
    let to = i64::from(case.to_hundredths);
    for (index, bar) in score.master_bars.iter_mut().enumerate() {
        let index = i64::try_from(index).expect("bar index");
        let hundredths = from + (to - from) * index / last;
        let hundredths = u32::try_from(hundredths).expect("hundredths");
        bar.tempo_changes.push(tempo(
            Fraction::new(0, 1),
            f64::from(hundredths) / 100.0,
            case.unit,
            case.linear,
        ));
    }
    score
}

#[derive(Debug, Clone, Copy)]
struct PreciseChange {
    position: i64,
    millionths: u32,
    unit: TempoUnit,
    linear: bool,
}

#[derive(Debug, Clone)]
struct PreciseBar {
    meter: Meter,
    repeat_start: bool,
    repeat_end: Option<u8>,
    changes: Vec<PreciseChange>,
}

fn arb_precise_bar() -> impl Strategy<Value = PreciseBar> {
    let change = (
        0..POSITION_GRID,
        1_000_000u32..=1_000_000_000,
        select(UNITS.to_vec()),
        any::<bool>(),
    )
        .prop_map(|(position, millionths, unit, linear)| PreciseChange {
            position,
            millionths,
            unit,
            linear,
        });
    (
        arb_meter(),
        proptest::bool::weighted(0.1),
        proptest::option::weighted(0.1, 2u8..=3),
        proptest::collection::vec(change, 0..=3),
    )
        .prop_map(|(meter, repeat_start, repeat_end, changes)| PreciseBar {
            meter,
            repeat_start,
            repeat_end,
            changes,
        })
}

fn precise_score(bars: &[PreciseBar]) -> Score {
    let meters: Vec<Meter> = bars.iter().map(|bar| bar.meter).collect();
    let mut score = metered_score(&meters);
    for (master, seed) in score.master_bars.iter_mut().zip(bars) {
        master.repeat_start = seed.repeat_start;
        master.repeat_end = seed.repeat_end;
        let length = master.time_signature.bar_length();
        for change in &seed.changes {
            master.tempo_changes.push(tempo(
                Fraction::new(change.position, POSITION_GRID) * length,
                f64::from(change.millionths) / 1_000_000.0,
                change.unit,
                change.linear,
            ));
        }
    }
    score
}

#[derive(Debug, Clone)]
struct LongCase {
    bars: usize,
    section: usize,
    change_every: usize,
    thousandths: Vec<u32>,
    ramps: Vec<bool>,
}

fn arb_long_case() -> impl Strategy<Value = LongCase> {
    (
        LONG_BARS..=LONG_BARS + 1000,
        2usize..=16,
        1usize..=8,
        proptest::collection::vec(1_000u32..=1_000_000, 1..=64),
        proptest::collection::vec(any::<bool>(), 1..=64),
    )
        .prop_map(
            |(bars, section, change_every, thousandths, ramps)| LongCase {
                bars,
                section,
                change_every,
                thousandths,
                ramps,
            },
        )
}

fn long_score(case: &LongCase) -> Score {
    let mut score = metered_score(&vec![
        Meter {
            numerator: 4,
            denominator: 4,
        };
        case.bars
    ]);
    for (index, bar) in score.master_bars.iter_mut().enumerate() {
        bar.repeat_start = index % case.section == 0;
        bar.repeat_end = (index % case.section == case.section - 1).then_some(2);
        if index % case.change_every == 0 {
            let change = index / case.change_every;
            let thousandths = case.thousandths[change % case.thousandths.len()];
            bar.tempo_changes.push(tempo(
                Fraction::new(1, 3),
                f64::from(thousandths) / 1000.0,
                TempoUnit::Quarter,
                case.ramps[change % case.ramps.len()],
            ));
        }
    }
    score
}

fn heavy(cases: u32) -> ProptestConfig {
    ProptestConfig { cases, ..config() }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn the_tempo_map_of_any_score_builds_monotonic_and_exact(
        score in arb_score(),
        grid in arb_grid(),
        speed in arb_speed(),
    ) {
        let order = play_order(&score);
        let map = build(&score, &order, grid)?;
        prop_assert_eq!(&map, &build(&score, &order, grid)?);
        whole_microseconds(&map)?;
        for bar in order.bars.iter().take(MAX_SAMPLED_BARS) {
            let start = bar.start.whole_notes();
            check_point(&map, start, speed)?;
            check_point(&map, start + bar.length / 3, speed)?;
        }
        check_monotonic(&map, speed, map.segments().len() / MAX_SAMPLED_BARS)?;
    }

    #[test]
    fn a_ramp_across_500_bars_never_overflows_and_stays_monotonic(
        case in arb_ramp_case(),
        grid in arb_grid(),
        speed in arb_speed(),
    ) {
        let score = ramp_score(&case);
        let order = play_order(&score);
        let map = build(&score, &order, grid)?;
        check_monotonic(&map, speed, 1)?;
        check_point(&map, order.end().whole_notes(), speed)?;
        let late = order.bars[RAMP_BARS - 50].start.whole_notes();
        check_point(&map, late + Fraction::new(1, case.prime), speed)?;
        let first = quarters_per_minute(&ramp_change(0, case.first, Fraction::new(1, 1), true))
            .map(microsecond_tempo);
        prop_assert!(
            map.segments()
                .iter()
                .any(|segment| Some(segment.quarters_per_minute) == first)
        );
    }

    #[test]
    fn random_tempi_with_six_decimals_build_monotonic_and_exact(
        bars in proptest::collection::vec(arb_precise_bar(), 1..=PRECISE_BARS),
        grid in arb_grid(),
        speed in arb_speed(),
    ) {
        let score = precise_score(&bars);
        let order = play_order(&score);
        let map = build(&score, &order, grid)?;
        whole_microseconds(&map)?;
        check_monotonic(&map, speed, map.segments().len() / MAX_SAMPLED_BARS)?;
        sampled_points(&map, &order, speed)?;
    }
}

proptest! {
    #![proptest_config(heavy(GRADUAL_CASES))]

    #[test]
    fn a_thousand_bar_ritardando_or_accelerando_builds_monotonic_and_exact(
        case in arb_gradual_case(),
        grid in arb_grid(),
        speed in arb_speed(),
    ) {
        let score = gradual_score(&case);
        let order = play_order(&score);
        let map = build(&score, &order, grid)?;
        prop_assert!(order.bars.len() >= GRADUAL_BARS);
        check_monotonic(&map, speed, map.segments().len() / MAX_SAMPLED_BARS)?;
        sampled_points(&map, &order, speed)?;
    }
}

proptest! {
    #![proptest_config(heavy(LONG_CASES))]

    #[test]
    fn a_long_score_with_repeats_builds_monotonic_and_exact(
        case in arb_long_case(),
        grid in arb_grid(),
        speed in arb_speed(),
    ) {
        let score = long_score(&case);
        let order = play_order(&score);
        prop_assert!(!order.truncated);
        let map = build(&score, &order, grid)?;
        check_monotonic(&map, speed, map.segments().len() / MAX_SAMPLED_BARS)?;
        sampled_points(&map, &order, speed)?;
    }
}

#[test]
fn every_generated_score_builds_a_tempo_map() {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    let mut runner = TestRunner::deterministic();
    let mut ramps = 0;
    for _ in 0..128 {
        let score = arb_score().new_tree(&mut runner).expect("tree").current();
        let order = play_order(&score);
        for grid in [RampGrid::MUSICAL, RampGrid::GP] {
            if let Err(error) = TempoMap::build(&score, &order, grid) {
                panic!("{error:?}");
            }
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
    assert!(ramps > 16);
}
