use std::path::Path;

use opentab_model::blank_score;
use serde::Serialize;

use crate::error::CliError;
use crate::files::{self, Overwrite};
use crate::output::Rendered;
use crate::text::escape_controls;

#[derive(Serialize)]
pub struct Created {
    created: String,
}

pub fn run(out: &Path, title: Option<String>) -> Result<Rendered<Created>, CliError> {
    files::require_json(out)?;
    let mut score = blank_score().map_err(CliError::internal)?;
    score.meta.title = title.unwrap_or_default();
    files::write_score(out, &score, Overwrite::Refused)?;
    let created = out.display().to_string();
    let human = format!("created {}", escape_controls(&created));
    Ok(Rendered {
        wire: Created { created },
        human,
    })
}
