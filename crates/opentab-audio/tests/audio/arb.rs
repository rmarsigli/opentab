use opentab_audio::{BuiltInSynth, SampleRate, Scheduler, Synth, encode_wav, full_length};
use opentab_playback::{PlaybackOptions, Speed, perform};
use opentab_testkit::arb_score;
use proptest::prelude::*;
use proptest::test_runner::{FileFailurePersistence, RngSeed};

use crate::support::{PROFILES, built_in_voices, render_frames};

const PROPERTY_SEED: u64 = 0x0074_3114;
const RENDER_CAP_MS: u32 = 1500;
const STEP_FRAMES: usize = 16;

fn config() -> ProptestConfig {
    ProptestConfig {
        failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
        rng_seed: RngSeed::Fixed(PROPERTY_SEED),
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn the_audio_crate_never_panics_on_generated_scores(score in arb_score()) {
        for profile in PROFILES {
            let Ok(performance) = perform(&score, &PlaybackOptions { profile }) else {
                continue;
            };
            let rate = SampleRate::STUDIO;
            let frames = full_length(&performance, rate)
                .expect("length")
                .min(rate.frames_of_millis(RENDER_CAP_MS));
            let audio = render_frames(&performance, built_in_voices(&performance, rate), rate, frames)
                .expect("render");
            prop_assert!(audio.left.iter().chain(&audio.right).all(|sample| sample.is_finite()));
            prop_assert_eq!(encode_wav(&audio).len() as u64, 44 + 4 * frames);
            let mut synths: Vec<BuiltInSynth> =
                performance.tracks.iter().map(|_| BuiltInSynth::new(rate)).collect();
            let mut left = [0.0; STEP_FRAMES];
            let mut right = [0.0; STEP_FRAMES];
            for event in Scheduler::new(&performance, Speed::NORMAL, rate).expect("schedule") {
                let synth = &mut synths[event.track];
                synth.handle(event.channel, &event.message);
                synth.render(&mut left, &mut right);
                prop_assert!(left.iter().chain(&right).all(|sample| sample.is_finite() && sample.abs() <= 8.0));
            }
        }
    }
}
