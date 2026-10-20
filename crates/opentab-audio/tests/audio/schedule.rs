use num_rational::Ratio;
use opentab_audio::{SampleRate, Session, SessionConfig, frame_at, render_session};
use opentab_playback::{Performance, PlaybackOptions, ScoreTime, Speed, TempoMap, perform};
use opentab_testkit::arb_score;
use proptest::prelude::*;
use proptest::test_runner::{FileFailurePersistence, RngSeed};

use crate::recording::{Heard, heard, recorders};
use crate::support::{PROFILES, fixture, performance};

const PROPERTY_SEED: u64 = 0x0074_3113;
const PERCENT: i128 = 100;
const TAIL_FRAMES: u64 = 2;

fn config() -> ProptestConfig {
    ProptestConfig {
        failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
        rng_seed: RngSeed::Fixed(PROPERTY_SEED),
        ..ProptestConfig::default()
    }
}

fn exact_frames(tempo: &TempoMap, at: ScoreTime, percent: u16, rate: SampleRate) -> Ratio<i128> {
    let normal = tempo.seconds_at(at, Speed::NORMAL).expect("seconds");
    Ratio::new(normal.numer(), normal.denom())
        * Ratio::from_integer(i128::from(rate.hz()) * PERCENT)
        / Ratio::from_integer(i128::from(percent))
}

fn expected_frame(tempo: &TempoMap, at: ScoreTime, percent: u16, rate: SampleRate) -> u64 {
    u64::try_from(exact_frames(tempo, at, percent, rate).floor().to_integer()).expect("frame")
}

fn arb_rate() -> impl Strategy<Value = SampleRate> {
    prop::sample::select(SampleRate::SUPPORTED.to_vec())
}

fn session_heard(performance: &Performance, percent: u16, rate: SampleRate) -> Vec<Vec<Heard>> {
    let recorders = recorders(performance);
    let config = SessionConfig {
        rate,
        speed: Speed::new(percent).expect("speed"),
        ..SessionConfig::default()
    };
    let mut session = Session::new(performance.clone(), recorders.voices, config).expect("session");
    let frames =
        expected_frame(&performance.tempo, performance.order.end(), percent, rate) + TAIL_FRAMES;
    let last = performance.events.last().map_or(0, |event| {
        expected_frame(&performance.tempo, event.at, percent, rate) + TAIL_FRAMES
    });
    render_session(&mut session, &[], frames.max(last)).expect("render");
    recorders.logs.iter().map(heard).collect()
}

fn expected_heard(performance: &Performance, percent: u16, rate: SampleRate) -> Vec<Vec<Heard>> {
    (0..performance.tracks.len())
        .map(|track| {
            performance
                .events
                .iter()
                .filter(|event| event.track == track)
                .map(|event| Heard {
                    frame: expected_frame(&performance.tempo, event.at, percent, rate),
                    channel: event.channel,
                    message: event.message,
                })
                .collect()
        })
        .collect()
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn every_event_reaches_its_synth_on_the_floor_of_its_exact_time_at_any_speed(
        score in arb_score(),
        percent in 25u16..=200,
        rate in arb_rate(),
    ) {
        for profile in PROFILES {
            let Ok(performance) = perform(&score, &PlaybackOptions { profile }) else {
                continue;
            };
            let expected = expected_heard(&performance, percent, rate);
            let logs = session_heard(&performance, percent, rate);
            prop_assert_eq!(logs, expected);
            let speed = Speed::new(percent).expect("speed");
            for event in &performance.events {
                prop_assert_eq!(
                    frame_at(&performance.tempo, event.at, speed, rate).expect("frame"),
                    expected_frame(&performance.tempo, event.at, percent, rate)
                );
            }
        }
    }
}

#[test]
fn half_speed_doubles_every_frame_and_double_speed_halves_it() {
    let score = fixture("durations.gp");
    for profile in PROFILES {
        let performance = performance(&score, profile);
        let rate = SampleRate::STUDIO;
        let at = |percent: u16| -> Vec<u64> {
            session_heard(&performance, percent, rate)
                .iter()
                .flatten()
                .map(|heard| heard.frame)
                .collect()
        };
        let normal = at(100);
        let half = at(50);
        let double = at(200);
        assert_eq!(normal.len(), performance.events.len());
        assert!(normal.iter().any(|frame| *frame > 0));
        for ((normal, half), double) in normal.iter().zip(&half).zip(&double) {
            assert!(
                *half == 2 * normal || *half == 2 * normal + 1,
                "{normal} {half}"
            );
            assert_eq!(*double, normal / 2);
        }
    }
}

const COVERAGE_SCORES: usize = 128;
const COVERAGE_PERCENT: u16 = 75;
const MIN_COVERED: usize = 200;

#[test]
fn the_frame_property_sees_many_positive_and_fractional_frames() {
    use proptest::strategy::ValueTree;
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let rate = SampleRate::CD;
    let mut positive = 0;
    let mut fractional = 0;
    let mut performed = 0;
    for _ in 0..COVERAGE_SCORES {
        let score = arb_score().new_tree(&mut runner).expect("tree").current();
        for profile in PROFILES {
            let Ok(performance) = perform(&score, &PlaybackOptions { profile }) else {
                continue;
            };
            performed += 1;
            let logs = session_heard(&performance, COVERAGE_PERCENT, rate);
            assert_eq!(logs, expected_heard(&performance, COVERAGE_PERCENT, rate));
            for event in &performance.events {
                let exact = exact_frames(&performance.tempo, event.at, COVERAGE_PERCENT, rate);
                positive += usize::from(exact >= Ratio::from_integer(1));
                fractional += usize::from(!exact.is_integer());
            }
        }
    }
    assert_eq!(performed, 2 * COVERAGE_SCORES);
    assert!(positive >= MIN_COVERED, "{positive}");
    assert!(fractional >= MIN_COVERED, "{fractional}");
}
