use std::time::Duration;

use opentab_audio::{
    AudioOutput, CpalOutput, QueueFull, RENDER_TAIL_MS, TransportCommand, TransportHandle,
    TransportState,
};
use serde::Serialize;

use crate::audio::seconds;
use crate::cli::PlaybackArgs;
use crate::error::CliError;
use crate::output::{self, Rendered};
use crate::sound::{Details, prepare};

const POLL_MILLIS: u64 = 50;

#[derive(Serialize)]
pub struct Played {
    seconds: f64,
    #[serde(flatten)]
    details: Details,
}

fn send(handle: &mut TransportHandle, command: TransportCommand) -> Result<(), CliError> {
    handle
        .send(command)
        .map_err(|error: QueueFull| CliError::Audio {
            message: error.to_string(),
        })
}

pub fn run(args: &PlaybackArgs) -> Result<(), CliError> {
    let mut output = CpalOutput::open()?;
    let rate = output.rate();
    let prepared = prepare(args, rate)?;
    let range = prepared.range;
    let backends = prepared.backends.clone();
    let details = prepared.details(&backends);
    let lines = prepared.lines(&backends);
    let solo = prepared.tracks.len() < prepared.names.len();
    let tracks = prepared.tracks.clone();
    let (transport, mut handle) = prepared.session.split();
    output.start(transport)?;
    if let Some(range) = range {
        send(&mut handle, TransportCommand::Seek(range.first))?;
    }
    if solo {
        for track in &tracks {
            send(&mut handle, TransportCommand::Solo(*track, true))?;
        }
    }
    send(&mut handle, TransportCommand::Play)?;
    let mut started = false;
    loop {
        std::thread::sleep(Duration::from_millis(POLL_MILLIS));
        if output.has_failed() {
            return Err(CliError::Audio {
                message: "the audio stream stopped unexpectedly".to_string(),
            });
        }
        let status = handle.status();
        started |= status.state != TransportState::Stopped;
        if started && status.state == TransportState::Stopped {
            break;
        }
        if status.state == TransportState::Playing
            && range.is_some_and(|range| status.played_bar > range.last)
        {
            send(&mut handle, TransportCommand::Pause)?;
            break;
        }
    }
    std::thread::sleep(Duration::from_millis(u64::from(RENDER_TAIL_MS)));
    let frames = handle.status().frame;
    output.stop();
    let length = seconds(frames, rate.hz());
    let mut text = vec![format!("played {length:.2} seconds")];
    text.extend(lines);
    output::emit(
        &Rendered {
            wire: Played {
                seconds: length,
                details,
            },
            human: text.join("\n"),
        },
        args.json,
    )
}
