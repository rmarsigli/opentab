use std::path::Path;

use opentab_command::{apply, element_ids, parse_jsonl_lines};
use serde::Serialize;

use crate::error::CliError;
use crate::files::{self, Overwrite};
use crate::output::Rendered;
use crate::text::escape_controls;

#[derive(Serialize)]
pub struct Applied {
    applied: usize,
    allocated: Vec<u64>,
}

pub fn run(path: &Path, commands: &Path, out: &Path) -> Result<Rendered<Applied>, CliError> {
    files::format_of(path)?;
    files::format_of(out)?;
    let mut score = files::read_score(path)?;
    let text = files::read_text(commands)?;
    let parsed = parse_jsonl_lines(&text).map_err(|error| CliError::InvalidCommand {
        line: error.line,
        message: error.message,
    })?;
    let before = element_ids(&score);
    for numbered in &parsed {
        apply(&mut score, &numbered.command).map_err(|source| CliError::CommandFailed {
            line: numbered.line,
            source,
        })?;
    }
    let allocated: Vec<u64> = element_ids(&score).difference(&before).copied().collect();
    files::write_score(out, &score, Overwrite::Allowed)?;
    let noun = if parsed.len() == 1 {
        "command"
    } else {
        "commands"
    };
    let human = format!(
        "applied {} {noun}, wrote {}",
        parsed.len(),
        escape_controls(&out.display().to_string())
    );
    Ok(Rendered {
        wire: Applied {
            applied: parsed.len(),
            allocated,
        },
        human,
    })
}
