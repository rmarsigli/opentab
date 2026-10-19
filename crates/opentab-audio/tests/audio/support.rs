use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use opentab_audio::{
    Bank, BuiltInBank, RenderedAudio, SampleRate, Sf2Bank, Synth, encode_wav, render_all,
};
use opentab_gp::read_gp;
use opentab_model::Score;
use opentab_playback::{Performance, PlaybackOptions, Profile, perform};
use sha2::{Digest, Sha256};

pub const PROFILES: [Profile; 2] = [Profile::Gp, Profile::Musical];

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn fixture_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root().join("fixtures/gp"))
        .expect("fixtures")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.ends_with(".gp"))
        .collect();
    names.sort();
    names
}

pub fn fixture(name: &str) -> Score {
    let bytes = std::fs::read(root().join("fixtures/gp").join(name)).expect("fixture");
    read_gp(&bytes).expect("fixture reads")
}

pub fn performance(score: &Score, profile: Profile) -> Performance {
    perform(score, &PlaybackOptions { profile }).expect("perform")
}

pub fn built_in_voices(performance: &Performance, rate: SampleRate) -> Vec<Box<dyn Synth>> {
    performance
        .tracks
        .iter()
        .map(|_| BuiltInBank.instantiate(rate))
        .collect()
}

pub fn render(performance: &Performance, rate: SampleRate) -> RenderedAudio {
    render_all(performance, built_in_voices(performance, rate), rate).expect("render")
}

pub fn wav_hash(audio: &RenderedAudio) -> String {
    let digest = Sha256::digest(encode_wav(audio));
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn profile_name(profile: Profile) -> &'static str {
    match profile {
        Profile::Gp => "gp",
        Profile::Musical => "musical",
    }
}

pub fn tiny_bytes() -> &'static [u8] {
    static BYTES: OnceLock<Vec<u8>> = OnceLock::new();
    BYTES.get_or_init(opentab_testkit::banks::tiny_sf2)
}

pub fn tiny_bank() -> Sf2Bank {
    Sf2Bank::load(tiny_bytes()).expect("the tiny bank loads")
}

pub fn bank_voices(
    bank: &dyn Bank,
    performance: &Performance,
    rate: SampleRate,
) -> Vec<Box<dyn Synth>> {
    performance
        .tracks
        .iter()
        .map(|_| bank.instantiate(rate))
        .collect()
}

pub fn scratch_dir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("sfz-")
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .expect("a scratch folder")
}

pub fn tiny_sfz_root() -> &'static Path {
    static ROOT: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    let (_, root) = ROOT.get_or_init(|| {
        let dir = scratch_dir();
        let root = opentab_testkit::banks::tiny_sfz(dir.path());
        (dir, root)
    });
    root
}

pub fn tiny_sfz_bank() -> opentab_audio::SfzBank {
    opentab_audio::SfzBank::load(tiny_sfz_root()).expect("the tiny SFZ bank loads")
}
