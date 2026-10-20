use std::path::Path;

use opentab_audio::{
    BarRange, RenderedAudio, SampleRate, ScriptStep, Session, SessionConfig, Synth,
    TransportCommand, WAV_HEADER_BYTES, encode_wav, render_session,
};
use opentab_playback::{Performance, Profile, Speed};

use crate::support::{
    bank_voices, built_in_voices, fixture, performance, tiny_bank, tiny_sfz_bank, wav_hash,
};

const BUILT_IN_FILE: &str = "tests/data/session-hashes.txt";
const BANK_FILE: &str = "tests/data/session-bank-hashes.txt";
const BLESS_VARIABLE: &str = "OPENTAB_BLESS_WAV";
const HASHED_PLATFORM: bool = cfg!(all(target_arch = "x86_64", target_os = "linux"));
const RATE: SampleRate = SampleRate::STUDIO;

struct Scenario {
    name: &'static str,
    fixture: &'static str,
    speed: u16,
    metronome: bool,
    count_in: bool,
    script: Vec<ScriptStep>,
    millis: u32,
}

fn step(frame: u64, command: TransportCommand) -> ScriptStep {
    ScriptStep { frame, command }
}

fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "loop",
            fixture: "repeats.gp",
            speed: 100,
            metronome: false,
            count_in: false,
            script: vec![
                step(0, TransportCommand::Loop(BarRange { first: 1, last: 2 })),
                step(300_000, TransportCommand::Stop),
            ],
            millis: 8_000,
        },
        Scenario {
            name: "speed",
            fixture: "multi-track.gp",
            speed: 75,
            metronome: false,
            count_in: false,
            script: vec![step(
                100_000,
                TransportCommand::Speed(Speed::new(140).expect("speed")),
            )],
            millis: 10_000,
        },
        Scenario {
            name: "count-in",
            fixture: "repeats.gp",
            speed: 100,
            metronome: true,
            count_in: true,
            script: Vec::new(),
            millis: 8_000,
        },
        Scenario {
            name: "mute-solo",
            fixture: "multi-track.gp",
            speed: 100,
            metronome: false,
            count_in: false,
            script: vec![
                step(0, TransportCommand::Mute(1, true)),
                step(96_000, TransportCommand::Solo(0, true)),
                step(192_000, TransportCommand::Solo(0, false)),
            ],
            millis: 8_000,
        },
        Scenario {
            name: "seek",
            fixture: "note-effects.gp",
            speed: 100,
            metronome: false,
            count_in: false,
            script: vec![
                step(48_000, TransportCommand::Seek(4)),
                step(150_000, TransportCommand::Pause),
                step(200_000, TransportCommand::Play),
            ],
            millis: 8_000,
        },
    ]
}

fn play(
    scenario: &Scenario,
    performance: &Performance,
    voices: Vec<Box<dyn Synth>>,
) -> RenderedAudio {
    let config = SessionConfig {
        rate: RATE,
        speed: Speed::new(scenario.speed).expect("speed"),
        metronome: scenario.metronome,
        count_in: scenario.count_in,
    };
    let mut session = Session::new(performance.clone(), voices, config).expect("session");
    let frames = RATE.frames_of_millis(scenario.millis);
    let audio = render_session(&mut session, &scenario.script, frames).expect("render");
    assert_eq!(audio.left.len() as u64, frames, "{}", scenario.name);
    assert_eq!(
        encode_wav(&audio).len() as u64,
        WAV_HEADER_BYTES as u64 + 4 * frames
    );
    let peak = audio
        .left
        .iter()
        .chain(&audio.right)
        .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
    assert!(peak > 0.0 && peak < 1.0, "{} peak {peak}", scenario.name);
    audio
}

fn compare(file: &str, lines: &[String], hashed: bool) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(file);
    if hashed && std::env::var_os(BLESS_VARIABLE).is_some() {
        std::fs::write(&path, lines.join("\n") + "\n").expect("bless");
        return;
    }
    let pinned = std::fs::read_to_string(&path).expect("hash file");
    let pinned: Vec<&str> = pinned.lines().collect();
    assert_eq!(pinned.len(), lines.len());
    if hashed {
        assert_eq!(pinned, lines);
    }
}

#[test]
fn every_transport_scenario_renders_to_its_pinned_built_in_hash_on_every_platform() {
    let mut lines = Vec::new();
    for scenario in scenarios() {
        let performance = performance(&fixture(scenario.fixture), Profile::Musical);
        let audio = play(&scenario, &performance, built_in_voices(&performance, RATE));
        lines.push(format!(
            "{} {} built-in {}",
            scenario.name,
            scenario.fixture,
            wav_hash(&audio)
        ));
    }
    compare(BUILT_IN_FILE, &lines, true);
}

#[test]
fn every_transport_scenario_renders_with_the_tiny_banks_to_its_pinned_hash_on_x86_64_linux() {
    let sf2 = tiny_bank();
    let sfz = tiny_sfz_bank();
    let mut lines = Vec::new();
    for scenario in scenarios() {
        let performance = performance(&fixture(scenario.fixture), Profile::Musical);
        let audio = play(
            &scenario,
            &performance,
            bank_voices(&sf2, &performance, RATE),
        );
        lines.push(format!(
            "{} {} sf2 {}",
            scenario.name,
            scenario.fixture,
            wav_hash(&audio)
        ));
        let audio = play(
            &scenario,
            &performance,
            bank_voices(&sfz, &performance, RATE),
        );
        lines.push(format!(
            "{} {} sfz {}",
            scenario.name,
            scenario.fixture,
            wav_hash(&audio)
        ));
    }
    assert!(sf2.faults().is_empty(), "{:?}", sf2.faults());
    assert!(sfz.faults().is_empty(), "{:?}", sfz.faults());
    compare(BANK_FILE, &lines, HASHED_PLATFORM);
}
