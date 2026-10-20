use opentab_audio::{
    BUS_FRACTION_BITS, Limiter, LimiterSettings, SampleRate, Synth, TRACK_HEADROOM,
};
use opentab_playback::{Channel, MidiMessage, Profile};
use proptest::prelude::*;

use crate::support::{fixture, performance, render_frames};

const HOSTILE: [f32; 6] = [
    f32::NAN,
    f32::INFINITY,
    f32::NEG_INFINITY,
    f32::MAX,
    -f32::MAX,
    1.0e30,
];
const FRAMES: u64 = 4_800;

struct ConstantSynth {
    samples: Vec<f32>,
    position: usize,
}

impl ConstantSynth {
    fn boxed(samples: &[f32]) -> Box<dyn Synth> {
        Box::new(ConstantSynth {
            samples: samples.to_vec(),
            position: 0,
        })
    }
}

impl Synth for ConstantSynth {
    fn reset(&mut self) {
        self.position = 0;
    }

    fn handle(&mut self, _channel: Channel, _message: &MidiMessage) {}

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (left, right) in left.iter_mut().zip(right.iter_mut()) {
            let sample = self.samples[self.position % self.samples.len()];
            *left = sample;
            *right = -sample;
            self.position += 1;
        }
    }
}

fn render_with(samples: &[f32]) -> Vec<f32> {
    let performance = performance(&fixture("multi-track.gp"), Profile::Musical);
    assert!(performance.tracks.len() >= 2);
    let voices = performance
        .tracks
        .iter()
        .map(|_| ConstantSynth::boxed(samples))
        .collect();
    let audio = render_frames(&performance, voices, SampleRate::STUDIO, FRAMES).expect("render");
    audio.left.into_iter().chain(audio.right).collect()
}

#[test]
fn non_finite_and_huge_samples_from_a_backend_never_overflow_the_bus() {
    let ceiling = LimiterSettings::DEFAULT.ceiling as f32 / (1u32 << BUS_FRACTION_BITS) as f32;
    let output = render_with(&HOSTILE);
    assert!(output.iter().all(|sample| sample.is_finite()));
    assert!(output.iter().all(|sample| sample.abs() < ceiling));
    assert!(output.iter().any(|sample| *sample != 0.0));
}

#[test]
fn nan_is_flushed_to_silence_and_out_of_range_samples_clamp_to_the_headroom() {
    assert!(render_with(&[f32::NAN]).iter().all(|sample| *sample == 0.0));
    assert_eq!(render_with(&[100.0]), render_with(&[TRACK_HEADROOM]));
    assert_eq!(render_with(&[f32::INFINITY]), render_with(&[0.0]));
    assert_ne!(
        render_with(&[TRACK_HEADROOM]),
        render_with(&[TRACK_HEADROOM / 2.0])
    );
}

fn signal() -> impl Strategy<Value = Vec<i64>> {
    let unity = 1i64 << BUS_FRACTION_BITS;
    prop::collection::vec(-4 * unity..4 * unity, 1..3_000)
}

proptest! {
    #[test]
    fn the_limiter_split_across_calls_equals_one_call(
        left in signal(),
        cuts in prop::collection::vec(1usize..700, 1..12),
        rate in prop::sample::select(SampleRate::SUPPORTED.to_vec()),
    ) {
        let right: Vec<i64> = left.iter().map(|sample| -sample / 3).collect();
        let mut whole = Limiter::new(LimiterSettings::DEFAULT, rate);
        let (mut whole_left, mut whole_right) = (left.clone(), right.clone());
        whole.process(&mut whole_left, &mut whole_right);
        let mut split = Limiter::new(LimiterSettings::DEFAULT, rate);
        let (mut split_left, mut split_right) = (left.clone(), right.clone());
        let mut start = 0;
        let mut cut = cuts.iter().cycle();
        while start < split_left.len() {
            let end = (start + cut.next().copied().unwrap_or(1)).min(split_left.len());
            split.process(&mut split_left[start..end], &mut split_right[start..end]);
            start = end;
        }
        prop_assert_eq!(&split_left, &whole_left);
        prop_assert_eq!(&split_right, &whole_right);
        prop_assert_eq!(split, whole);
    }
}
