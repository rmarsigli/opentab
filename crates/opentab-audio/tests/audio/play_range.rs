use opentab_audio::{
    AudioOutput, BarRange, NullOutput, RenderedAudio, SampleRate, ScriptStep, Session,
    SessionConfig, TransportCommand, TransportHandle, TransportState, range_frames, render_session,
};
use opentab_playback::{MidiMessage, Performance, Profile, Speed};

use crate::recording::{SharedLog, heard, recorders};
use crate::support::{built_in_voices, config, fixture, performance};
use crate::transport::with_pickup;

const RATE: SampleRate = SampleRate::STUDIO;
const FIXTURES: [&str; 5] = [
    "repeats.gp",
    "note-effects.gp",
    "multi-track.gp",
    "tempo-ramp.gp",
    "trill.gp",
];
const SPEEDS: [u16; 4] = [25, 100, 131, 200];
const MAX_FIRST_BAR: usize = 12;
const LENGTHS: [usize; 2] = [1, 2];
const ODD_BLOCKS: [usize; 8] = [1, 7, 333, 511, 512, 513, 4_800, 2];
const AFTER_FRAMES: u64 = 48_000;
const CONTROL_FRAMES: u64 = 4_800;

struct Stepped {
    output: NullOutput,
    handle: TransportHandle,
    logs: Vec<SharedLog>,
}

fn speed(percent: u16) -> Speed {
    Speed::new(percent).expect("speed")
}

fn session_config(percent: u16, count_in: bool) -> SessionConfig {
    SessionConfig {
        speed: speed(percent),
        count_in,
        ..config(RATE)
    }
}

fn stepped(performance: &Performance, config: SessionConfig, start: TransportCommand) -> Stepped {
    let recorders = recorders(performance);
    let session = Session::new(performance.clone(), recorders.voices, config).expect("session");
    let (transport, mut handle) = session.split();
    handle.send(start).expect("send");
    let mut output = NullOutput::manual(RATE);
    output.start(transport).expect("start");
    Stepped {
        output,
        handle,
        logs: recorders.logs,
    }
}

fn ranges(performance: &Performance) -> Vec<BarRange> {
    let bars = performance.order.bars.len();
    let mut found = Vec::new();
    for first in 0..bars.min(MAX_FIRST_BAR) {
        for length in LENGTHS {
            let last = first + length - 1;
            if last + 1 < bars {
                found.push(BarRange { first, last });
            }
        }
    }
    found
}

fn note_on_from(logs: &[SharedLog], frame: u64) -> bool {
    logs.iter().any(|log| {
        heard(log).iter().any(|entry| {
            entry.frame >= frame
                && matches!(entry.message, MidiMessage::NoteOn { velocity, .. } if velocity > 0)
        })
    })
}

#[test]
fn a_play_range_pauses_in_the_frame_where_the_next_bar_would_start() {
    let mut checked = 0;
    for name in FIXTURES {
        let performance = performance(&fixture(name), Profile::Musical);
        for percent in SPEEDS {
            for range in ranges(&performance) {
                let expected =
                    range_frames(&performance, range, speed(percent), RATE).expect("frames");
                let label = format!("{name} {percent}% {range:?}");
                let mut run = stepped(
                    &performance,
                    session_config(percent, false),
                    TransportCommand::PlayRange(range),
                );
                let mut total = 0;
                let mut blocks = ODD_BLOCKS.iter().copied().cycle();
                while total + 1 < expected {
                    let frames = (blocks.next().unwrap_or(1) as u64).min(expected - 1 - total);
                    assert!(run.output.step(frames));
                    total += frames;
                    assert_eq!(
                        run.handle.status().state,
                        TransportState::Playing,
                        "{label} at {total}"
                    );
                }
                let before = run.handle.status();
                assert_eq!(before.frame, expected - 1, "{label}");
                assert!(before.played_bar <= range.last, "{label}");
                assert!(run.output.step(1));
                let after = run.handle.status();
                assert_eq!(after.state, TransportState::Paused, "{label}");
                assert_eq!(after.frame, expected, "{label}");
                assert!(run.output.step(AFTER_FRAMES));
                assert_eq!(run.handle.status().state, TransportState::Paused, "{label}");
                assert!(!note_on_from(&run.logs, expected), "{label}");
                checked += 1;
            }
        }
    }
    assert!(checked > 100, "{checked}");
}

#[test]
fn without_the_range_end_the_next_bar_is_heard() {
    let performance = performance(&fixture("repeats.gp"), Profile::Musical);
    let range = BarRange { first: 1, last: 1 };
    let expected = range_frames(&performance, range, speed(100), RATE).expect("frames");
    let mut run = stepped(
        &performance,
        session_config(100, false),
        TransportCommand::Seek(range.first),
    );
    run.handle.send(TransportCommand::Play).expect("send");
    assert!(run.output.step(expected + CONTROL_FRAMES));
    assert_eq!(run.handle.status().state, TransportState::Playing);
    assert!(note_on_from(&run.logs, expected));
}

fn reference(
    performance: &Performance,
    range: BarRange,
    percent: u16,
    pause: u64,
) -> RenderedAudio {
    let mut session = Session::new(
        performance.clone(),
        built_in_voices(performance, RATE),
        session_config(percent, false),
    )
    .expect("session");
    let script = [
        ScriptStep {
            frame: 0,
            command: TransportCommand::Seek(range.first),
        },
        ScriptStep {
            frame: pause,
            command: TransportCommand::Pause,
        },
    ];
    render_session(&mut session, &script, pause + AFTER_FRAMES).expect("render")
}

fn ranged(performance: &Performance, range: BarRange, percent: u16, frames: u64) -> RenderedAudio {
    let mut session = Session::new(
        performance.clone(),
        built_in_voices(performance, RATE),
        session_config(percent, false),
    )
    .expect("session");
    session
        .handle
        .send(TransportCommand::PlayRange(range))
        .expect("send");
    let length = usize::try_from(frames).expect("length");
    let mut left = vec![0.0f32; length];
    let mut right = vec![0.0f32; length];
    let mut done = 0;
    let mut blocks = ODD_BLOCKS.iter().copied().cycle();
    while done < length {
        let size = blocks.next().unwrap_or(1).min(length - done);
        session
            .transport
            .process(&mut left[done..done + size], &mut right[done..done + size]);
        done += size;
    }
    RenderedAudio {
        rate: RATE,
        frames,
        left,
        right,
    }
}

#[test]
fn a_play_range_sounds_like_a_pause_sent_at_its_end_in_any_block_split() {
    for (name, percent, range) in [
        ("repeats.gp", 100u16, BarRange { first: 1, last: 1 }),
        ("note-effects.gp", 200, BarRange { first: 2, last: 3 }),
        ("multi-track.gp", 131, BarRange { first: 0, last: 1 }),
        ("tempo-ramp.gp", 75, BarRange { first: 3, last: 3 }),
    ] {
        let performance = performance(&fixture(name), Profile::Musical);
        let end = range_frames(&performance, range, speed(percent), RATE).expect("frames");
        let expected = reference(&performance, range, percent, end);
        let got = ranged(&performance, range, percent, end + AFTER_FRAMES);
        assert!(got.left.iter().any(|sample| *sample != 0.0), "{name}");
        assert!(got.left == expected.left, "{name}: left differs");
        assert!(got.right == expected.right, "{name}: right differs");
    }
}

#[test]
fn a_counted_in_play_range_pauses_where_its_next_bar_would_start() {
    for (name, percent, range) in [
        ("repeats.gp", 200u16, BarRange { first: 2, last: 2 }),
        ("note-effects.gp", 175, BarRange { first: 1, last: 2 }),
    ] {
        for performance in [
            performance(&fixture(name), Profile::Musical),
            with_pickup(name),
        ] {
            let music = range_frames(&performance, range, speed(percent), RATE).expect("frames");
            let mut run = stepped(
                &performance,
                session_config(percent, true),
                TransportCommand::PlayRange(range),
            );
            let mut previous = run.handle.status();
            let mut counted = false;
            let mut paused_at = None;
            for frame in 1..=(3 * music + AFTER_FRAMES) {
                assert!(run.output.step(1));
                let status = run.handle.status();
                counted |= status.state == TransportState::CountIn;
                if status.state == TransportState::Paused {
                    assert_eq!(previous.state, TransportState::Playing, "{name}");
                    assert_eq!(previous.played_bar, range.last, "{name}");
                    assert_eq!(status.played_bar, range.last + 1, "{name}");
                    paused_at = Some(frame);
                    break;
                }
                previous = status;
            }
            let paused_at = paused_at.expect("the range ends");
            assert!(counted, "{name}");
            assert!(paused_at > music, "{name}");
            assert!(run.output.step(AFTER_FRAMES));
            assert!(!note_on_from(&run.logs, paused_at), "{name}");
        }
    }
}

#[test]
fn a_play_range_to_the_last_bar_ends_like_the_song() {
    let performance = performance(&fixture("repeats.gp"), Profile::Musical);
    let last = performance.order.bars.len() - 1;
    let range = BarRange {
        first: last - 1,
        last,
    };
    let music = range_frames(&performance, range, speed(200), RATE).expect("frames");
    let mut run = stepped(
        &performance,
        session_config(200, false),
        TransportCommand::PlayRange(range),
    );
    assert!(run.output.step(1));
    assert_eq!(run.handle.status().played_bar, last - 1);
    assert!(run.output.step(music + AFTER_FRAMES));
    let status = run.handle.status();
    assert_eq!(status.state, TransportState::Stopped);
    assert_eq!(status.played_bar, 0);
}

#[test]
fn a_bad_play_range_is_ignored() {
    let performance = performance(&fixture("repeats.gp"), Profile::Musical);
    let bars = performance.order.bars.len();
    for range in [
        BarRange { first: 2, last: 1 },
        BarRange {
            first: 0,
            last: bars,
        },
        BarRange {
            first: bars,
            last: bars,
        },
    ] {
        let mut run = stepped(
            &performance,
            session_config(100, false),
            TransportCommand::PlayRange(range),
        );
        assert!(run.output.step(4_096));
        let status = run.handle.status();
        assert_eq!(status.state, TransportState::Stopped, "{range:?}");
        assert_eq!(status.frame, 4_096);
    }
}

#[test]
fn play_after_a_range_goes_on_and_stop_inside_it_returns_to_its_start() {
    let performance = performance(&fixture("repeats.gp"), Profile::Musical);
    let range = BarRange { first: 1, last: 1 };
    let music = range_frames(&performance, range, speed(200), RATE).expect("frames");
    let mut run = stepped(
        &performance,
        session_config(200, false),
        TransportCommand::PlayRange(range),
    );
    assert!(run.output.step(music));
    assert_eq!(run.handle.status().state, TransportState::Paused);
    run.handle.send(TransportCommand::Play).expect("send");
    assert!(run.output.step(music));
    let status = run.handle.status();
    assert_eq!(status.state, TransportState::Playing);
    assert!(status.played_bar > range.last);

    run.handle
        .send(TransportCommand::PlayRange(range))
        .expect("send");
    assert!(run.output.step(music / 2));
    run.handle.send(TransportCommand::Stop).expect("send");
    assert!(run.output.step(1));
    let status = run.handle.status();
    assert_eq!(status.state, TransportState::Stopped);
    assert_eq!(status.played_bar, range.first);
    run.handle.send(TransportCommand::Play).expect("send");
    assert!(run.output.step(music));
    assert_eq!(run.handle.status().state, TransportState::Paused);
}
