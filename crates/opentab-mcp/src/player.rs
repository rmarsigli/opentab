use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread::JoinHandle;
use std::time::Duration;

use opentab_audio::{
    AudioOutput, OutputError, SampleRate, Transport, TransportCommand, TransportHandle,
    TransportState, TransportStatus,
};
use serde::Serialize;

use crate::error::ToolError;

pub const POLL_MILLIS: u64 = 20;

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

pub fn range_finished(status: &TransportStatus, last: Option<usize>) -> bool {
    status.state == TransportState::Playing && last.is_some_and(|last| status.played_bar > last)
}

pub struct Launch {
    pub transport: Transport,
    pub handle: TransportHandle,
    pub last_bar: Option<usize>,
}

enum Message {
    Launch(Box<Launch>, Sender<Result<(), OutputError>>),
    Status(Sender<Option<TransportStatus>>),
    Stop(Sender<Option<TransportStatus>>),
}

struct Running {
    handle: TransportHandle,
    last_bar: Option<usize>,
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

fn guard(running: &mut Running) {
    let status = running.handle.status();
    if range_finished(&status, running.last_bar) {
        let _ = running.handle.send(TransportCommand::Pause);
    }
}

fn serve(ready: Sender<Result<SampleRate, OutputError>>, messages: Receiver<Message>) {
    let mut output = match open_output() {
        Ok(output) => output,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    if ready.send(Ok(output.rate())).is_err() {
        return;
    }
    let mut running: Option<Running> = None;
    loop {
        match messages.recv_timeout(Duration::from_millis(POLL_MILLIS)) {
            Ok(Message::Launch(launch, done)) => {
                let Launch {
                    transport,
                    handle,
                    last_bar,
                } = *launch;
                let started = output.start(transport);
                if started.is_ok() {
                    running = Some(Running { handle, last_bar });
                }
                let _ = done.send(started);
            }
            Ok(Message::Status(reply)) => {
                let _ = reply.send(running.as_ref().map(|run| run.handle.status()));
            }
            Ok(Message::Stop(reply)) => {
                let last = running.as_ref().map(|run| run.handle.status());
                output.stop();
                let _ = reply.send(last);
                return;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                output.stop();
                return;
            }
        }
        if let Some(run) = running.as_mut() {
            guard(run);
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
        let (messages, receiver) = channel();
        let (ready, rate) = channel();
        let thread = std::thread::Builder::new()
            .name("opentab-mcp-output".to_owned())
            .spawn(move || serve(ready, receiver))
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
}

#[cfg(test)]
mod tests {
    use opentab_model::Fraction;

    use super::*;

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
    fn a_range_is_finished_only_while_playing_past_its_last_played_bar() {
        assert!(range_finished(&status(TransportState::Playing, 5), Some(4)));
        assert!(!range_finished(
            &status(TransportState::Playing, 4),
            Some(4)
        ));
        assert!(!range_finished(&status(TransportState::Paused, 5), Some(4)));
        assert!(!range_finished(
            &status(TransportState::CountIn, 5),
            Some(4)
        ));
        assert!(!range_finished(&status(TransportState::Playing, 99), None));
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

    #[cfg(not(feature = "live"))]
    #[test]
    fn a_worker_without_playback_reports_nothing_and_stops_cleanly() {
        let worker = Worker::open().expect("an output opens");
        assert_eq!(worker.status().unwrap(), None);
        assert_eq!(worker.stop(), None);
    }
}
