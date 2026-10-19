use std::path::Path;

use opentab_command::Info;

use crate::error::CliError;
use crate::ids;
use crate::output::Rendered;
use crate::text::escape_controls;
use opentab_files as files;

pub fn run(path: &Path, show_ids: bool) -> Result<Rendered<Info>, CliError> {
    let score = files::read_score(path)?;
    let info = Info::of(&score);
    let human = human(&info, show_ids);
    Ok(Rendered { wire: info, human })
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
        lines.extend(ids::human(&info.ids));
    }
    lines.join("\n")
}
