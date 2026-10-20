use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use opentab_audio::{AudioOutput, CpalOutput, RENDER_TAIL_MS};
use serde::Serialize;

use crate::audio::seconds;
use crate::cli::PlaybackArgs;
use crate::error::CliError;
use crate::output::{self, Rendered};
use crate::playing::{Device, Pacing, Start, perform};
use crate::sound::{Details, prepare};

const POLL_MILLIS: u64 = 50;

#[derive(Serialize)]
pub struct Played {
    seconds: f64,
    #[serde(flatten)]
    details: Details,
}

impl Device for CpalOutput {
    fn has_failed(&self) -> bool {
        CpalOutput::has_failed(self)
    }

    fn pause(&mut self, span: Duration) {
        std::thread::sleep(span);
    }
}

fn watch_interrupt() -> Result<Arc<AtomicBool>, CliError> {
    let flag = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&flag);
    ctrlc::set_handler(move || seen.store(true, Ordering::SeqCst)).map_err(|error| {
        CliError::Audio {
            message: format!("cannot watch for Ctrl-C: {error}"),
        }
    })?;
    Ok(flag)
}

pub fn run(args: &PlaybackArgs) -> Result<(), CliError> {
    let mut output = CpalOutput::open()?;
    let rate = output.rate();
    let prepared = prepare(args, rate)?;
    let interrupt = watch_interrupt()?;
    let backends = prepared.backends.clone();
    let details = prepared.details(&backends);
    let lines = prepared.lines(&backends);
    let start = Start {
        range: prepared.range,
        solo: if prepared.tracks.len() < prepared.names.len() {
            prepared.tracks.clone()
        } else {
            Vec::new()
        },
    };
    let pacing = Pacing {
        poll: Duration::from_millis(POLL_MILLIS),
        tail: Duration::from_millis(u64::from(RENDER_TAIL_MS)),
    };
    let frames = perform(&mut output, prepared.session, &start, &interrupt, pacing)?;
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
