use num_rational::Ratio;
use opentab_audio::{
    AudioOutput, CURSOR_STEP, NullClock, NullOutput, OutputError, SampleRate, Session,
    SessionConfig, TransportCommand, TransportHandle, TransportState, TransportStatus, frame_at,
};
use opentab_model::Fraction;
use opentab_playback::{Performance, PlayedPosition, Profile, ScoreTime, Speed};

use crate::support::{built_in_voices, config, fixture, performance};

const RATE: SampleRate = SampleRate::STUDIO;
const PERCENT: i128 = 100;
const STEPS: [u64; 9] = [1, 7, 511, 512, 513, 4_800, 48_000, 1, 100_000];
const STEP_MARGIN: u64 = 2;

struct Started {
    output: NullOutput,
    handle: TransportHandle,
}

fn started_at(performance: &Performance, percent: u16) -> Started {
    let session = Session::new(
        performance.clone(),
        built_in_voices(performance, RATE),
        SessionConfig {
            speed: Speed::new(percent).expect("speed"),
            ..config(RATE)
        },
    )
    .expect("session");
    let (transport, handle) = session.split();
    let mut output = NullOutput::manual(RATE);
    assert_eq!(output.clock(), NullClock::Manual);
    output.start(transport).expect("start");
    Started { output, handle }
}

fn started(performance: &Performance) -> Started {
    started_at(performance, 100)
}

fn seconds_of(performance: &Performance, whole_notes: Fraction) -> Ratio<i128> {
    let time = ScoreTime::new(whole_notes).expect("time");
    let seconds = performance
        .tempo
        .seconds_at(time, Speed::NORMAL)
        .expect("seconds");
    Ratio::new(seconds.numer(), seconds.denom())
}

fn heard_by(performance: &Performance, whole_notes: Fraction, frames: u64, percent: u16) -> bool {
    let reached = Ratio::from_integer(i128::from(frames + 1) * i128::from(percent))
        / Ratio::from_integer(i128::from(RATE.hz()) * PERCENT);
    seconds_of(performance, whole_notes) < reached
}

fn assert_cursor(performance: &Performance, status: &TransportStatus, frames: u64, percent: u16) {
    let bars = &performance.order.bars;
    let expected = bars
        .iter()
        .rposition(|bar| heard_by(performance, bar.start.whole_notes(), frames, percent))
        .expect("the first bar starts at once");
    let label = format!("{percent}% after {frames} frames");
    assert_eq!(status.played_bar, expected, "{label}");
    let bar = &bars[expected];
    assert_eq!(status.source_bar, bar.source, "{label}");
    assert!(status.offset < bar.length, "{label}");
    let at = bar.start.whole_notes() + status.offset;
    assert!(heard_by(performance, at, frames, percent), "{label}");
    let next = status.offset + CURSOR_STEP;
    assert!(
        next >= bar.length
            || !heard_by(performance, bar.start.whole_notes() + next, frames, percent),
        "{label}"
    );
}

fn cursor(status: &TransportStatus) -> PlayedPosition {
    PlayedPosition {
        played_bar: status.played_bar,
        source_bar: status.source_bar,
        offset: status.offset,
    }
}

#[test]
fn the_reported_frame_advances_by_exactly_the_stepped_frames_and_the_cursor_follows_the_play_order()
{
    for (name, percent) in [
        ("repeats.gp", 100u16),
        ("repeats.gp", 25),
        ("repeats.gp", 200),
        ("tempo.gp", 75),
        ("tempo-ramp.gp", 131),
    ] {
        let performance = performance(&fixture(name), Profile::Musical);
        let Started {
            mut output,
            mut handle,
        } = started_at(&performance, percent);
        assert_eq!(handle.status().state, TransportState::Stopped);
        handle.send(TransportCommand::Play).expect("send");
        let song = frame_at(
            &performance.tempo,
            performance.order.end(),
            Speed::new(percent).expect("speed"),
            RATE,
        )
        .expect("end");
        let mut total = 0;
        let mut bars = Vec::new();
        let mut steps = STEPS.iter().copied().cycle();
        while total + STEP_MARGIN < song {
            let frames = steps.next().unwrap_or(1).min(song - STEP_MARGIN - total);
            let before = handle.status().frame;
            assert!(output.step(frames));
            total += frames;
            let status = handle.status();
            assert_eq!(status.frame, before + frames);
            assert_eq!(status.frame, total);
            assert_eq!(status.state, TransportState::Playing);
            assert_cursor(&performance, &status, total, percent);
            bars.push(status.played_bar);
        }
        assert!(bars.windows(2).all(|pair| pair[0] <= pair[1]), "{name}");
        assert_eq!(
            bars.last().copied(),
            Some(performance.order.bars.len() - 1),
            "{name}"
        );
    }
}

#[test]
fn pause_holds_the_cursor_and_stop_returns_to_the_first_bar() {
    let performance = performance(&fixture("repeats.gp"), Profile::Musical);
    let Started {
        mut output,
        mut handle,
    } = started(&performance);
    handle.send(TransportCommand::Play).expect("send");
    output.step(150_000);
    handle.send(TransportCommand::Pause).expect("send");
    output.step(1);
    let paused = handle.status();
    assert_eq!(paused.state, TransportState::Paused);
    output.step(48_000);
    let held = handle.status();
    assert_eq!(cursor(&held), cursor(&paused));
    assert_eq!(held.frame, paused.frame + 48_000);
    handle.send(TransportCommand::Play).expect("send");
    output.step(1);
    assert_eq!(handle.status().state, TransportState::Playing);
    handle.send(TransportCommand::Seek(3)).expect("send");
    output.step(1);
    assert_eq!(handle.status().played_bar, 3);
    handle.send(TransportCommand::Stop).expect("send");
    output.step(1);
    let stopped = handle.status();
    assert_eq!(stopped.state, TransportState::Stopped);
    assert_eq!(
        (stopped.played_bar, stopped.offset),
        (0, opentab_model::Fraction::new_raw(0, 1))
    );
    let transport = output.stop().expect("the transport comes back");
    assert_eq!(transport.clock(), stopped.frame);
    assert!(!output.step(1));
}

#[test]
fn an_output_refuses_a_second_start_and_a_transport_at_another_rate() {
    let performance = performance(&fixture("durations.gp"), Profile::Musical);
    let Started { mut output, .. } = started(&performance);
    let other = Session::new(
        performance.clone(),
        built_in_voices(&performance, RATE),
        config(RATE),
    )
    .expect("session");
    assert_eq!(
        output.start(other.split().0).expect_err("already running"),
        OutputError::AlreadyRunning
    );
    let cd = Session::new(
        performance.clone(),
        built_in_voices(&performance, SampleRate::CD),
        config(SampleRate::CD),
    )
    .expect("session");
    let mut fresh = NullOutput::manual(RATE);
    assert_eq!(
        fresh.start(cd.split().0).expect_err("rate"),
        OutputError::RateMismatch {
            transport: 44_100,
            output: 48_000
        }
    );
}

#[test]
fn the_real_time_null_output_starts_and_returns_its_transport_when_stopped() {
    let performance = performance(&fixture("durations.gp"), Profile::Musical);
    let session = Session::new(
        performance.clone(),
        built_in_voices(&performance, RATE),
        config(RATE),
    )
    .expect("session");
    let (transport, mut handle) = session.split();
    let mut output = NullOutput::real_time(RATE);
    assert_eq!(output.clock(), NullClock::RealTime);
    output.start(transport).expect("start");
    assert!(output.is_running());
    assert!(!output.step(1));
    handle.send(TransportCommand::Play).expect("send");
    let first = handle.status().frame;
    let second = handle.status().frame;
    assert!(second >= first);
    let transport = output.stop().expect("the transport comes back");
    assert!(!output.is_running());
    assert!(transport.clock() >= second);
    assert_eq!(output.stop().map(|transport| transport.clock()), None);
}

#[test]
fn the_status_reports_the_count_in_until_the_first_bar_starts() {
    let performance = performance(&fixture("repeats.gp"), Profile::Musical);
    let config = opentab_audio::SessionConfig {
        count_in: true,
        ..config(RATE)
    };
    let session = Session::new(
        performance.clone(),
        built_in_voices(&performance, RATE),
        config,
    )
    .expect("session");
    let (transport, mut handle) = session.split();
    let mut output = NullOutput::manual(RATE);
    output.start(transport).expect("start");
    handle.send(TransportCommand::Play).expect("send");
    let bar = opentab_audio::range_frames(
        &performance,
        opentab_audio::BarRange { first: 0, last: 0 },
        Speed::NORMAL,
        RATE,
    )
    .expect("bar");
    output.step(bar);
    let counting = handle.status();
    assert_eq!(counting.state, TransportState::CountIn);
    assert_eq!(
        (counting.played_bar, counting.offset),
        (0, opentab_model::Fraction::new_raw(0, 1))
    );
    output.step(1);
    assert_eq!(handle.status().state, TransportState::Playing);
}

fn downbeat_frame(performance: &Performance, bar: usize, percent: u16) -> u64 {
    let units = seconds_of(performance, performance.order.bars[bar].start.whole_notes())
        * Ratio::from_integer(i128::from(RATE.hz()) * PERCENT)
        / Ratio::from_integer(i128::from(percent));
    u64::try_from(units.floor().to_integer()).expect("frame")
}

#[test]
fn the_cursor_turns_to_each_bar_in_the_frame_of_its_downbeat() {
    for (name, percent) in [
        ("repeats.gp", 100u16),
        ("repeats.gp", 25),
        ("signatures.gp", 200),
        ("tempo-ramp.gp", 131),
        ("tempo.gp", 75),
    ] {
        let performance = performance(&fixture(name), Profile::Musical);
        let Started {
            mut output,
            mut handle,
        } = started_at(&performance, percent);
        handle.send(TransportCommand::Play).expect("send");
        let mut total = 0;
        for bar in 1..performance.order.bars.len() {
            let downbeat = downbeat_frame(&performance, bar, percent);
            assert!(downbeat > total, "{name} bar {bar}");
            output.step(downbeat - 1 - total);
            let before = handle.status();
            assert_eq!(
                before.played_bar,
                bar - 1,
                "{name} {percent}% before bar {bar}"
            );
            assert_cursor(&performance, &before, downbeat - 1, percent);
            output.step(1);
            let after = handle.status();
            assert_eq!(after.played_bar, bar, "{name} {percent}% at bar {bar}");
            assert_cursor(&performance, &after, downbeat, percent);
            total = downbeat;
        }
    }
}
