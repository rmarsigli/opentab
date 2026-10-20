use opentab_audio::{
    AudioOutput, CURSOR_STEP, NullClock, NullOutput, OutputError, SampleRate, Session,
    TransportCommand, TransportHandle, TransportState, TransportStatus,
};
use opentab_playback::{Performance, Profile, Seconds, Speed};

use crate::support::{built_in_voices, config, fixture, performance};

const RATE: SampleRate = SampleRate::STUDIO;
const PERCENT: i128 = 100;
const STEPS: [u64; 9] = [1, 7, 511, 512, 513, 4_800, 48_000, 1, 100_000];

fn started(performance: &Performance) -> (NullOutput, TransportHandle) {
    let session = Session::new(
        performance.clone(),
        built_in_voices(performance, RATE),
        config(RATE),
    )
    .expect("session");
    let (transport, handle) = session.split();
    let mut output = NullOutput::manual(RATE);
    assert_eq!(output.clock(), NullClock::Manual);
    output.start(transport).expect("start");
    (output, handle)
}

fn expected_cursor(
    performance: &Performance,
    position: u64,
) -> (usize, usize, opentab_model::Fraction) {
    let reached = position + u64::from(Speed::NORMAL.percent()) - 1;
    let seconds =
        Seconds::new(i128::from(reached), i128::from(RATE.hz()) * PERCENT).expect("seconds");
    let time = performance
        .tempo
        .time_at_floor(seconds, Speed::NORMAL, CURSOR_STEP)
        .expect("time");
    let found = performance
        .order
        .position_at(time)
        .expect("inside the song");
    (found.played_bar, found.source_bar, found.offset)
}

fn cursor(status: &TransportStatus) -> (usize, usize, opentab_model::Fraction) {
    (status.played_bar, status.source_bar, status.offset)
}

#[test]
fn the_reported_frame_advances_by_exactly_the_stepped_frames_and_the_cursor_follows_the_play_order()
{
    let performance = performance(&fixture("repeats.gp"), Profile::Musical);
    let (mut output, mut handle) = started(&performance);
    assert_eq!(handle.status().state, TransportState::Stopped);
    handle.send(TransportCommand::Play).expect("send");
    let mut total = 0;
    let mut bars = Vec::new();
    for frames in STEPS {
        let before = handle.status().frame;
        assert!(output.step(frames));
        total += frames;
        let status = handle.status();
        assert_eq!(status.frame, before + frames);
        assert_eq!(status.frame, total);
        assert_eq!(status.state, TransportState::Playing);
        assert_eq!(cursor(&status), expected_cursor(&performance, total * 100));
        bars.push(status.played_bar);
    }
    assert!(bars.windows(2).all(|pair| pair[0] <= pair[1]));
    assert!(bars.last().copied().unwrap_or(0) > 0);
    let sources = performance.order.sources();
    let last = handle.status();
    assert_eq!(last.source_bar, sources[last.played_bar]);
}

#[test]
fn pause_holds_the_cursor_and_stop_returns_to_the_first_bar() {
    let performance = performance(&fixture("repeats.gp"), Profile::Musical);
    let (mut output, mut handle) = started(&performance);
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
    let (mut output, _handle) = started(&performance);
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
