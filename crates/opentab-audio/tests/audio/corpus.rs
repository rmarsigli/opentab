use std::path::PathBuf;

use opentab_audio::{SampleRate, frame_at, render_frames};
use opentab_gp::read_gp;
use opentab_playback::Speed;

use crate::support::{PROFILES, built_in_voices, performance, profile_name};

const CORPUS_VARIABLE: &str = "OPENTAB_CORPUS";
const EXPECTED_FILES: usize = 27;
const PLAYED_BARS: usize = 8;
const TAIL_MS: u32 = 1000;

fn corpus() -> Option<Vec<PathBuf>> {
    let dir = PathBuf::from(std::env::var_os(CORPUS_VARIABLE)?);
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("corpus directory")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "gp"))
        .collect();
    paths.sort();
    Some(paths)
}

#[test]
fn every_corpus_example_renders_its_opening_bars_in_both_profiles() {
    let Some(paths) = corpus() else {
        return;
    };
    assert_eq!(paths.len(), EXPECTED_FILES);
    let rate = SampleRate::STUDIO;
    let mut rendered = 0;
    for path in &paths {
        let score = read_gp(&std::fs::read(path).expect("read")).expect("parse");
        for profile in PROFILES {
            let performance = performance(&score, profile);
            let cut = performance
                .order
                .bars
                .get(PLAYED_BARS)
                .map_or(performance.order.end(), |bar| bar.start);
            let frames = frame_at(&performance.tempo, cut, Speed::NORMAL, rate).expect("frame")
                + rate.frames_of_millis(TAIL_MS);
            let audio = render_frames(
                &performance,
                built_in_voices(&performance, rate),
                rate,
                frames,
            )
            .expect("render");
            let samples = audio.left.iter().chain(&audio.right);
            assert!(
                samples.clone().all(|sample| sample.is_finite()),
                "{}",
                path.display()
            );
            let peak = samples.fold(0.0f32, |peak, sample| peak.max(sample.abs()));
            assert!(
                peak > 0.0,
                "{} {} is silent",
                path.display(),
                profile_name(profile)
            );
            println!(
                "{} {} tracks {} frames {} peak {peak}",
                path.file_name().expect("name").to_string_lossy(),
                profile_name(profile),
                performance.tracks.len(),
                audio.frames
            );
            rendered += 1;
        }
    }
    assert_eq!(rendered, 2 * EXPECTED_FILES);
}
