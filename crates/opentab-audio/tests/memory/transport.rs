use opentab_audio::{
    Bank, BarRange, BuiltInBank, SampleRate, Session, SessionConfig, Sf2Bank, SfzBank,
    TransportCommand,
};
use opentab_gp::read_gp;
use opentab_playback::{Performance, PlaybackOptions, Profile, Speed, perform};
use stats_alloc::{Region, Stats};

use crate::GLOBAL;

const RATE: SampleRate = SampleRate::STUDIO;
const BLOCKS: usize = 1_600;
const ODD_BLOCKS: [usize; 6] = [1, 127, 512, 513, 1_000, 64];
const FIXTURES: [&str; 3] = ["note-effects.gp", "multi-track.gp", "repeats.gp"];

fn fixture(name: &str) -> Performance {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/gp")
        .join(name);
    let score = read_gp(&std::fs::read(path).expect("fixture")).expect("reads");
    perform(
        &score,
        &PlaybackOptions {
            profile: Profile::Musical,
        },
    )
    .expect("perform")
}

fn script(block: usize) -> Vec<TransportCommand> {
    let speed = |percent: u16| TransportCommand::Speed(Speed::new(percent).expect("speed"));
    match block {
        0 => vec![TransportCommand::Play, TransportCommand::Metronome(true)],
        40 => vec![TransportCommand::Loop(BarRange { first: 1, last: 2 })],
        300 => vec![TransportCommand::Seek(3), speed(150)],
        400 => vec![
            TransportCommand::Mute(1, true),
            TransportCommand::Solo(0, true),
        ],
        500 => vec![
            TransportCommand::Solo(0, false),
            TransportCommand::Mute(1, false),
            TransportCommand::CountIn(true),
            TransportCommand::Pause,
        ],
        520 => vec![TransportCommand::Play, speed(25)],
        800 => vec![TransportCommand::Stop, speed(200)],
        810 => vec![TransportCommand::Play, TransportCommand::ClearLoop],
        1_200 => vec![
            TransportCommand::Seek(0),
            TransportCommand::Metronome(false),
        ],
        _ => Vec::new(),
    }
}

fn allocations(bank: &dyn Bank, performance: &Performance) -> Stats {
    let voices = performance
        .tracks
        .iter()
        .map(|_| bank.instantiate(RATE))
        .collect();
    let config = SessionConfig {
        rate: RATE,
        ..SessionConfig::default()
    };
    let (mut transport, mut handle) = Session::new(performance.clone(), voices, config)
        .expect("session")
        .split();
    let scripted: Vec<Vec<TransportCommand>> = (0..BLOCKS).map(script).collect();
    let mut left = vec![0.0f32; 1_024];
    let mut right = vec![0.0f32; 1_024];
    let mut peak = 0.0f32;
    let mut frames_seen = 0;
    let region = Region::new(GLOBAL);
    for (block, commands) in scripted.iter().enumerate() {
        for command in commands {
            handle.send(*command).expect("room");
        }
        let frames = ODD_BLOCKS[block % ODD_BLOCKS.len()];
        transport.process(&mut left[..frames], &mut right[..frames]);
        peak = left[..frames]
            .iter()
            .fold(peak, |peak, sample| peak.max(sample.abs()));
        frames_seen = handle.status().frame;
    }
    let change = region.change();
    assert!(peak > 0.0, "the session sounds");
    assert!(frames_seen > 0);
    change
}

pub fn the_transport_processes_blocks_without_allocating() {
    let sf2 = Sf2Bank::load(&opentab_testkit::banks::tiny_sf2()).expect("tiny sf2");
    for name in FIXTURES {
        let performance = fixture(name);
        for (label, bank) in [("built-in", &BuiltInBank as &dyn Bank), ("sf2", &sf2)] {
            let change = allocations(bank, &performance);
            assert_eq!(
                (
                    change.allocations,
                    change.reallocations,
                    change.bytes_allocated
                ),
                (0, 0, 0),
                "{name} {label}: {change:?}"
            );
        }
    }
}

pub fn sfz_allocations_after_preparation(root: &std::path::Path) -> Stats {
    let bank = SfzBank::load(root).expect("tiny sfz");
    let performance = fixture("multi-track.gp");
    allocations(&bank, &performance)
}
