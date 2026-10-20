use std::path::Path;

use opentab_audio::{SampleRate, WAV_HEADER_BYTES, encode_wav};
use opentab_playback::Profile;

use crate::support::{
    PROFILES, fixture, fixture_names, performance, profile_name, render, wav_hash,
};

const HASH_FILE: &str = "tests/data/wav-hashes.txt";
const BLESS_VARIABLE: &str = "OPENTAB_BLESS_WAV";
const EXPECTED_FIXTURES: usize = 35;
const CD_FIXTURE: &str = "note-effects.gp";

#[test]
fn every_fixture_renders_to_its_pinned_wav_hash_on_every_platform() {
    let names = fixture_names();
    assert_eq!(names.len(), EXPECTED_FIXTURES);
    let mut lines = Vec::new();
    for name in &names {
        let score = fixture(name);
        for profile in PROFILES {
            let rate = SampleRate::STUDIO;
            let audio = render(&performance(&score, profile), rate);
            assert_eq!(audio.left.len() as u64, audio.frames);
            assert_eq!(
                encode_wav(&audio).len() as u64,
                WAV_HEADER_BYTES as u64 + 4 * audio.frames
            );
            lines.push(format!(
                "{name} {} {} {}",
                profile_name(profile),
                rate.hz(),
                wav_hash(&audio)
            ));
        }
    }
    let rate = SampleRate::CD;
    let audio = render(&performance(&fixture(CD_FIXTURE), Profile::Musical), rate);
    lines.push(format!(
        "{CD_FIXTURE} musical {} {}",
        rate.hz(),
        wav_hash(&audio)
    ));
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(HASH_FILE);
    if std::env::var_os(BLESS_VARIABLE).is_some() {
        std::fs::write(&path, lines.join("\n") + "\n").expect("bless");
        return;
    }
    let pinned = std::fs::read_to_string(&path).expect("hash file");
    let pinned: Vec<&str> = pinned.lines().collect();
    assert_eq!(pinned, lines);
}
