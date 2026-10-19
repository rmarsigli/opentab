use opentab_model::{Fraction, Score, Tempo, TempoUnit};
use opentab_playback::{
    PlaybackError, RampGrid, RampStep, ScoreTime, Seconds, Speed, TempoMap, TempoSegment,
    play_order, playback_bpm, quarters_per_minute, quarters_per_unit,
};

use crate::support::{frac, plain_score, score_from, tempo};

fn map(score: &Score, grid: RampGrid) -> TempoMap {
    TempoMap::build(score, &play_order(score), grid).expect("tempo map")
}

fn at(whole_notes: Fraction) -> ScoreTime {
    ScoreTime::new(whole_notes).expect("time")
}

fn seconds(map: &TempoMap, whole_notes: Fraction, percent: u16) -> Fraction {
    map.seconds_at(at(whole_notes), Speed::new(percent).expect("speed"))
        .and_then(Seconds::fraction)
        .expect("seconds")
}

fn segment(start: Fraction, quarters_per_minute: Fraction) -> TempoSegment {
    TempoSegment {
        start: at(start),
        quarters_per_minute,
    }
}

#[test]
fn a_score_without_tempo_automation_plays_at_120() {
    let score = plain_score(2);
    let map = map(&score, RampGrid::MUSICAL);
    assert_eq!(map.segments(), [segment(frac(0, 1), frac(120, 1))]);
    assert_eq!(seconds(&map, frac(1, 1), 100), frac(2, 1));
    assert_eq!(seconds(&map, frac(2, 1), 100), frac(4, 1));
    let empty = map_of_empty();
    assert_eq!(empty.segments(), [segment(frac(0, 1), frac(120, 1))]);
}

fn map_of_empty() -> TempoMap {
    let score = Score::new();
    TempoMap::build(&score, &play_order(&score), RampGrid::GP).expect("empty map")
}

#[test]
fn a_tempo_change_at_the_bar_end_applies_from_the_next_bar() {
    let mut score = plain_score(3);
    score.master_bars[0]
        .tempo_changes
        .push(tempo(frac(1, 1), 60.0, TempoUnit::Quarter, false));
    score.master_bars[1]
        .tempo_changes
        .push(tempo(frac(3, 2), 240.0, TempoUnit::Quarter, false));
    let map = map(&score, RampGrid::MUSICAL);
    assert_eq!(
        map.segments(),
        [
            segment(frac(0, 1), frac(120, 1)),
            segment(frac(1, 1), frac(60, 1)),
            segment(frac(2, 1), frac(240, 1))
        ]
    );
    assert_eq!(seconds(&map, frac(1, 1), 100), frac(2, 1));
    assert_eq!(seconds(&map, frac(2, 1), 100), frac(6, 1));
    assert_eq!(seconds(&map, frac(3, 1), 100), frac(7, 1));
}

#[test]
fn tempo_units_convert_to_quarters_by_their_factor() {
    assert_eq!(quarters_per_unit(TempoUnit::Eighth), frac(1, 2));
    assert_eq!(quarters_per_unit(TempoUnit::Quarter), frac(1, 1));
    assert_eq!(quarters_per_unit(TempoUnit::DottedQuarter), frac(3, 2));
    assert_eq!(quarters_per_unit(TempoUnit::Half), frac(2, 1));
    assert_eq!(quarters_per_unit(TempoUnit::DottedHalf), frac(3, 1));
    let cases = [
        (TempoUnit::Eighth, 120.0, frac(4, 1)),
        (TempoUnit::DottedQuarter, 60.0, frac(666_667, 250_000)),
        (TempoUnit::Half, 60.0, frac(2, 1)),
        (TempoUnit::DottedHalf, 40.0, frac(2, 1)),
        (TempoUnit::Quarter, 90.0, frac(666_667, 250_000)),
    ];
    for (unit, bpm, bar_seconds) in cases {
        let mut score = plain_score(1);
        score.master_bars[0]
            .tempo_changes
            .push(tempo(frac(0, 1), bpm, unit, false));
        let map = map(&score, RampGrid::MUSICAL);
        assert_eq!(seconds(&map, frac(1, 1), 100), bar_seconds, "{unit:?}");
    }
}

#[test]
fn speed_scales_the_tempo_not_the_score() {
    let score = plain_score(2);
    let map = map(&score, RampGrid::MUSICAL);
    assert_eq!(seconds(&map, frac(1, 1), 50), frac(4, 1));
    assert_eq!(seconds(&map, frac(1, 1), 200), frac(1, 1));
    assert_eq!(seconds(&map, frac(1, 1), 25), frac(8, 1));
    assert_eq!(seconds(&map, frac(3, 2), 75), frac(4, 1));
    let half = Speed::new(50).expect("speed");
    assert_eq!(
        map.time_at(Seconds::from(frac(4, 1)), half),
        Ok(at(frac(1, 1)))
    );
    assert_eq!(
        map.time_at(Seconds::from(frac(3, 2)), Speed::new(200).expect("speed")),
        Ok(at(frac(3, 2)))
    );
    assert_eq!(
        map.time_at(Seconds::from(frac(-1, 1)), half),
        Err(PlaybackError::NegativeTime)
    );
    assert_eq!(
        Speed::new(24),
        Err(PlaybackError::InvalidSpeed { percent: 24 })
    );
    assert_eq!(
        Speed::new(201),
        Err(PlaybackError::InvalidSpeed { percent: 201 })
    );
    assert_eq!(Speed::NORMAL.percent(), 100);
}

fn microsecond_tempo(quarters_per_minute: Fraction) -> Fraction {
    let micros = (Fraction::from_integer(60_000_000) / quarters_per_minute).round();
    Fraction::from_integer(60_000_000) / micros
}

fn quarter_steps() -> RampGrid {
    RampGrid::Step(RampStep::new(frac(1, 4)).expect("step"))
}

fn ramp_score(end: Fraction) -> Score {
    let mut score = plain_score(3);
    score.master_bars[0]
        .tempo_changes
        .push(tempo(frac(0, 1), 60.0, TempoUnit::Quarter, true));
    score.master_bars[1]
        .tempo_changes
        .push(tempo(end, 120.0, TempoUnit::Quarter, false));
    score
}

#[test]
fn a_ramp_steps_on_its_grid_up_to_the_next_tempo() {
    let score = ramp_score(frac(1, 2));
    let map = map(&score, RampGrid::MUSICAL);
    let mut expected = vec![segment(frac(0, 1), frac(60, 1))];
    for step in 1..24 {
        expected.push(segment(
            frac(step, 16),
            microsecond_tempo(frac(60, 1) + frac(60 * step, 24)),
        ));
    }
    expected.push(segment(frac(3, 2), frac(120, 1)));
    assert_eq!(map.segments(), expected.as_slice());
    let gp = self::map(&score, quarter_steps());
    let starts: Vec<Fraction> = gp
        .segments()
        .iter()
        .map(|segment| segment.start.whole_notes())
        .collect();
    assert_eq!(
        starts,
        [
            frac(0, 1),
            frac(1, 4),
            frac(1, 2),
            frac(3, 4),
            frac(1, 1),
            frac(5, 4),
            frac(3, 2)
        ]
    );
    assert_eq!(
        gp.segments()[3].quarters_per_minute,
        microsecond_tempo(frac(90, 1))
    );
}

#[test]
fn a_ramp_keeps_its_last_partial_step() {
    let end = frac(1, 2) + frac(1, 32);
    let score = ramp_score(end);
    let map = map(&score, RampGrid::MUSICAL);
    let segments = map.segments();
    let last_step = segments[segments.len() - 2];
    let span = frac(49, 32);
    assert_eq!(
        last_step,
        segment(
            frac(24, 16),
            microsecond_tempo(frac(60, 1) + frac(60, 1) * frac(24, 16) / span)
        )
    );
    assert_eq!(segments[segments.len() - 1], segment(span, frac(120, 1)));
    assert_eq!(segments.len(), 26);
}

#[test]
fn a_ramp_on_the_last_tempo_holds_it() {
    let mut score = plain_score(2);
    score.master_bars[1]
        .tempo_changes
        .push(tempo(frac(0, 1), 90.0, TempoUnit::Quarter, true));
    let map = map(&score, RampGrid::MUSICAL);
    assert_eq!(
        map.segments(),
        [
            segment(frac(0, 1), frac(120, 1)),
            segment(frac(1, 1), microsecond_tempo(frac(90, 1)))
        ]
    );
}

#[test]
fn a_repeated_bar_plays_its_own_tempo_again() {
    let mut score = score_from(3, "1 DaCapo");
    score.master_bars[0]
        .tempo_changes
        .push(tempo(frac(0, 1), 60.0, TempoUnit::Quarter, true));
    score.master_bars[2]
        .tempo_changes
        .push(tempo(frac(0, 1), 120.0, TempoUnit::Quarter, false));
    let map = map(&score, quarter_steps());
    let first: Vec<Fraction> = map
        .segments()
        .iter()
        .filter(|segment| segment.start.whole_notes() < frac(2, 1))
        .map(|segment| segment.quarters_per_minute)
        .collect();
    let again: Vec<Fraction> = map
        .segments()
        .iter()
        .filter(|segment| {
            segment.start.whole_notes() >= frac(2, 1) && segment.start.whole_notes() < frac(4, 1)
        })
        .map(|segment| segment.quarters_per_minute)
        .collect();
    assert_eq!(first, again);
    assert_eq!(first.len(), 8);
}

#[test]
fn an_inexact_tempo_plays_at_its_nearest_millionth() {
    let up = Tempo::new(120.1234567).expect("tempo");
    assert_eq!(up.exact(), None);
    assert_eq!(playback_bpm(up), Some(frac(120_123_457, 1_000_000)));
    let down = Tempo::new(99.99999949).expect("tempo");
    assert_eq!(down.exact(), None);
    assert_eq!(playback_bpm(down), Some(frac(99_999_999, 1_000_000)));
    let noise = Tempo::new(100.00000000000001).expect("tempo");
    assert_eq!(noise.exact(), None);
    assert_eq!(playback_bpm(noise), Some(frac(100, 1)));
    let exact = Tempo::new(98.0001).expect("tempo");
    assert_eq!(playback_bpm(exact), Some(frac(980_001, 10_000)));
    let change = tempo(frac(0, 1), 120.1234567, TempoUnit::Eighth, false);
    assert_eq!(
        quarters_per_minute(&change),
        Some(frac(120_123_457, 2_000_000))
    );
}

#[test]
fn ramp_grids_hold_or_step_on_a_positive_step() {
    assert_eq!(RampGrid::MUSICAL, RampGrid::Step(RampStep::SIXTEENTH));
    assert_eq!(RampStep::SIXTEENTH.whole_notes(), frac(1, 16));
    assert_eq!(RampGrid::GP, RampGrid::Hold);
    assert_eq!(
        RampStep::new(frac(0, 1)),
        Err(PlaybackError::NonPositiveStep)
    );
    assert_eq!(
        RampStep::new(frac(-1, 8)),
        Err(PlaybackError::NonPositiveStep)
    );
    assert_eq!(
        RampStep::new(frac(1, 32)).map(RampStep::whole_notes),
        Ok(frac(1, 32))
    );
}

#[test]
fn the_gp_profile_holds_the_start_tempo_across_a_ramp() {
    let score = ramp_score(frac(1, 2));
    let map = map(&score, RampGrid::Hold);
    assert_eq!(
        map.segments(),
        [
            segment(frac(0, 1), frac(60, 1)),
            segment(frac(3, 2), frac(120, 1))
        ]
    );
    assert_eq!(seconds(&map, frac(3, 2), 100), frac(6, 1));
}

#[test]
fn every_tempo_plays_at_whole_microseconds_per_quarter() {
    let mut score = plain_score(2);
    score.master_bars[0]
        .tempo_changes
        .push(tempo(frac(0, 1), 84.4235, TempoUnit::Quarter, false));
    score.master_bars[1].tempo_changes.push(tempo(
        frac(0, 1),
        109.659,
        TempoUnit::DottedQuarter,
        false,
    ));
    let map = map(&score, RampGrid::MUSICAL);
    assert_eq!(
        map.segments(),
        [
            segment(frac(0, 1), frac(60_000_000, 710_703)),
            segment(frac(1, 1), frac(60_000_000, 364_767))
        ]
    );
    assert_eq!(seconds(&map, frac(1, 1), 100), frac(4 * 710_703, 1_000_000));
}

#[test]
fn equal_positions_ramp_to_the_tempo_that_wins_there() {
    let mut score = plain_score(2);
    score.master_bars[0]
        .tempo_changes
        .push(tempo(frac(0, 1), 60.0, TempoUnit::Quarter, true));
    score.master_bars[1]
        .tempo_changes
        .push(tempo(frac(0, 1), 240.0, TempoUnit::Quarter, false));
    score.master_bars[1]
        .tempo_changes
        .push(tempo(frac(0, 1), 120.0, TempoUnit::Quarter, false));
    let map = map(&score, quarter_steps());
    let tempi: Vec<Fraction> = map
        .segments()
        .iter()
        .map(|segment| segment.quarters_per_minute)
        .collect();
    assert_eq!(
        tempi,
        [
            frac(60, 1),
            microsecond_tempo(frac(75, 1)),
            microsecond_tempo(frac(90, 1)),
            microsecond_tempo(frac(105, 1)),
            frac(120, 1)
        ]
    );
}

#[test]
fn a_play_order_of_another_score_is_refused() {
    let long = plain_score(4);
    let short = plain_score(2);
    assert_eq!(
        TempoMap::build(&short, &play_order(&long), RampGrid::MUSICAL),
        Err(PlaybackError::OrderMismatch)
    );
}

#[test]
fn only_absurd_positions_overflow_and_never_wrap() {
    const PRIMES: [i64; 12] = [
        999_983, 999_979, 999_961, 999_959, 999_953, 999_931, 999_917, 999_907, 999_883, 999_863,
        999_853, 999_841,
    ];
    let mut score = plain_score(PRIMES.len());
    for (bar, prime) in score.master_bars.iter_mut().zip(PRIMES) {
        bar.tempo_changes
            .push(tempo(frac(1, prime), 100.0, TempoUnit::Quarter, false));
        bar.tempo_changes
            .push(tempo(frac(2, prime), 50.0, TempoUnit::Quarter, false));
    }
    let built = TempoMap::build(&score, &play_order(&score), RampGrid::MUSICAL);
    assert_eq!(built, Err(PlaybackError::TimeOverflow));
}
