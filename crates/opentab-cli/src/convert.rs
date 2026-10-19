use std::path::Path;

use serde::Serialize;

use crate::error::CliError;
use crate::output::Rendered;
use crate::text::escape_controls;
use opentab_files::{self as files, Overwrite};

#[derive(Serialize)]
pub struct Conversion {
    from: String,
    to: String,
}

#[derive(Serialize)]
pub struct Converted {
    converted: Conversion,
}

pub fn run(path: &Path, out: &Path) -> Result<Rendered<Converted>, CliError> {
    files::format_of(path)?;
    files::format_of(out)?;
    let score = files::read_score(path)?;
    files::write_score(out, &score, Overwrite::Allowed)?;
    let from = path.display().to_string();
    let to = out.display().to_string();
    let human = format!(
        "converted {} to {}",
        escape_controls(&from),
        escape_controls(&to)
    );
    Ok(Rendered {
        wire: Converted {
            converted: Conversion { from, to },
        },
        human,
    })
}
