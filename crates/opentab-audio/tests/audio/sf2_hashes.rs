use std::path::Path;

use opentab_audio::{RenderedAudio, SampleRate, WAV_HEADER_BYTES, encode_wav};
use opentab_playback::MidiMessage;

use crate::support::{
    PROFILES, bank_voices, fixture, fixture_names, performance, profile_name, render_all,
    tiny_bank, wav_hash,
};

const HASH_FILE: &str = "tests/data/sf2-wav-hashes.txt";
const BLESS_VARIABLE: &str = "OPENTAB_BLESS_WAV";
const EXPECTED_FIXTURES: usize = 35;
const HASHED_PLATFORM: bool = cfg!(all(target_arch = "x86_64", target_os = "linux"));

fn peak(audio: &RenderedAudio) -> f32 {
    audio
        .left
        .iter()
        .chain(&audio.right)
        .fold(0.0f32, |peak, sample| peak.max(sample.abs()))
}

#[test]
fn every_fixture_renders_with_the_tiny_sf2_to_its_pinned_hash_on_x86_64_linux() {
    let names = fixture_names();
    assert_eq!(names.len(), EXPECTED_FIXTURES);
    let bank = tiny_bank();
    let rate = SampleRate::STUDIO;
    let mut lines = Vec::new();
    for name in &names {
        let score = fixture(name);
        for profile in PROFILES {
            let performance = performance(&score, profile);
            let audio = render_all(&performance, bank_voices(&bank, &performance, rate), rate)
                .expect("render");
            assert_eq!(audio.left.len() as u64, audio.frames, "{name}");
            assert_eq!(
                encode_wav(&audio).len() as u64,
                WAV_HEADER_BYTES as u64 + 4 * audio.frames
            );
            assert!(
                audio.left.iter().chain(&audio.right).all(|s| s.is_finite()),
                "{name}"
            );
            let peak = peak(&audio);
            let sounding = performance.events.iter().any(|event| {
                matches!(event.message, MidiMessage::NoteOn { velocity, .. } if velocity > 0)
            });
            assert_eq!(peak > 0.0, sounding, "{name} peak {peak}");
            assert!(peak < 1.0, "{name} peak {peak}");
            lines.push(format!(
                "{name} {} {} {}",
                profile_name(profile),
                rate.hz(),
                wav_hash(&audio)
            ));
        }
    }
    assert!(bank.faults().is_empty(), "{:?}", bank.faults());
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(HASH_FILE);
    if HASHED_PLATFORM && std::env::var_os(BLESS_VARIABLE).is_some() {
        std::fs::write(&path, lines.join("\n") + "\n").expect("bless");
        return;
    }
    let pinned = std::fs::read_to_string(&path).expect("hash file");
    let pinned: Vec<&str> = pinned.lines().collect();
    assert_eq!(pinned.len(), lines.len());
    if HASHED_PLATFORM {
        assert_eq!(pinned, lines);
    }
}
