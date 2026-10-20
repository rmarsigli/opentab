use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use opentab_audio::{
    AudioOutput, BarRange, QueueFull, Session, TransportCommand, TransportHandle, TransportState,
    TransportStatus,
};

use crate::error::CliError;

const SETTLE_POLLS: u32 = 20;

pub trait Device: AudioOutput {
    fn has_failed(&self) -> bool;
    fn pause(&mut self, span: Duration);
}

#[derive(Debug, Clone, Copy)]
pub struct Pacing {
    pub poll: Duration,
    pub tail: Duration,
}

#[derive(Debug, Clone)]
pub struct Start {
    pub range: Option<BarRange>,
    pub solo: Vec<usize>,
}

fn send(handle: &mut TransportHandle, command: TransportCommand) -> Result<(), CliError> {
    handle
        .send(command)
        .map_err(|error: QueueFull| CliError::Audio {
            message: error.to_string(),
        })
}

fn halt<D: Device>(handle: &mut TransportHandle, device: &mut D, pacing: Pacing) -> CliError {
    let _ = handle.send(TransportCommand::Stop);
    for _ in 0..SETTLE_POLLS {
        if handle.status().state == TransportState::Stopped || device.has_failed() {
            break;
        }
        device.pause(pacing.poll);
    }
    let _ = device.stop();
    CliError::Interrupted
}

fn finished(status: &TransportStatus) -> bool {
    match status.state {
        TransportState::Stopped => status.frame > 0,
        TransportState::Paused => true,
        TransportState::Playing | TransportState::CountIn => false,
    }
}

fn commands(start: &Start) -> Vec<TransportCommand> {
    let mut commands: Vec<TransportCommand> = start
        .solo
        .iter()
        .map(|track| TransportCommand::Solo(*track, true))
        .collect();
    commands.push(
        start
            .range
            .map_or(TransportCommand::Play, TransportCommand::PlayRange),
    );
    commands
}

fn wait<D: Device>(
    handle: &mut TransportHandle,
    device: &mut D,
    interrupt: &AtomicBool,
    pacing: Pacing,
) -> Result<u64, CliError> {
    loop {
        if interrupt.load(Ordering::SeqCst) {
            return Err(halt(handle, device, pacing));
        }
        if finished(&handle.status()) {
            break;
        }
        if device.has_failed() {
            return Err(CliError::Audio {
                message: "the audio stream stopped unexpectedly".to_string(),
            });
        }
        device.pause(pacing.poll);
    }
    let mut left = pacing.tail;
    while !left.is_zero() {
        if interrupt.load(Ordering::SeqCst) {
            return Err(halt(handle, device, pacing));
        }
        let span = left.min(pacing.poll);
        device.pause(span);
        left -= span;
    }
    Ok(handle.status().frame)
}

pub fn perform<D: Device>(
    device: &mut D,
    session: Session,
    start: &Start,
    interrupt: &AtomicBool,
    pacing: Pacing,
) -> Result<u64, CliError> {
    let (transport, mut handle) = session.split();
    for command in commands(start) {
        send(&mut handle, command)?;
    }
    device.start(transport)?;
    let frames = wait(&mut handle, device, interrupt, pacing)?;
    let _ = device.stop();
    Ok(frames)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use clap::Parser;
    use opentab_audio::{BankFormat, NullOutput, OutputError, SampleRate, Transport};
    use opentab_files::{self as files, Overwrite};
    use opentab_model::blank_score;

    use super::*;
    use crate::cli::{Cli, Subcommand};
    use crate::error::EXIT_INTERRUPTED;
    use crate::sound::{built_in_voices, prepare_with};

    const POLL: Duration = Duration::from_millis(50);
    const PAUSE_LIMIT: u32 = 100_000;
    const FRAMES_PER_PAUSE: u64 = 2_400;
    const ODD_FRAMES: u64 = 7_000;

    struct Trip {
        after: u32,
        flag: Arc<AtomicBool>,
    }

    struct Stepped {
        output: NullOutput,
        frames: u64,
        pauses: u32,
        trip: Option<Trip>,
        stopped: bool,
    }

    impl Stepped {
        fn new(frames: u64, trip: Option<Trip>) -> Stepped {
            Stepped {
                output: NullOutput::manual(SampleRate::default()),
                frames,
                pauses: 0,
                trip,
                stopped: false,
            }
        }
    }

    impl AudioOutput for Stepped {
        fn rate(&self) -> SampleRate {
            self.output.rate()
        }

        fn start(&mut self, transport: Transport) -> Result<(), OutputError> {
            self.output.start(transport)
        }

        fn stop(&mut self) -> Option<Transport> {
            self.stopped = true;
            self.output.stop()
        }
    }

    impl Device for Stepped {
        fn has_failed(&self) -> bool {
            false
        }

        fn pause(&mut self, _span: Duration) {
            self.pauses += 1;
            assert!(self.pauses < PAUSE_LIMIT, "the wait never ended");
            self.output.step(self.frames);
            if let Some(trip) = &self.trip
                && self.pauses == trip.after
            {
                trip.flag.store(true, Ordering::SeqCst);
            }
        }
    }

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/gp")
            .join(name)
    }

    fn start_of(path: &Path, extra: &[&str]) -> (Session, Start) {
        let mut args = vec!["opentab", "play", path.to_str().expect("utf-8")];
        args.extend(extra);
        let Subcommand::Play { playback } = Cli::try_parse_from(args).expect("arguments").command
        else {
            panic!("not a play command");
        };
        let rate = SampleRate::default();
        let prepared = prepare_with(&playback, rate, |score| {
            Ok(built_in_voices(score.tracks.len(), rate, None))
        })
        .expect("prepared");
        let start = Start {
            range: prepared.range,
            solo: Vec::new(),
        };
        assert_eq!(
            prepared.backends[0].format,
            BankFormat::BuiltIn,
            "built-in voices"
        );
        (prepared.session, start)
    }

    fn pacing(tail: Duration) -> Pacing {
        Pacing { poll: POLL, tail }
    }

    fn short_score() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("short.opentab.json");
        files::write_score(&path, &blank_score().expect("blank"), Overwrite::Refused)
            .expect("write");
        (dir, path)
    }

    #[test]
    fn a_score_shorter_than_one_poll_ends_after_that_poll() {
        let (_dir, path) = short_score();
        let (session, start) = start_of(&path, &[]);
        let mut device = Stepped::new(100_000, None);
        let interrupt = AtomicBool::new(false);
        let frames = perform(
            &mut device,
            session,
            &start,
            &interrupt,
            pacing(Duration::ZERO),
        )
        .expect("played");
        assert_eq!(device.pauses, 1);
        assert!(frames >= 96_000, "{frames}");
        assert!(device.stopped);
    }

    #[test]
    fn a_score_that_ended_before_the_first_poll_needs_no_poll() {
        let (_dir, path) = short_score();
        let (session, _start) = start_of(&path, &[]);
        let mut device = Stepped::new(100_000, None);
        let (transport, mut handle) = session.split();
        handle.send(TransportCommand::Play).expect("queued");
        device.start(transport).expect("started");
        device.output.step(100_000);
        assert_eq!(handle.status().state, TransportState::Stopped);
        let interrupt = AtomicBool::new(false);
        let frames =
            wait(&mut handle, &mut device, &interrupt, pacing(Duration::ZERO)).expect("waited");
        assert_eq!(device.pauses, 0);
        assert!(frames > 0);
    }

    #[test]
    fn the_tail_is_waited_out_in_polls() {
        let (_dir, path) = short_score();
        let (session, start) = start_of(&path, &[]);
        let mut device = Stepped::new(100_000, None);
        let interrupt = AtomicBool::new(false);
        perform(
            &mut device,
            session,
            &start,
            &interrupt,
            pacing(POLL * 7 + Duration::from_millis(1)),
        )
        .expect("played");
        assert_eq!(device.pauses, 1 + 8);
    }

    #[test]
    fn an_interrupt_stops_the_transport_and_the_output_and_exits_130() {
        let path = fixture("repeats.gp");
        let (session, start) = start_of(&path, &[]);
        let flag = Arc::new(AtomicBool::new(false));
        let trip = Trip {
            after: 3,
            flag: Arc::clone(&flag),
        };
        let mut device = Stepped::new(FRAMES_PER_PAUSE, Some(trip));
        let outcome = perform(
            &mut device,
            session,
            &start,
            &flag,
            pacing(Duration::from_secs(3)),
        );
        let error = outcome.expect_err("interrupted");
        assert!(matches!(error, CliError::Interrupted));
        assert_eq!(error.exit_code(), EXIT_INTERRUPTED);
        assert_eq!(error.exit_code(), 130);
        assert_eq!(error.body().kind, "interrupted");
        assert!(device.stopped);
        assert!(device.pauses >= 3, "{}", device.pauses);
        assert!(device.pauses < 3 + SETTLE_POLLS, "{}", device.pauses);
    }

    #[test]
    fn a_stop_is_applied_before_the_output_is_released() {
        let path = fixture("repeats.gp");
        let (session, _start) = start_of(&path, &[]);
        let (transport, mut handle) = session.split();
        let mut device = Stepped::new(FRAMES_PER_PAUSE, None);
        handle.send(TransportCommand::Play).expect("queued");
        device.start(transport).expect("started");
        device.pause(POLL);
        assert_eq!(handle.status().state, TransportState::Playing);
        let flag = AtomicBool::new(true);
        let error = wait(&mut handle, &mut device, &flag, pacing(POLL)).expect_err("interrupted");
        assert!(matches!(error, CliError::Interrupted));
        assert_eq!(handle.status().state, TransportState::Stopped);
        assert!(device.stopped);
    }

    #[test]
    fn an_interrupt_during_the_tail_still_exits_130() {
        let (_dir, path) = short_score();
        let (session, start) = start_of(&path, &[]);
        let flag = Arc::new(AtomicBool::new(false));
        let trip = Trip {
            after: 3,
            flag: Arc::clone(&flag),
        };
        let mut device = Stepped::new(100_000, Some(trip));
        let outcome = perform(
            &mut device,
            session,
            &start,
            &flag,
            pacing(Duration::from_secs(3)),
        );
        assert!(matches!(outcome, Err(CliError::Interrupted)));
        assert!(device.pauses < 20, "{}", device.pauses);
    }

    #[test]
    fn the_start_solos_then_plays_the_range_or_the_whole_score() {
        let range = BarRange { first: 1, last: 2 };
        let ranged = Start {
            range: Some(range),
            solo: vec![0, 2],
        };
        assert_eq!(
            commands(&ranged),
            vec![
                TransportCommand::Solo(0, true),
                TransportCommand::Solo(2, true),
                TransportCommand::PlayRange(range),
            ]
        );
        let whole = Start {
            range: None,
            solo: Vec::new(),
        };
        assert_eq!(commands(&whole), vec![TransportCommand::Play]);
    }

    #[test]
    fn a_ranged_play_is_paused_by_the_transport_at_its_last_bar() {
        let path = fixture("repeats.gp");
        for (frames_per_pause, until) in [(FRAMES_PER_PAUSE, 96_000), (ODD_FRAMES, 98_000)] {
            let (session, start) = start_of(&path, &["--bars", "2"]);
            let (transport, mut handle) = session.split();
            let mut device = Stepped::new(frames_per_pause, None);
            for command in commands(&start) {
                handle.send(command).expect("queued");
            }
            device.start(transport).expect("started");
            let interrupt = AtomicBool::new(false);
            let frames =
                wait(&mut handle, &mut device, &interrupt, pacing(Duration::ZERO)).expect("waited");
            assert_eq!(handle.status().state, TransportState::Paused);
            assert_eq!(frames, until, "{frames_per_pause}");
        }
    }
}
