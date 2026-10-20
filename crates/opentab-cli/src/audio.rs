use std::path::Path;

use opentab_audio::{SampleRate, encode_wav};
use opentab_files as files;
use serde::Serialize;

use crate::capture::capture;
use crate::cli::PlaybackArgs;
use crate::error::CliError;
use crate::outfile;
use crate::output::Rendered;
use crate::sound::{Details, prepare};
use crate::text::escape_controls;

#[derive(Serialize)]
pub struct Audio {
    files: Vec<String>,
    seconds: f64,
    frames: u64,
    sample_rate: u32,
    #[serde(flatten)]
    details: Details,
    played_bars: Vec<usize>,
}

pub fn seconds(frames: u64, rate: u32) -> f64 {
    frames as f64 / f64::from(rate)
}

pub fn run(args: &PlaybackArgs, out: &Path) -> Result<Rendered<Audio>, CliError> {
    outfile::check(out, ".wav")?;
    let mut prepared = prepare(args, SampleRate::default())?;
    let audio = capture(&mut prepared)?;
    let bytes = encode_wav(&audio);
    files::write_bytes(out, &bytes)?;
    let backends = prepared.backends.clone();
    let report = prepared.session.report(backends);
    let rate = audio.rate.hz();
    let played_bars = prepared.source_bars(&report.played_bars);
    let details = prepared.details(&report.backends);
    let length = seconds(audio.frames, rate);
    let file = out.display().to_string();
    let mut lines = vec![format!(
        "wrote {}: {length:.2} seconds, {} played bars, profile {}, speed {}%",
        escape_controls(&file),
        played_bars.len(),
        details.profile,
        details.speed
    )];
    lines.extend(prepared.lines(&report.backends));
    Ok(Rendered {
        wire: Audio {
            files: vec![file],
            seconds: length,
            frames: audio.frames,
            sample_rate: rate,
            details,
            played_bars,
        },
        human: lines.join("\n"),
    })
}
