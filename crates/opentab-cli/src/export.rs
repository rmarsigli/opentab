use std::path::Path;

use opentab_audio::{SampleRate, frame_at};
use opentab_files as files;
use opentab_playback::{PlaybackOptions, Speed, perform, write_smf};
use serde::Serialize;

use crate::audio::seconds;
use crate::error::CliError;
use crate::outfile;
use crate::output::Rendered;
use crate::sound::{
    ProfileChoice, UnplayedWire, WarningWire, unplayed_lines, unplayed_wire, warnings_wire,
};
use crate::text::escape_controls;

#[derive(Serialize)]
pub struct Exported {
    files: Vec<String>,
    bytes: usize,
    profile: &'static str,
    ppq: u16,
    seconds: f64,
    played_bars: Vec<usize>,
    unplayed: Vec<UnplayedWire>,
    warnings: Vec<WarningWire>,
}

pub fn run(path: &Path, choice: ProfileChoice, out: &Path) -> Result<Rendered<Exported>, CliError> {
    outfile::check(out, ".mid")?;
    let score = files::read_score(path)?;
    let profile = choice.profile();
    let performance = perform(&score, &PlaybackOptions { profile })?;
    let ppq = profile.ppq();
    let bytes = write_smf(&performance, ppq);
    files::write_bytes(out, &bytes)?;
    let rate = SampleRate::default();
    let frames = frame_at(
        &performance.tempo,
        performance.order.end(),
        Speed::NORMAL,
        rate,
    )?;
    let length = seconds(frames, rate.hz());
    let played_bars: Vec<usize> = performance
        .order
        .bars
        .iter()
        .map(|bar| bar.source + 1)
        .collect();
    let file = out.display().to_string();
    let mut lines = vec![format!(
        "wrote {}: {} bytes, {length:.2} seconds, {} played bars, profile {}",
        escape_controls(&file),
        bytes.len(),
        played_bars.len(),
        choice.name()
    )];
    lines.extend(unplayed_lines(&performance.unplayed));
    Ok(Rendered {
        wire: Exported {
            files: vec![file],
            bytes: bytes.len(),
            profile: choice.name(),
            ppq: ppq.get(),
            seconds: length,
            played_bars,
            unplayed: unplayed_wire(&performance.unplayed),
            warnings: warnings_wire(&performance.warnings),
        },
        human: lines.join("\n"),
    })
}
