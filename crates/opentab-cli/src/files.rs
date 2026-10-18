use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use opentab_model::{Score, from_json, to_json};

use crate::error::CliError;

pub const SUFFIX: &str = ".opentab.json";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Overwrite {
    Allowed,
    Refused,
}

pub fn require_format(path: &Path) -> Result<(), CliError> {
    let supported = path
        .file_name()
        .map(|name| name.as_encoded_bytes())
        .is_some_and(|name| name.len() > SUFFIX.len() && name.ends_with(SUFFIX.as_bytes()));
    if supported {
        Ok(())
    } else {
        Err(CliError::UnsupportedFormat {
            path: path.to_path_buf(),
        })
    }
}

pub fn read_score(path: &Path) -> Result<Score, CliError> {
    require_format(path)?;
    let bytes = fs::read(path).map_err(|error| CliError::io(path, &error))?;
    let text = String::from_utf8(bytes).map_err(|_| CliError::InvalidScore {
        path: path.to_path_buf(),
        message: "file is not valid UTF-8".to_string(),
    })?;
    from_json(&text).map_err(|error| CliError::InvalidScore {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

pub fn read_text(path: &Path) -> Result<String, CliError> {
    let bytes = fs::read(path).map_err(|error| CliError::io(path, &error))?;
    String::from_utf8(bytes).map_err(|_| CliError::InvalidCommand {
        line: 1,
        message: format!("{} is not valid UTF-8", path.display()),
    })
}

pub fn write_score(path: &Path, score: &Score, overwrite: Overwrite) -> Result<(), CliError> {
    require_format(path)?;
    if overwrite == Overwrite::Refused && fs::symlink_metadata(path).is_ok() {
        return Err(CliError::OutputExists {
            path: path.to_path_buf(),
        });
    }
    let temp = temp_path(path);
    let written =
        write_synced(&temp, to_json(score).as_bytes()).and_then(|()| fs::rename(&temp, path));
    match written {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&temp);
            Err(CliError::io(path, &error))
        }
    }
}

fn write_synced(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = File::create_new(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn temp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp_name = format!(".{name}.{}.tmp", std::process::id());
    match path.parent() {
        Some(parent) => parent.join(temp_name),
        None => PathBuf::from(temp_name),
    }
}
