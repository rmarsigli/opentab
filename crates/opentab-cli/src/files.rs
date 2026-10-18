use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use opentab_model::{Score, from_json, to_json};

use crate::error::CliError;

pub const SUFFIX: &str = ".opentab.json";

const TEMP_ATTEMPTS: usize = 16;
const MAX_LINK_HOPS: usize = 40;
const MAX_TEMP_NAME_BYTES: usize = 100;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

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
    String::from_utf8(bytes).map_err(|_| CliError::InvalidInput {
        message: format!("{} is not valid UTF-8", path.display()),
    })
}

pub fn write_score(path: &Path, score: &Score, overwrite: Overwrite) -> Result<(), CliError> {
    require_format(path)?;
    if overwrite == Overwrite::Refused && fs::symlink_metadata(path).is_ok() {
        return Err(output_exists(path));
    }
    let text = to_json(score).map_err(CliError::internal)?;
    match overwrite {
        Overwrite::Allowed => replace(path, text.as_bytes()),
        Overwrite::Refused => create(path, text.as_bytes()),
    }
}

fn output_exists(path: &Path) -> CliError {
    CliError::OutputExists {
        path: path.to_path_buf(),
    }
}

fn replace(path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    let target = resolve_target(path).map_err(|error| CliError::io(path, &error))?;
    let temp = stage(&target, bytes, next_counter).map_err(|error| CliError::io(path, &error))?;
    fs::rename(&temp, &target).map_err(|error| {
        let _ = fs::remove_file(&temp);
        CliError::io(path, &error)
    })
}

fn create(path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    let temp = stage(path, bytes, next_counter).map_err(|error| CliError::io(path, &error))?;
    let outcome = match fs::hard_link(&temp, path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => Err(output_exists(path)),
        Err(_) => create_new_file(path, bytes),
    };
    let _ = fs::remove_file(&temp);
    outcome
}

fn create_new_file(path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => return Err(output_exists(path)),
        Err(error) => return Err(CliError::io(path, &error)),
    };
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    written.map_err(|error| {
        drop(file);
        let _ = fs::remove_file(path);
        CliError::io(path, &error)
    })
}

fn resolve_target(path: &Path) -> io::Result<PathBuf> {
    let mut current = path.to_path_buf();
    for _ in 0..MAX_LINK_HOPS {
        let is_link = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata.file_type().is_symlink(),
            Err(error) if error.kind() == ErrorKind::NotFound => false,
            Err(error) => return Err(error),
        };
        if !is_link {
            return anchored(&current);
        }
        let link = fs::read_link(&current)?;
        current = match current.parent() {
            Some(parent) => parent.join(link),
            None => link,
        };
    }
    Err(io::Error::other("too many levels of symbolic links"))
}

fn anchored(path: &Path) -> io::Result<PathBuf> {
    let Some(name) = path.file_name() else {
        return Ok(path.to_path_buf());
    };
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    Ok(fs::canonicalize(parent)?.join(name))
}

fn next_counter() -> u64 {
    TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
}

fn stage(target: &Path, bytes: &[u8], next: impl FnMut() -> u64) -> io::Result<PathBuf> {
    let (temp, mut file) = create_temp(target, next)?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    match written {
        Ok(()) => Ok(temp),
        Err(error) => {
            let _ = fs::remove_file(&temp);
            Err(error)
        }
    }
}

fn create_temp(target: &Path, mut next: impl FnMut() -> u64) -> io::Result<(PathBuf, File)> {
    for _ in 0..TEMP_ATTEMPTS {
        let temp = temp_path(target, next());
        match File::create_new(&temp) {
            Ok(file) => return Ok((temp, file)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        ErrorKind::AlreadyExists,
        "no free temporary file name",
    ))
}

fn temp_path(path: &Path, counter: u64) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp_name = format!(".{}.{}.{counter}.tmp", truncated(&name), std::process::id());
    match path.parent() {
        Some(parent) => parent.join(temp_name),
        None => PathBuf::from(temp_name),
    }
}

fn truncated(name: &str) -> &str {
    let mut end = name.len().min(MAX_TEMP_NAME_BYTES);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    &name[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn a_taken_temp_name_is_skipped_for_the_next_counter() {
        let dir = dir();
        let target = dir.path().join("a.opentab.json");
        let taken = temp_path(&target, 0);
        fs::write(&taken, "other writer").unwrap();
        let mut counters = [0, 0, 1].into_iter();
        let (temp, _file) = create_temp(&target, || counters.next().unwrap()).unwrap();
        assert_eq!(temp, temp_path(&target, 1));
        assert_eq!(fs::read_to_string(&taken).unwrap(), "other writer");
    }

    #[test]
    fn temp_creation_gives_up_after_sixteen_taken_names() {
        let dir = dir();
        let target = dir.path().join("a.opentab.json");
        fs::write(temp_path(&target, 7), "x").unwrap();
        let mut attempts = 0;
        let error = create_temp(&target, || {
            attempts += 1;
            7
        })
        .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::AlreadyExists);
        assert_eq!(attempts, TEMP_ATTEMPTS);
    }

    #[test]
    fn temp_names_carry_pid_and_counter_and_stay_short_for_long_names() {
        let long = "é".repeat(120);
        let temp = temp_path(Path::new(&format!("{long}.opentab.json")), 5);
        let name = temp.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.ends_with(&format!(".{}.5.tmp", std::process::id())));
        assert!(name.len() < 130, "{}", name.len());
    }

    #[test]
    fn where_hard_links_are_unsupported_creation_falls_back_to_create_new() {
        let dir = dir();
        let path = dir.path().join("a.opentab.json");
        create_new_file(&path, b"fresh").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "fresh");
        let error = create_new_file(&path, b"second").unwrap_err();
        assert!(matches!(error, CliError::OutputExists { .. }));
        assert_eq!(fs::read_to_string(&path).unwrap(), "fresh");
    }
}
