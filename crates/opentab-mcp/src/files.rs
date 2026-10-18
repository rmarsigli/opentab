use std::fs;
use std::path::Path;

use opentab_gp::{GpError, read_gp, write_gp};
use opentab_model::{Score, from_json, to_json};

use crate::error::ToolError;

pub const JSON_SUFFIX: &str = ".opentab.json";
pub const GP_SUFFIX: &str = ".gp";
pub const SUPPORTED: &str = ".gp, .opentab.json";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    Json,
    Gp,
}

impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Format::Json => "opentab.json",
            Format::Gp => "gp",
        }
    }
}

pub fn format_of(path: &Path) -> Result<Format, ToolError> {
    let name = path.file_name().map(|name| name.as_encoded_bytes());
    let has = |suffix: &str| {
        name.is_some_and(|name| {
            name.len() > suffix.len()
                && name
                    .get(name.len() - suffix.len()..)
                    .is_some_and(|tail| tail.eq_ignore_ascii_case(suffix.as_bytes()))
        })
    };
    if has(JSON_SUFFIX) {
        Ok(Format::Json)
    } else if has(GP_SUFFIX) {
        Ok(Format::Gp)
    } else {
        Err(ToolError::new(
            "unsupported_format",
            format!(
                "{}: unsupported file type, supported formats: {SUPPORTED}",
                path.display()
            ),
        ))
    }
}

pub fn read_score(path: &Path) -> Result<Score, ToolError> {
    let format = format_of(path)?;
    let bytes = fs::read(path).map_err(|error| io(path, &error))?;
    match format {
        Format::Gp => read_gp(&bytes)
            .map_err(|error| ToolError::invalid_input(format!("{}: {error}", path.display()))),
        Format::Json => {
            let text = String::from_utf8(bytes).map_err(|_| {
                ToolError::new(
                    "invalid_score",
                    format!("{}: file is not valid UTF-8", path.display()),
                )
            })?;
            from_json(&text).map_err(|error| {
                ToolError::new("invalid_score", format!("{}: {error}", path.display()))
            })
        }
    }
}

pub fn write_score(path: &Path, score: &Score) -> Result<Format, ToolError> {
    let format = format_of(path)?;
    let bytes = match format {
        Format::Json => to_json(score)
            .map(String::into_bytes)
            .map_err(|error| ToolError::internal(error.to_string()))?,
        Format::Gp => write_gp(score).map_err(|error| refusal(path, error))?,
    };
    fs::write(path, bytes).map_err(|error| io(path, &error))?;
    Ok(format)
}

fn refusal(path: &Path, error: GpError) -> ToolError {
    match error {
        GpError::Unsupported {
            path: model,
            reason,
        } => ToolError::new(
            "unsupported",
            format!("{}: {model}: {reason}", path.display()),
        ),
        other => ToolError::internal(other.to_string()),
    }
}

fn io(path: &Path, error: &std::io::Error) -> ToolError {
    ToolError::new("io", format!("{}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_format_follows_the_suffix_in_any_case() {
        assert_eq!(format_of(Path::new("a.gp")), Ok(Format::Gp));
        assert_eq!(format_of(Path::new("dir/A.GP")), Ok(Format::Gp));
        assert_eq!(format_of(Path::new("a.opentab.json")), Ok(Format::Json));
        assert_eq!(format_of(Path::new("a.OpenTab.JSON")), Ok(Format::Json));
        for refused in ["a.json", "a.gpx", "a.gp5", ".gp", ".opentab.json", "a"] {
            let error = format_of(Path::new(refused)).unwrap_err();
            assert_eq!(error.kind, "unsupported_format", "{refused}");
        }
    }
}
