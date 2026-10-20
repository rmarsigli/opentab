use std::path::Path;

use opentab_audio::{
    Bank, BarRange, BuiltInBank, SampleRate, Session, SessionConfig, Sf2Bank, SfzBank,
    TransportCommand, TransportHandle, TransportState, TransportStatus,
};
use opentab_gp::read_gp;
use opentab_model::Fraction;
use opentab_playback::{MidiMessage, Performance, PlaybackOptions, Profile, Speed, perform};
use stats_alloc::{Region, Stats};

use crate::GLOBAL;

const RATE: SampleRate = SampleRate::STUDIO;
const MAX_BLOCKS: usize = 6_000;
const ODD_BLOCKS: [usize; 6] = [1, 127, 512, 513, 1_000, 64];
const FIXTURES: [&str; 3] = ["note-effects.gp", "repeats.gp", "mixer.gp"];
const LOOP: BarRange = BarRange { first: 1, last: 2 };
const LOOP_SPEED: u16 = 150;
const RANGE: BarRange = BarRange { first: 0, last: 0 };
const RANGE_FROM_BLOCK: usize = 601;
const WRAPS: usize = 2;
const MIN_NOTE: Fraction = Fraction::new_raw(1, 8);
const TAIL: [TailStep; 13] = [
    TailStep {
        block: 0,
        command: TransportCommand::Metronome(true),
    },
    TailStep {
        block: 0,
        command: TransportCommand::Mute(1, true),
    },
    TailStep {
        block: 0,
        command: TransportCommand::Solo(0, true),
    },
    TailStep {
        block: 50,
        command: TransportCommand::Solo(0, false),
    },
    TailStep {
        block: 50,
        command: TransportCommand::CountIn(true),
    },
    TailStep {
        block: 50,
        command: TransportCommand::Pause,
    },
    TailStep {
        block: 60,
        command: TransportCommand::Play,
    },
    TailStep {
        block: 300,
        command: TransportCommand::Stop,
    },
    TailStep {
        block: 310,
        command: TransportCommand::Play,
    },
    TailStep {
        block: 310,
        command: TransportCommand::ClearLoop,
    },
    TailStep {
        block: 600,
        command: TransportCommand::Seek(0),
    },
    TailStep {
        block: 600,
        command: TransportCommand::Metronome(false),
    },
    TailStep {
        block: 601,
        command: TransportCommand::PlayRange(RANGE),
    },
];
const TAIL_SPEEDS: [TailSpeed; 2] = [
    TailSpeed {
        block: 60,
        percent: 25,
    },
    TailSpeed {
        block: 300,
        percent: 200,
    },
];
const TAIL_BLOCKS: usize = 1_000;

#[derive(Debug, Clone, Copy)]
struct TailStep {
    block: usize,
    command: TransportCommand,
}

#[derive(Debug, Clone, Copy)]
struct TailSpeed {
    block: usize,
    percent: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NoteWindow {
    bar: usize,
    from: Fraction,
    to: Fraction,
}

impl NoteWindow {
    fn holds(&self, status: &TransportStatus) -> bool {
        status.played_bar == self.bar && status.offset >= self.from && status.offset <= self.to
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    SpeedMidNote,
    FirstWrap,
    SeekMidNote,
    SecondWrap,
    Tail(usize),
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Driven {
    speed_changed: bool,
    seeked: bool,
    wraps: usize,
    tail_blocks: usize,
    peak_positive: bool,
    range_paused: bool,
}

fn fixture(name: &str) -> Performance {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
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

fn long_note(performance: &Performance, bars: &[usize]) -> Option<NoteWindow> {
    let order = &performance.order;
    performance
        .events
        .iter()
        .enumerate()
        .find_map(|(index, on)| {
            let MidiMessage::NoteOn { key, velocity } = on.message else {
                return None;
            };
            if velocity == 0 {
                return None;
            }
            let off = performance.events.iter().skip(index + 1).find(|event| {
                event.track == on.track
                    && event.channel == on.channel
                    && matches!(
                        event.message,
                        MidiMessage::NoteOff { key: closed, .. }
                            | MidiMessage::NoteOn { key: closed, velocity: 0 }
                            if closed == key
                    )
            })?;
            let bar = order.position_at(on.at)?.played_bar;
            let played = order.bars.get(bar)?;
            let start = played.start.whole_notes();
            let struck = on.at.whole_notes() - start;
            let released = off.at.whole_notes() - start;
            let length = released - struck;
            (bars.contains(&bar) && released <= played.length && length >= MIN_NOTE).then(|| {
                let margin = length / Fraction::from_integer(4);
                NoteWindow {
                    bar,
                    from: struck + margin,
                    to: released - margin,
                }
            })
        })
}

fn speed(percent: u16) -> TransportCommand {
    TransportCommand::Speed(Speed::new(percent).expect("speed"))
}

fn tail_commands(handle: &mut TransportHandle, block: usize) {
    for entry in TAIL.iter().filter(|entry| entry.block == block) {
        handle.send(entry.command).expect("room");
    }
    for entry in TAIL_SPEEDS.iter().filter(|entry| entry.block == block) {
        handle.send(speed(entry.percent)).expect("room");
    }
}

struct Outcome {
    change: Stats,
    driven: Driven,
}

fn drive(bank: &dyn Bank, performance: &Performance) -> Outcome {
    assert!(performance.order.bars.len() > LOOP.last + 1);
    let speed_note =
        long_note(performance, &[LOOP.first, LOOP.last]).expect("a long note in the loop");
    let seek_note = long_note(performance, &[LOOP.first])
        .or_else(|| long_note(performance, &[LOOP.last]))
        .expect("a long note to seek from");
    let seek_target = if seek_note.bar == LOOP.first {
        LOOP.last
    } else {
        LOOP.first
    };
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
    let mut left = vec![0.0f32; 1_024];
    let mut right = vec![0.0f32; 1_024];
    let mut driven = Driven {
        speed_changed: false,
        seeked: false,
        wraps: 0,
        tail_blocks: 0,
        peak_positive: false,
        range_paused: false,
    };
    let mut phase = Phase::SpeedMidNote;
    let mut previous = handle.status();
    let region = Region::new(GLOBAL);
    handle.send(TransportCommand::Play).expect("room");
    handle.send(TransportCommand::Loop(LOOP)).expect("room");
    for block in 0..MAX_BLOCKS {
        let frames = ODD_BLOCKS[block % ODD_BLOCKS.len()];
        transport.process(&mut left[..frames], &mut right[..frames]);
        if !matches!(phase, Phase::Tail(_) | Phase::Done) {
            driven.peak_positive |= left[..frames].iter().any(|sample| *sample != 0.0);
        }
        let status = handle.status();
        let wrapped = status.state == TransportState::Playing
            && previous.state == TransportState::Playing
            && status.played_bar < previous.played_bar;
        previous = status;
        phase = match phase {
            Phase::SpeedMidNote if speed_note.holds(&status) => {
                handle.send(speed(LOOP_SPEED)).expect("room");
                driven.speed_changed = true;
                previous.state = TransportState::Stopped;
                Phase::FirstWrap
            }
            Phase::FirstWrap if wrapped => {
                driven.wraps += 1;
                Phase::SeekMidNote
            }
            Phase::SeekMidNote if seek_note.holds(&status) => {
                handle
                    .send(TransportCommand::Seek(seek_target))
                    .expect("room");
                driven.seeked = true;
                previous.state = TransportState::Stopped;
                Phase::SecondWrap
            }
            Phase::SecondWrap if wrapped => {
                driven.wraps += 1;
                tail_commands(&mut handle, 0);
                Phase::Tail(1)
            }
            Phase::Tail(step) if step >= TAIL_BLOCKS => Phase::Done,
            Phase::Tail(step) => {
                driven.range_paused |=
                    step > RANGE_FROM_BLOCK && status.state == TransportState::Paused;
                tail_commands(&mut handle, step);
                driven.tail_blocks = step;
                Phase::Tail(step + 1)
            }
            Phase::Done => break,
            other => other,
        };
    }
    let change = region.change();
    Outcome { change, driven }
}

pub fn the_transport_processes_blocks_without_allocating(sfz_root: &Path) {
    let sf2 = Sf2Bank::load(&opentab_testkit::banks::tiny_sf2()).expect("tiny sf2");
    let sfz = SfzBank::load(sfz_root).expect("tiny sfz");
    for name in FIXTURES {
        let performance = fixture(name);
        for (label, bank) in [
            ("built-in", &BuiltInBank as &dyn Bank),
            ("sf2", &sf2),
            ("sfz", &sfz),
        ] {
            let Outcome { change, driven } = drive(bank, &performance);
            assert_eq!(
                driven,
                Driven {
                    speed_changed: true,
                    seeked: true,
                    wraps: WRAPS,
                    tail_blocks: TAIL_BLOCKS - 1,
                    peak_positive: true,
                    range_paused: true,
                },
                "{name} {label}"
            );
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
    assert_eq!(sfz.unprepared_count(), 0);
    assert!(sfz.faults().is_empty(), "{:?}", sfz.faults());
}
