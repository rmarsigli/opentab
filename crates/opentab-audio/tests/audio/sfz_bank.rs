use std::path::{Path, PathBuf};

use opentab_audio::{SampleRate, SfzBank, encode_wav, full_length};
use opentab_gp::read_gp;
use opentab_playback::Profile;

use crate::support::{bank_voices, built_in_voices, performance, render_frames, root, wav_hash};

const BANK_VARIABLE: &str = "OPENTAB_SFZ";
const SCORES_VARIABLE: &str = "OPENTAB_SFZ_SCORES";
const OUT_VARIABLE: &str = "OPENTAB_SFZ_OUT";
const DEFAULT_SCORE: &str = "fixtures/gp/note-effects.gp";
const LIMIT_MS: u32 = 40_000;
const REAL_BANK_WINDOW_DB: f64 = 3.0;
const BLOCK_FRAMES: usize = 2_400;
const ACTIVE_FLOOR_DB: f64 = -60.0;

fn decibels(level: f64) -> f64 {
    20.0 * level.max(1e-10).log10()
}

fn median_active_db(samples: &[f32]) -> f64 {
    let mut levels: Vec<f64> = samples
        .chunks(BLOCK_FRAMES)
        .map(|block| {
            let power = block
                .iter()
                .map(|sample| f64::from(*sample).powi(2))
                .sum::<f64>()
                / block.len() as f64;
            decibels(power.sqrt())
        })
        .filter(|level| *level > ACTIVE_FLOOR_DB)
        .collect();
    levels.sort_by(f64::total_cmp);
    levels
        .get(levels.len() / 2)
        .copied()
        .unwrap_or(f64::NEG_INFINITY)
}

fn scores() -> Vec<PathBuf> {
    match std::env::var(SCORES_VARIABLE) {
        Ok(list) => list
            .split(':')
            .filter(|part| !part.is_empty())
            .map(PathBuf::from)
            .collect(),
        Err(_) => vec![root().join(DEFAULT_SCORE)],
    }
}

#[test]
fn a_real_sfz_bank_renders_scores_for_listening() {
    let Some(bank_path) = std::env::var_os(BANK_VARIABLE) else {
        return;
    };
    let out = std::env::var_os(OUT_VARIABLE).map_or_else(std::env::temp_dir, PathBuf::from);
    std::fs::create_dir_all(&out).expect("output directory");
    let bank = SfzBank::load(Path::new(&bank_path)).expect("the bank passes every limit and loads");
    println!(
        "bank {}: instruments {:?}, {} samples, {} loader bytes, skipped {:?}",
        bank.root().display(),
        bank.instrument_names(),
        bank.sample_count(),
        bank.loader_bytes(),
        bank.skipped()
    );
    for name in bank.instrument_names() {
        println!(
            "{name}: {} regions, ignored {:?}",
            bank.regions(&name).len(),
            bank.ignored(&name)
        );
        let samples = bank
            .preload(&name)
            .expect("every sample passes the decoder limits");
        println!(
            "{name}: {samples} samples decoded, {} bytes",
            bank.decoded_bytes()
        );
    }
    let rate = SampleRate::STUDIO;
    for path in scores() {
        let score = read_gp(&std::fs::read(&path).expect("score")).expect("parse");
        let performance = performance(&score, Profile::Musical);
        let frames = full_length(&performance, rate)
            .expect("length")
            .min(rate.frames_of_millis(LIMIT_MS));
        let audio = render_frames(
            &performance,
            bank_voices(&bank, &performance, rate),
            rate,
            frames,
        )
        .expect("render");
        let samples: Vec<f32> = audio.left.iter().chain(&audio.right).copied().collect();
        assert!(samples.iter().all(|sample| sample.is_finite()));
        let peak = samples
            .iter()
            .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
        assert!(peak > 0.0, "{} is silent", path.display());
        let stem = Path::new(path.file_stem().expect("stem"))
            .display()
            .to_string();
        let target = out.join(format!("{stem}-musical-sfz.wav"));
        std::fs::write(&target, encode_wav(&audio)).expect("write");
        println!(
            "{} tracks {} seconds {:.1} peak {:.1} dBFS median active {:.1} dBFS sha256 {}",
            target.display(),
            performance.tracks.len(),
            audio.frames as f64 / f64::from(rate.hz()),
            decibels(f64::from(peak)),
            median_active_db(&audio.left),
            wav_hash(&audio)
        );
    }
    assert!(bank.faults().is_empty(), "{:?}", bank.faults());
    let reference = performance(
        &read_gp(&std::fs::read(root().join(DEFAULT_SCORE)).expect("score")).expect("parse"),
        Profile::Musical,
    );
    let frames = full_length(&reference, rate).expect("length");
    let sfz = render_frames(
        &reference,
        bank_voices(&bank, &reference, rate),
        rate,
        frames,
    )
    .expect("render");
    let built_in =
        render_frames(&reference, built_in_voices(&reference, rate), rate, frames).expect("render");
    let sfz_level = median_active_db(&sfz.left);
    let built_in_level = median_active_db(&built_in.left);
    println!(
        "note-effects median active: SFZ {sfz_level:.1} dBFS, built-in {built_in_level:.1} dBFS"
    );
    assert!(
        (sfz_level - built_in_level).abs() <= REAL_BANK_WINDOW_DB,
        "SFZ {sfz_level:.1} dBFS against built-in {built_in_level:.1} dBFS"
    );
}
