use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread::JoinHandle;
use std::time::Duration;

use opentab_audio::{
    AudioOutput, OutputError, RENDER_TAIL_MS, SampleRate, Transport, TransportHandle,
    TransportState, TransportStatus,
};
use serde::Serialize;

use crate::error::ToolError;

pub const RELEASE_POLL_MILLIS: u64 = 100;

pub type OutputOpener = Box<dyn FnOnce() -> Result<Box<dyn AudioOutput>, OutputError> + Send>;

#[derive(Serialize)]
pub struct StatusWire {
    pub state: &'static str,
    pub session: Option<String>,
    pub played_bar: Option<usize>,
    pub source_bar: Option<usize>,
    pub offset: Option<String>,
    pub frame: Option<u64>,
}

pub fn state_name(state: TransportState) -> &'static str {
    match state {
        TransportState::Stopped => "stopped",
        TransportState::Playing => "playing",
        TransportState::Paused => "paused",
        TransportState::CountIn => "count_in",
    }
}

impl StatusWire {
    pub fn idle() -> StatusWire {
        StatusWire {
            state: "idle",
            session: None,
            played_bar: None,
            source_bar: None,
            offset: None,
            frame: None,
        }
    }

    pub fn of(session: &str, status: &TransportStatus) -> StatusWire {
        StatusWire {
            state: state_name(status.state),
            session: Some(session.to_owned()),
            played_bar: Some(status.played_bar + 1),
            source_bar: Some(status.source_bar + 1),
            offset: Some(status.offset.to_string()),
            frame: Some(status.frame),
        }
    }
}

pub fn finished(status: &TransportStatus) -> bool {
    matches!(
        status.state,
        TransportState::Stopped | TransportState::Paused
    )
}

pub struct Launch {
    pub transport: Transport,
    pub handle: TransportHandle,
}

enum Message {
    Launch(Box<Launch>, Sender<Result<(), OutputError>>),
    Status(Sender<Option<TransportStatus>>),
    Stop(Sender<Option<TransportStatus>>),
}

struct Running {
    handle: TransportHandle,
    ended_at: Option<u64>,
}

enum Playback {
    Idle,
    Running(Running),
    Released(TransportStatus),
}

struct Service {
    output: Box<dyn AudioOutput>,
    playback: Playback,
    tail: u64,
}

impl Service {
    fn settle(&mut self) {
        let Playback::Running(run) = &mut self.playback else {
            return;
        };
        let status = run.handle.status();
        if !finished(&status) {
            run.ended_at = None;
            return;
        }
        let ended = *run.ended_at.get_or_insert(status.frame);
        if status.frame >= ended.saturating_add(self.tail) {
            self.output.stop();
            self.playback = Playback::Released(status);
        }
    }

    fn status(&mut self) -> Option<TransportStatus> {
        self.settle();
        match &self.playback {
            Playback::Idle => None,
            Playback::Running(run) => Some(run.handle.status()),
            Playback::Released(last) => Some(*last),
        }
    }

    fn launch(&mut self, launch: Launch) -> Result<(), OutputError> {
        let Launch { transport, handle } = launch;
        self.output.start(transport)?;
        self.playback = Playback::Running(Running {
            handle,
            ended_at: None,
        });
        Ok(())
    }

    fn next(&self, messages: &Receiver<Message>) -> Result<Message, RecvTimeoutError> {
        match self.playback {
            Playback::Running(_) => {
                messages.recv_timeout(Duration::from_millis(RELEASE_POLL_MILLIS))
            }
            Playback::Idle | Playback::Released(_) => {
                messages.recv().map_err(|_| RecvTimeoutError::Disconnected)
            }
        }
    }
}

#[cfg(feature = "live")]
fn open_output() -> Result<Box<dyn AudioOutput>, OutputError> {
    Ok(Box::new(opentab_audio::CpalOutput::open()?))
}

#[cfg(not(feature = "live"))]
fn open_output() -> Result<Box<dyn AudioOutput>, OutputError> {
    Ok(Box::new(opentab_audio::NullOutput::real_time(
        SampleRate::default(),
    )))
}

fn serve(
    open: OutputOpener,
    ready: Sender<Result<SampleRate, OutputError>>,
    messages: Receiver<Message>,
) {
    let output = match open() {
        Ok(output) => output,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    let rate = output.rate();
    if ready.send(Ok(rate)).is_err() {
        return;
    }
    let mut service = Service {
        output,
        playback: Playback::Idle,
        tail: rate.frames_of_millis(RENDER_TAIL_MS),
    };
    loop {
        match service.next(&messages) {
            Ok(Message::Launch(launch, done)) => {
                let _ = done.send(service.launch(*launch));
            }
            Ok(Message::Status(reply)) => {
                let _ = reply.send(service.status());
            }
            Ok(Message::Stop(reply)) => {
                let last = service.status();
                service.output.stop();
                let _ = reply.send(last);
                return;
            }
            Err(RecvTimeoutError::Timeout) => service.settle(),
            Err(RecvTimeoutError::Disconnected) => {
                service.output.stop();
                return;
            }
        }
    }
}

pub struct Worker {
    messages: Sender<Message>,
    thread: Option<JoinHandle<()>>,
    rate: SampleRate,
}

fn gone() -> ToolError {
    ToolError::new("audio", "the audio output stopped unexpectedly")
}

impl Worker {
    pub fn open() -> Result<Worker, ToolError> {
        Worker::open_with(Box::new(open_output))
    }

    pub fn open_with(open: OutputOpener) -> Result<Worker, ToolError> {
        let (messages, receiver) = channel();
        let (ready, rate) = channel();
        let thread = std::thread::Builder::new()
            .name("opentab-mcp-output".to_owned())
            .spawn(move || serve(open, ready, receiver))
            .map_err(|_| ToolError::from(OutputError::Thread))?;
        let rate = rate.recv().map_err(|_| gone())??;
        Ok(Worker {
            messages,
            thread: Some(thread),
            rate,
        })
    }

    pub fn rate(&self) -> SampleRate {
        self.rate
    }

    pub fn launch(&self, launch: Launch) -> Result<(), ToolError> {
        let (done, outcome) = channel();
        self.messages
            .send(Message::Launch(Box::new(launch), done))
            .map_err(|_| gone())?;
        outcome.recv().map_err(|_| gone())??;
        Ok(())
    }

    pub fn status(&self) -> Result<Option<TransportStatus>, ToolError> {
        let (reply, status) = channel();
        self.messages
            .send(Message::Status(reply))
            .map_err(|_| gone())?;
        status.recv().map_err(|_| gone())
    }

    pub fn stop(mut self) -> Option<TransportStatus> {
        let (reply, status) = channel();
        let last = match self.messages.send(Message::Stop(reply)) {
            Ok(()) => status.recv().ok().flatten(),
            Err(_) => None,
        };
        self.join();
        last
    }

    fn join(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let (reply, _status) = channel();
        let _ = self.messages.send(Message::Stop(reply));
        self.join();
    }
}

pub struct Active {
    pub worker: Worker,
    pub session: String,
}

#[derive(Default)]
pub struct Player {
    pub active: Option<Active>,
}

impl Player {
    pub fn status(&mut self) -> Result<StatusWire, ToolError> {
        let Some(active) = &self.active else {
            return Ok(StatusWire::idle());
        };
        match active.worker.status() {
            Ok(Some(status)) => Ok(StatusWire::of(&active.session, &status)),
            Ok(None) => Ok(StatusWire::idle()),
            Err(error) => {
                self.active = None;
                Err(error)
            }
        }
    }

    pub fn stop(&mut self) -> StatusWire {
        let Some(active) = self.active.take() else {
            return StatusWire::idle();
        };
        let session = active.session.clone();
        match active.worker.stop() {
            Some(last) => StatusWire {
                state: "stopped",
                ..StatusWire::of(&session, &last)
            },
            None => StatusWire {
                state: "stopped",
                session: Some(session),
                ..StatusWire::idle()
            },
        }
    }

    pub fn close(&mut self, session: &str) {
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.session == session)
        {
            self.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, PoisonError};

    use opentab_audio::{
        Bank, BarRange, BuiltInBank, Session, SessionConfig, TransportCommand, range_frames,
    };
    use opentab_model::Fraction;
    use opentab_playback::{PlaybackOptions, Profile, Speed, perform};

    use super::*;

    const BLOCK: usize = 512;
    const REPEATS_PLAYED_BARS: usize = 16;

    #[derive(Default)]
    struct Slot {
        transport: Option<Transport>,
        started: usize,
        stopped: usize,
    }

    type SharedSlot = Arc<Mutex<Slot>>;

    struct Stepped {
        slot: SharedSlot,
    }

    impl AudioOutput for Stepped {
        fn rate(&self) -> SampleRate {
            SampleRate::default()
        }

        fn start(&mut self, transport: Transport) -> Result<(), OutputError> {
            let mut slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
            slot.transport = Some(transport);
            slot.started += 1;
            Ok(())
        }

        fn stop(&mut self) -> Option<Transport> {
            let mut slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
            slot.stopped += 1;
            slot.transport.take()
        }
    }

    fn step(slot: &SharedSlot, frames: u64) {
        let mut slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(transport) = slot.transport.as_mut() else {
            return;
        };
        let mut left = [0.0f32; BLOCK];
        let mut right = [0.0f32; BLOCK];
        let mut rest = frames;
        while rest > 0 {
            let count = usize::try_from(rest).unwrap_or(BLOCK).min(BLOCK);
            transport.process(&mut left[..count], &mut right[..count]);
            rest -= count as u64;
        }
    }

    fn counts(slot: &SharedSlot) -> (usize, usize) {
        let slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
        (slot.started, slot.stopped)
    }

    fn stepped_worker() -> (Worker, SharedSlot) {
        let slot = SharedSlot::default();
        let shared = Arc::clone(&slot);
        let worker = Worker::open_with(Box::new(move || {
            Ok(Box::new(Stepped { slot: shared }) as Box<dyn AudioOutput>)
        }))
        .expect("a worker");
        (worker, slot)
    }

    struct Played {
        worker: Worker,
        slot: SharedSlot,
        music: u64,
        tail: u64,
    }

    fn played(start: TransportCommand, range: BarRange) -> Played {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/gp/repeats.gp");
        let score = opentab_gp::read_gp(&std::fs::read(path).unwrap()).unwrap();
        let performance = perform(
            &score,
            &PlaybackOptions {
                profile: Profile::Musical,
            },
        )
        .unwrap();
        let rate = SampleRate::default();
        let music = range_frames(&performance, range, Speed::NORMAL, rate).unwrap();
        let voices = performance
            .tracks
            .iter()
            .map(|_| BuiltInBank.instantiate(rate))
            .collect();
        let config = SessionConfig {
            rate,
            ..SessionConfig::default()
        };
        let (transport, mut handle) = Session::new(performance, voices, config).unwrap().split();
        handle.send(start).unwrap();
        let (worker, slot) = stepped_worker();
        worker.launch(Launch { transport, handle }).unwrap();
        Played {
            worker,
            slot,
            music,
            tail: rate.frames_of_millis(RENDER_TAIL_MS),
        }
    }

    fn status(state: TransportState, played_bar: usize) -> TransportStatus {
        TransportStatus {
            state,
            played_bar,
            source_bar: played_bar,
            offset: Fraction::new_raw(1, 4),
            frame: 7,
        }
    }

    #[test]
    fn playback_has_finished_once_the_transport_stopped_or_paused_itself() {
        assert!(finished(&status(TransportState::Stopped, 0)));
        assert!(finished(&status(TransportState::Paused, 3)));
        assert!(!finished(&status(TransportState::Playing, 3)));
        assert!(!finished(&status(TransportState::CountIn, 0)));
    }

    #[test]
    fn a_status_counts_bars_from_one_and_names_its_state() {
        let wire = StatusWire::of("session-1", &status(TransportState::CountIn, 0));
        assert_eq!(wire.state, "count_in");
        assert_eq!(wire.played_bar, Some(1));
        assert_eq!(wire.source_bar, Some(1));
        assert_eq!(wire.offset.as_deref(), Some("1/4"));
        assert_eq!(wire.frame, Some(7));
        let idle = StatusWire::idle();
        assert_eq!(idle.state, "idle");
        assert!(idle.frame.is_none() && idle.session.is_none());
    }

    #[test]
    fn a_ranged_play_pauses_at_its_end_and_releases_the_output_after_the_tail() {
        let range = BarRange { first: 1, last: 1 };
        let run = played(TransportCommand::PlayRange(range), range);
        step(&run.slot, run.music - 1);
        let playing = run.worker.status().unwrap().unwrap();
        assert_eq!(playing.state, TransportState::Playing);
        step(&run.slot, 1);
        let paused = run.worker.status().unwrap().unwrap();
        assert_eq!(paused.state, TransportState::Paused);
        assert_eq!(paused.frame, run.music);
        assert_eq!(counts(&run.slot), (1, 0));
        step(&run.slot, run.tail - 1);
        assert_eq!(counts(&run.slot), (1, 0));
        step(&run.slot, 1);
        let released = run.worker.status().unwrap().unwrap();
        assert_eq!(counts(&run.slot), (1, 1));
        assert_eq!(released.state, TransportState::Paused);
        assert_eq!(released.frame, run.music + run.tail);
        step(&run.slot, 4_800);
        assert_eq!(run.worker.status().unwrap(), Some(released));
        assert_eq!(run.worker.stop(), Some(released));
    }

    #[test]
    fn a_natural_end_releases_the_output_and_keeps_the_stopped_status() {
        let whole = BarRange {
            first: 0,
            last: REPEATS_PLAYED_BARS - 1,
        };
        let run = played(TransportCommand::Play, whole);
        step(&run.slot, run.music - 1);
        let mut ended = run.worker.status().unwrap().unwrap();
        assert_eq!(ended.state, TransportState::Playing);
        for _ in 0..run.tail {
            if ended.state == TransportState::Stopped {
                break;
            }
            step(&run.slot, 1);
            ended = run.worker.status().unwrap().unwrap();
        }
        assert_eq!(ended.state, TransportState::Stopped);
        assert_eq!(counts(&run.slot), (1, 0));
        step(&run.slot, run.tail - 1);
        assert_eq!(
            run.worker.status().unwrap().unwrap().state,
            TransportState::Stopped
        );
        assert_eq!(counts(&run.slot), (1, 0));
        step(&run.slot, 1);
        let released = run.worker.status().unwrap().unwrap();
        assert_eq!(counts(&run.slot), (1, 1));
        assert_eq!(released.state, TransportState::Stopped);
        assert_eq!(run.worker.status().unwrap(), Some(released));
    }

    #[test]
    fn stop_releases_a_running_output_and_returns_its_last_status() {
        let range = BarRange { first: 0, last: 0 };
        let run = played(TransportCommand::Play, range);
        step(&run.slot, 1_000);
        let last = run.worker.stop().unwrap();
        assert_eq!(last.state, TransportState::Playing);
        assert_eq!(last.frame, 1_000);
        assert_eq!(counts(&run.slot), (1, 1));
    }

    #[test]
    fn closing_the_playing_session_stops_it_and_closing_another_does_not() {
        let range = BarRange { first: 0, last: 0 };
        let mut player = Player::default();
        let run = played(TransportCommand::Play, range);
        let slot = Arc::clone(&run.slot);
        step(&slot, 1);
        player.active = Some(Active {
            worker: run.worker,
            session: "session-1".to_owned(),
        });
        player.close("session-2");
        assert_eq!(player.status().unwrap().state, "playing");
        assert_eq!(counts(&slot), (1, 0));
        player.close("session-1");
        assert_eq!(counts(&slot), (1, 1));
        assert_eq!(player.status().unwrap().state, "idle");
    }

    #[cfg(not(feature = "live"))]
    #[test]
    fn a_worker_without_playback_reports_nothing_and_stops_cleanly() {
        let worker = Worker::open().expect("an output opens");
        assert_eq!(worker.status().unwrap(), None);
        assert_eq!(worker.stop(), None);
    }
}
