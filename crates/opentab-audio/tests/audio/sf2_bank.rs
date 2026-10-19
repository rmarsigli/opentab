use std::path::{Path, PathBuf};

use opentab_audio::{Bank, SampleRate, Sf2Bank, encode_wav, full_length, render_frames};
use opentab_gp::read_gp;
use opentab_playback::Profile;

use crate::support::{bank_voices, performance, root, wav_hash};

const BANK_VARIABLE: &str = "OPENTAB_SF2";
const SCORES_VARIABLE: &str = "OPENTAB_SF2_SCORES";
const OUT_VARIABLE: &str = "OPENTAB_SF2_OUT";
const DEFAULT_SCORE: &str = "fixtures/gp/note-effects.gp";
const LIMIT_MS: u32 = 40_000;
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
fn a_real_sf2_bank_renders_scores_for_listening() {
    let Some(bank_path) = std::env::var_os(BANK_VARIABLE) else {
        return;
    };
    let out = std::env::var_os(OUT_VARIABLE).map_or_else(std::env::temp_dir, PathBuf::from);
    std::fs::create_dir_all(&out).expect("output directory");
    let bank = Sf2Bank::load(&std::fs::read(&bank_path).expect("bank file")).expect("bank loads");
    println!(
        "bank {:?}: {} instruments, {} sample bytes",
        bank.name(),
        bank.instruments().len(),
        bank.sample_bytes()
    );
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
        let target = out.join(format!("{stem}-musical-sf2.wav"));
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
}
