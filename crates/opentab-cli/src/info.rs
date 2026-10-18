use std::path::Path;

use opentab_model::{Diagnostic, InstrumentKind, Pitch, Score, diagnose};
use serde::Serialize;

use crate::error::CliError;
use crate::files;
use crate::output::Report;

#[derive(Serialize)]
pub struct TrackInfo {
    id: u64,
    name: String,
    kind: &'static str,
    strings: Vec<Pitch>,
    capo: u8,
}

#[derive(Serialize)]
pub struct Info {
    title: String,
    artist: String,
    tracks: Vec<TrackInfo>,
    master_bars: usize,
    time_signatures: Vec<(String, usize)>,
    diagnostics: Vec<Diagnostic>,
}

pub fn run(path: &Path) -> Result<Info, CliError> {
    let score = files::read_score(path)?;
    Ok(summarise(&score))
}

fn summarise(score: &Score) -> Info {
    Info {
        title: score.meta.title.clone(),
        artist: score.meta.artist.clone(),
        tracks: score
            .tracks
            .iter()
            .map(|track| TrackInfo {
                id: track.id.0.0,
                name: track.name.clone(),
                kind: match track.instrument.kind {
                    InstrumentKind::Fretted => "fretted",
                    InstrumentKind::Percussion => "percussion",
                },
                strings: track.tuning.strings.clone(),
                capo: track.capo,
            })
            .collect(),
        master_bars: score.master_bars.len(),
        time_signatures: time_signatures(score),
        diagnostics: diagnose(score),
    }
}

fn time_signatures(score: &Score) -> Vec<(String, usize)> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for master_bar in &score.master_bars {
        let label = format!(
            "{}/{}",
            master_bar.time_signature.numerator(),
            master_bar.time_signature.denominator()
        );
        match counts.iter_mut().find(|(seen, _)| *seen == label) {
            Some((_, count)) => *count += 1,
            None => counts.push((label, 1)),
        }
    }
    counts
}

impl Report for Info {
    fn human(&self) -> String {
        let mut lines = vec![
            format!("title: {}", self.title),
            format!("artist: {}", self.artist),
            format!("tracks: {}", self.tracks.len()),
        ];
        for (index, track) in self.tracks.iter().enumerate() {
            let strings: Vec<String> = track
                .strings
                .iter()
                .map(|pitch| pitch.0.to_string())
                .collect();
            lines.push(format!(
                "  {}: {}, {}, strings {}, capo {}",
                index + 1,
                track.name,
                track.kind,
                strings.join(" "),
                track.capo
            ));
        }
        lines.push(format!("master bars: {}", self.master_bars));
        let signatures: Vec<String> = self
            .time_signatures
            .iter()
            .map(|(label, count)| format!("{label} x{count}"))
            .collect();
        lines.push(format!("time signatures: {}", signatures.join(", ")));
        if self.diagnostics.is_empty() {
            lines.push("diagnostics: none".to_string());
        } else {
            lines.push(format!("diagnostics: {}", self.diagnostics.len()));
            for diagnostic in &self.diagnostics {
                lines.push(format!(
                    "  {}",
                    serde_json::to_string(diagnostic).unwrap_or_default()
                ));
            }
        }
        lines.join("\n")
    }
}
