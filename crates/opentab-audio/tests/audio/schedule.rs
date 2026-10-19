use num_rational::Ratio;
use opentab_audio::{SampleRate, Scheduler, frame_at};
use opentab_playback::{MidiMessage, PlaybackOptions, Speed, perform};
use opentab_testkit::arb_score;
use proptest::prelude::*;
use proptest::test_runner::{FileFailurePersistence, RngSeed};

use crate::support::{PROFILES, fixture, performance};

const PROPERTY_SEED: u64 = 0x0074_3113;
const PERCENT: i128 = 100;

fn config() -> ProptestConfig {
    ProptestConfig {
        failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
        rng_seed: RngSeed::Fixed(PROPERTY_SEED),
        ..ProptestConfig::default()
    }
}

fn expected_frame(
    tempo: &opentab_playback::TempoMap,
    at: opentab_playback::ScoreTime,
    percent: u16,
    rate: SampleRate,
) -> u64 {
    let normal = tempo.seconds_at(at, Speed::NORMAL).expect("seconds");
    let exact = Ratio::new(normal.numer(), normal.denom())
        * Ratio::from_integer(i128::from(rate.hz()) * PERCENT)
        / Ratio::from_integer(i128::from(percent));
    u64::try_from(exact.floor().to_integer()).expect("frame")
}

fn arb_rate() -> impl Strategy<Value = SampleRate> {
    prop::sample::select(SampleRate::SUPPORTED.to_vec())
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn every_event_lands_on_the_floor_of_its_exact_time_at_any_speed(
        score in arb_score(),
        percent in 25u16..=200,
        rate in arb_rate(),
    ) {
        for profile in PROFILES {
            let Ok(performance) = perform(&score, &PlaybackOptions { profile }) else {
                continue;
            };
            let speed = Speed::new(percent).expect("speed");
            let scheduler = Scheduler::new(&performance, speed, rate).expect("schedule");
            prop_assert_eq!(scheduler.events().len(), performance.events.len());
            let mut previous = 0;
            for (sampled, event) in scheduler.events().iter().zip(&performance.events) {
                prop_assert_eq!(sampled.frame, expected_frame(&performance.tempo, event.at, percent, rate));
                prop_assert_eq!(sampled.track, event.track);
                prop_assert_eq!(sampled.channel, event.channel);
                prop_assert_eq!(sampled.message, event.message);
                prop_assert!(sampled.frame >= previous);
                previous = sampled.frame;
            }
            let yielded: Vec<_> = scheduler.clone().collect();
            prop_assert_eq!(yielded.as_slice(), scheduler.events());
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
            Scheduler::new(&performance, Speed::new(percent).expect("speed"), rate)
                .expect("schedule")
                .map(|event| event.frame)
                .collect()
        };
        let normal = at(100);
        let half = at(50);
        let double = at(200);
        assert!(normal.iter().any(|frame| *frame > 0));
        for ((normal, half), double) in normal.iter().zip(&half).zip(&double) {
            assert!(
                *half == 2 * normal || *half == 2 * normal + 1,
                "{normal} {half}"
            );
            assert_eq!(*double, normal / 2);
        }
        let last = performance.events.last().expect("events");
        assert_eq!(
            frame_at(
                &performance.tempo,
                last.at,
                Speed::new(50).expect("speed"),
                rate
            )
            .expect("frame"),
            *half.last().expect("last")
        );
    }
}

#[test]
fn the_scheduler_peeks_without_consuming() {
    let performance = performance(
        &fixture("note-effects.gp"),
        opentab_playback::Profile::Musical,
    );
    let mut scheduler =
        Scheduler::new(&performance, Speed::NORMAL, SampleRate::STUDIO).expect("schedule");
    let first = *scheduler.peek().expect("first");
    assert_eq!(scheduler.next(), Some(first));
    assert!(matches!(
        scheduler.events()[0].message,
        MidiMessage::Bank { .. } | MidiMessage::Program { .. }
    ));
}
