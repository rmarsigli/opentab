use std::path::Path;

use opentab_command::{apply, parse_jsonl};
use serde::Serialize;

use crate::error::CliError;
use crate::files::{self, Overwrite};
use crate::output::Report;

const BOM: char = '\u{feff}';

#[derive(Serialize)]
pub struct Applied {
    applied: usize,
    #[serde(skip)]
    out: String,
}

impl Report for Applied {
    fn human(&self) -> String {
        let noun = if self.applied == 1 {
            "command"
        } else {
            "commands"
        };
        format!("applied {} {noun}, wrote {}", self.applied, self.out)
    }
}

pub fn run(path: &Path, commands: &Path, out: &Path) -> Result<Applied, CliError> {
    files::require_format(path)?;
    files::require_format(out)?;
    let mut score = files::read_score(path)?;
    let text = files::read_text(commands)?;
    let text = text.strip_prefix(BOM).unwrap_or(&text);
    let mut applied = 0;
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let parsed = parse_jsonl(raw).map_err(|error| CliError::InvalidCommand {
            line,
            message: error.message,
        })?;
        for command in parsed {
            apply(&mut score, &command)
                .map_err(|source| CliError::CommandFailed { line, source })?;
            applied += 1;
        }
    }
    files::write_score(out, &score, Overwrite::Allowed)?;
    Ok(Applied {
        applied,
        out: out.display().to_string(),
    })
}
