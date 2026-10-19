use opentab_audio::{Limiter, LimiterSettings, SampleRate, encode_wav, pcm16, render_all};
use opentab_playback::Profile;

use crate::support::{
    PROFILES, bank_voices, fixture, performance, render, tiny_bank, tiny_sfz_bank,
};

const BLOCK_FRAMES: usize = 2_400;
const ACTIVE_FLOOR_DB: f64 = -60.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Loudness {
    median_active_db: f64,
    peak_db: f64,
    hard_clipped: usize,
}

fn loudness(left: &[f32], right: &[f32]) -> Loudness {
    let mut levels: Vec<f64> = left
        .chunks(BLOCK_FRAMES)
        .map(|block| {
            let power = block
                .iter()
                .map(|sample| f64::from(*sample).powi(2))
                .sum::<f64>()
                / block.len() as f64;
            10.0 * power.max(1e-20).log10()
        })
        .filter(|level| *level > ACTIVE_FLOOR_DB)
        .collect();
    levels.sort_by(f64::total_cmp);
    let peak = left
        .iter()
        .chain(right)
        .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
    let hard_clipped = left
        .iter()
        .chain(right)
        .filter(|sample| matches!(pcm16(**sample), i16::MAX | i16::MIN))
        .count();
    Loudness {
        median_active_db: levels
            .get(levels.len() / 2)
            .copied()
            .unwrap_or(f64::NEG_INFINITY),
        peak_db: 20.0 * f64::from(peak).max(1e-10).log10(),
        hard_clipped,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Window {
    name: &'static str,
    low_db: f64,
    high_db: f64,
}

const WINDOWS: [Window; 2] = [
    Window {
        name: "note-effects.gp",
        low_db: -21.0,
        high_db: -16.0,
    },
    Window {
        name: "multi-track.gp",
        low_db: -14.0,
        high_db: -8.0,
    },
];
const DENSE: &str = "tunings.gp";
const BUS_UNITY: i64 = 1 << 20;

#[test]
fn solo_and_band_fixtures_sit_in_their_level_windows() {
    for window in WINDOWS {
        let audio = render(
            &performance(&fixture(window.name), Profile::Musical),
            SampleRate::STUDIO,
        );
        let level = loudness(&audio.left, &audio.right);
        assert!(
            (window.low_db..=window.high_db).contains(&level.median_active_db),
            "{window:?} {level:?}"
        );
        assert_eq!(level.hard_clipped, 0, "{window:?}");
    }
}

#[test]
fn the_dense_band_never_hard_clips() {
    for profile in PROFILES {
        let audio = render(&performance(&fixture(DENSE), profile), SampleRate::STUDIO);
        let level = loudness(&audio.left, &audio.right);
        assert_eq!(level.hard_clipped, 0, "{profile:?} {level:?}");
        assert!(level.peak_db < 0.0, "{profile:?} {level:?}");
        let bytes = encode_wav(&audio);
        let loudest = bytes[44..]
            .chunks(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]).unsigned_abs())
            .max()
            .expect("samples");
        assert!(loudest < 32_767, "{profile:?} {loudest}");
    }
}

#[test]
fn the_limiter_passes_quiet_audio_and_never_reaches_full_scale() {
    let settings = LimiterSettings::DEFAULT;
    let mut limiter = Limiter::new(settings, SampleRate::STUDIO);
    let mut quiet_left: Vec<i64> = (0..4_800)
        .map(|index| (index % 200 - 100) * 4_000)
        .collect();
    let mut quiet_right = quiet_left.clone();
    let expected = quiet_left.clone();
    limiter.process(&mut quiet_left, &mut quiet_right);
    assert_eq!(quiet_left, expected);
    let mut loud_left: Vec<i64> = (0..4_800)
        .map(|index| (index % 7 - 3) * BUS_UNITY * 40)
        .collect();
    let mut loud_right: Vec<i64> = loud_left.iter().map(|sample| -sample).collect();
    limiter.process(&mut loud_left, &mut loud_right);
    assert!(
        loud_left
            .iter()
            .chain(&loud_right)
            .all(|sample| sample.abs() < settings.ceiling)
    );
    assert!(loud_left.iter().any(|sample| sample.abs() > settings.knee));
}

const PARITY_FIXTURES: [&str; 2] = ["note-effects.gp", "multi-track.gp"];
const PARITY_WINDOW_DB: f64 = 6.0;

#[test]
fn the_tiny_sf2_sits_within_a_fixed_window_of_the_built_in_level() {
    let bank = tiny_bank();
    let rate = SampleRate::STUDIO;
    for name in PARITY_FIXTURES {
        let performance = performance(&fixture(name), Profile::Musical);
        let built_in = render(&performance, rate);
        let sf2 =
            render_all(&performance, bank_voices(&bank, &performance, rate), rate).expect("render");
        let reference = loudness(&built_in.left, &built_in.right).median_active_db;
        let level = loudness(&sf2.left, &sf2.right).median_active_db;
        println!("{name}: built-in {reference:.1} dBFS, tiny SF2 {level:.1} dBFS");
        assert!(
            (level - reference).abs() <= PARITY_WINDOW_DB,
            "{name}: tiny SF2 {level:.1} dBFS against built-in {reference:.1} dBFS"
        );
    }
}

#[test]
fn the_tiny_sfz_sits_within_a_fixed_window_of_the_built_in_level() {
    let bank = tiny_sfz_bank();
    let rate = SampleRate::STUDIO;
    for name in PARITY_FIXTURES {
        let performance = performance(&fixture(name), Profile::Musical);
        let built_in = render(&performance, rate);
        let sfz =
            render_all(&performance, bank_voices(&bank, &performance, rate), rate).expect("render");
        let reference = loudness(&built_in.left, &built_in.right).median_active_db;
        let level = loudness(&sfz.left, &sfz.right);
        println!("{name}: built-in {reference:.1} dBFS, tiny SFZ {level:?}");
        assert!(
            (level.median_active_db - reference).abs() <= PARITY_WINDOW_DB,
            "{name}: tiny SFZ {:.1} dBFS against built-in {reference:.1} dBFS",
            level.median_active_db
        );
    }
}
