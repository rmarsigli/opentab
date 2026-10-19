use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use opentab_gp::{GpError, read_gp, write_gp};
use opentab_model::{Score, from_json, to_json};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FileError {
    #[error("{}: {message}", path.display())]
    Io { path: PathBuf, message: String },
    #[error("{}: unsupported file type, supported formats: {supported}", path.display())]
    UnsupportedFormat {
        path: PathBuf,
        supported: &'static str,
    },
    #[error("{}: {message}", path.display())]
    Unsupported { path: PathBuf, message: String },
    #[error("{}: {message}", path.display())]
    InvalidScore { path: PathBuf, message: String },
    #[error("{} already exists, refusing to overwrite", path.display())]
    OutputExists { path: PathBuf },
    #[error("{message}")]
    InvalidInput { message: String },
    #[error("{message}")]
    Internal { message: String },
}

impl FileError {
    pub fn kind(&self) -> &'static str {
        match self {
            FileError::Io { .. } => "io",
            FileError::UnsupportedFormat { .. } => "unsupported_format",
            FileError::Unsupported { .. } => "unsupported",
            FileError::InvalidScore { .. } => "invalid_score",
            FileError::OutputExists { .. } => "output_exists",
            FileError::InvalidInput { .. } => "invalid_input",
            FileError::Internal { .. } => "internal",
        }
    }

    pub fn io(path: &Path, source: &io::Error) -> FileError {
        FileError::Io {
            path: path.to_path_buf(),
            message: source.to_string(),
        }
    }
}

pub const SUFFIX: &str = ".opentab.json";
pub const GP_SUFFIX: &str = ".gp";
pub const SUPPORTED: &str = ".gp, .opentab.json";

const TEMP_ATTEMPTS: usize = 16;
const MAX_LINK_HOPS: usize = 40;
const MAX_TEMP_NAME_BYTES: usize = 100;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Overwrite {
    Allowed,
    Refused,
}

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

pub fn format_of(path: &Path) -> Result<Format, FileError> {
    let name = path.file_name().map(|name| name.as_encoded_bytes());
    let has = |suffix: &str| {
        name.is_some_and(|name| {
            name.len() > suffix.len()
                && name
                    .get(name.len() - suffix.len()..)
                    .is_some_and(|tail| tail.eq_ignore_ascii_case(suffix.as_bytes()))
        })
    };
    if has(SUFFIX) {
        Ok(Format::Json)
    } else if has(GP_SUFFIX) {
        Ok(Format::Gp)
    } else {
        Err(unsupported_format(path, SUPPORTED))
    }
}

pub fn require_json(path: &Path) -> Result<(), FileError> {
    match format_of(path) {
        Ok(Format::Json) => Ok(()),
        _ => Err(unsupported_format(path, SUFFIX)),
    }
}

fn unsupported_format(path: &Path, supported: &'static str) -> FileError {
    FileError::UnsupportedFormat {
        path: path.to_path_buf(),
        supported,
    }
}

pub fn read_score(path: &Path) -> Result<Score, FileError> {
    let format = format_of(path)?;
    let bytes = fs::read(path).map_err(|error| FileError::io(path, &error))?;
    match format {
        Format::Json => parse_json(path, bytes),
        Format::Gp => read_gp(&bytes).map_err(|error| FileError::InvalidInput {
            message: format!("{}: {error}", path.display()),
        }),
    }
}

fn parse_json(path: &Path, bytes: Vec<u8>) -> Result<Score, FileError> {
    let text = String::from_utf8(bytes).map_err(|_| FileError::InvalidScore {
        path: path.to_path_buf(),
        message: "file is not valid UTF-8".to_string(),
    })?;
    from_json(&text).map_err(|error| FileError::InvalidScore {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

pub fn require_regular_file(path: &Path) -> Result<(), FileError> {
    let metadata = fs::metadata(path).map_err(|error| FileError::io(path, &error))?;
    if metadata.is_file() {
        Ok(())
    } else {
        Err(FileError::Io {
            path: path.to_path_buf(),
            message: "not a regular file".to_string(),
        })
    }
}

pub fn read_text(path: &Path) -> Result<String, FileError> {
    let bytes = fs::read(path).map_err(|error| FileError::io(path, &error))?;
    String::from_utf8(bytes).map_err(|_| FileError::InvalidInput {
        message: format!("{} is not valid UTF-8", path.display()),
    })
}

pub fn write_score(path: &Path, score: &Score, overwrite: Overwrite) -> Result<(), FileError> {
    let format = format_of(path)?;
    if overwrite == Overwrite::Refused && fs::symlink_metadata(path).is_ok() {
        return Err(output_exists(path));
    }
    let bytes = encode(path, score, format)?;
    match overwrite {
        Overwrite::Allowed => replace(path, &bytes),
        Overwrite::Refused => create(path, &bytes),
    }
}

pub fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), FileError> {
    replace(path, bytes)
}

fn encode(path: &Path, score: &Score, format: Format) -> Result<Vec<u8>, FileError> {
    match format {
        Format::Json => {
            to_json(score)
                .map(String::into_bytes)
                .map_err(|error| FileError::Internal {
                    message: error.to_string(),
                })
        }
        Format::Gp => write_gp(score).map_err(|error| refusal(path, error)),
    }
}

fn refusal(path: &Path, error: GpError) -> FileError {
    match error {
        GpError::Unsupported {
            path: model,
            reason,
        } => FileError::Unsupported {
            path: path.to_path_buf(),
            message: format!("{model}: {reason}"),
        },
        other => FileError::Internal {
            message: other.to_string(),
        },
    }
}

fn output_exists(path: &Path) -> FileError {
    FileError::OutputExists {
        path: path.to_path_buf(),
    }
}

fn replace(path: &Path, bytes: &[u8]) -> Result<(), FileError> {
    let target = resolve_target(path).map_err(|error| FileError::io(path, &error))?;
    let temp = stage(&target, bytes, next_counter).map_err(|error| FileError::io(path, &error))?;
    fs::rename(&temp, &target).map_err(|error| {
        let _ = fs::remove_file(&temp);
        FileError::io(path, &error)
    })
}

fn create(path: &Path, bytes: &[u8]) -> Result<(), FileError> {
    let temp = stage(path, bytes, next_counter).map_err(|error| FileError::io(path, &error))?;
    let outcome = match fs::hard_link(&temp, path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => Err(output_exists(path)),
        Err(_) => create_new_file(path, bytes),
    };
    let _ = fs::remove_file(&temp);
    outcome
}

fn create_new_file(path: &Path, bytes: &[u8]) -> Result<(), FileError> {
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => return Err(output_exists(path)),
        Err(error) => return Err(FileError::io(path, &error)),
    };
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    written.map_err(|error| {
        drop(file);
        let _ = fs::remove_file(path);
        FileError::io(path, &error)
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
        assert!(matches!(error, FileError::OutputExists { .. }));
        assert_eq!(fs::read_to_string(&path).unwrap(), "fresh");
    }

    #[test]
    fn only_regular_files_pass_the_regular_file_check() {
        let dir = dir();
        let file = dir.path().join("a.gp");
        fs::write(&file, b"x").unwrap();
        assert_eq!(require_regular_file(&file), Ok(()));
        let link = dir.path().join("link.gp");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        assert_eq!(require_regular_file(&link), Ok(()));
        for special in [dir.path().to_path_buf(), PathBuf::from("/dev/null")] {
            let error = require_regular_file(&special).unwrap_err();
            assert_eq!(error.kind(), "io");
            assert_eq!(
                error.to_string(),
                format!("{}: not a regular file", special.display())
            );
        }
        let device = dir.path().join("zero.gp");
        std::os::unix::fs::symlink("/dev/zero", &device).unwrap();
        assert!(require_regular_file(&device).is_err());
        let missing = require_regular_file(&dir.path().join("missing.gp")).unwrap_err();
        assert_eq!(missing.kind(), "io");
    }

    #[test]
    fn errors_name_their_kind_and_print_the_cli_messages() {
        let path = PathBuf::from("dir/a.gp");
        let cases = [
            (
                FileError::Io {
                    path: path.clone(),
                    message: "denied".to_string(),
                },
                "io",
                "dir/a.gp: denied",
            ),
            (
                FileError::UnsupportedFormat {
                    path: path.clone(),
                    supported: SUPPORTED,
                },
                "unsupported_format",
                "dir/a.gp: unsupported file type, supported formats: .gp, .opentab.json",
            ),
            (
                FileError::Unsupported {
                    path: path.clone(),
                    message: "no track".to_string(),
                },
                "unsupported",
                "dir/a.gp: no track",
            ),
            (
                FileError::InvalidScore {
                    path: path.clone(),
                    message: "bad".to_string(),
                },
                "invalid_score",
                "dir/a.gp: bad",
            ),
            (
                FileError::OutputExists { path: path.clone() },
                "output_exists",
                "dir/a.gp already exists, refusing to overwrite",
            ),
            (
                FileError::InvalidInput {
                    message: "m".to_string(),
                },
                "invalid_input",
                "m",
            ),
            (
                FileError::Internal {
                    message: "i".to_string(),
                },
                "internal",
                "i",
            ),
        ];
        for (error, kind, message) in cases {
            assert_eq!(error.kind(), kind);
            assert_eq!(error.to_string(), message);
        }
        assert_eq!(Format::Gp.name(), "gp");
        assert_eq!(Format::Json.name(), "opentab.json");
    }
}
