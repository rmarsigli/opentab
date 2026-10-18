use std::path::Path;

use opentab_model::{Diagnostic, InstrumentKind, Pitch, Score, diagnose};
use serde::Serialize;

use crate::error::CliError;
use crate::files;
use crate::ids::IdTree;
use crate::output::Rendered;
use crate::text::escape_controls;

#[derive(Serialize)]
pub struct TrackInfo {
    id: u64,
    name: String,
    kind: &'static str,
    strings: Vec<Pitch>,
    capo: u8,
}

#[derive(Serialize)]
pub struct TimeSignatureCount(String, usize);

#[derive(Serialize)]
pub struct Info {
    title: String,
    artist: String,
    tracks: Vec<TrackInfo>,
    master_bars: usize,
    time_signatures: Vec<TimeSignatureCount>,
    diagnostics: Vec<Diagnostic>,
    ids: IdTree,
}

pub fn run(path: &Path, show_ids: bool) -> Result<Rendered<Info>, CliError> {
    let score = files::read_score(path)?;
    let info = summarise(&score);
    let human = human(&info, show_ids);
    Ok(Rendered { wire: info, human })
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
        ids: IdTree::of(score),
    }
}

fn time_signatures(score: &Score) -> Vec<TimeSignatureCount> {
    let mut counts: Vec<TimeSignatureCount> = Vec::new();
    for master_bar in &score.master_bars {
        let label = format!(
            "{}/{}",
            master_bar.time_signature.numerator(),
            master_bar.time_signature.denominator()
        );
        match counts.iter_mut().find(|seen| seen.0 == label) {
            Some(seen) => seen.1 += 1,
            None => counts.push(TimeSignatureCount(label, 1)),
        }
    }
    counts
}

fn human(info: &Info, show_ids: bool) -> String {
    let mut lines = vec![
        format!("title: {}", escape_controls(&info.title)),
        format!("artist: {}", escape_controls(&info.artist)),
        format!("tracks: {}", info.tracks.len()),
    ];
    for (index, track) in info.tracks.iter().enumerate() {
        let strings: Vec<String> = track
            .strings
            .iter()
            .map(|pitch| pitch.0.to_string())
            .collect();
        lines.push(format!(
            "  {}: {}, {}, strings {}, capo {}",
            index + 1,
            escape_controls(&track.name),
            track.kind,
            strings.join(" "),
            track.capo
        ));
    }
    lines.push(format!("master bars: {}", info.master_bars));
    let signatures: Vec<String> = info
        .time_signatures
        .iter()
        .map(|seen| format!("{} x{}", seen.0, seen.1))
        .collect();
    lines.push(format!("time signatures: {}", signatures.join(", ")));
    if info.diagnostics.is_empty() {
        lines.push("diagnostics: none".to_string());
    } else {
        lines.push(format!("diagnostics: {}", info.diagnostics.len()));
        for diagnostic in &info.diagnostics {
            lines.push(format!(
                "  {}",
                serde_json::to_string(diagnostic).unwrap_or_default()
            ));
        }
    }
    if show_ids {
        lines.extend(info.ids.human());
    }
    lines.join("\n")
}
