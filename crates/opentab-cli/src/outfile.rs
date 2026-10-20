use std::path::Path;

use crate::error::CliError;

pub fn check(out: &Path, suffix: &'static str) -> Result<(), CliError> {
    let name = out.file_name().and_then(|name| name.to_str());
    let fits = name.is_some_and(|name| {
        name.len() > suffix.len()
            && name.is_char_boundary(name.len() - suffix.len())
            && name[name.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
    });
    if !fits {
        return Err(CliError::UnsupportedFormat {
            path: out.to_path_buf(),
            supported: suffix,
        });
    }
    if out.is_dir() {
        return Err(CliError::Io {
            path: out.to_path_buf(),
            message: "is a directory; give a file name".to_string(),
        });
    }
    match out.parent() {
        Some(parent) if !parent.as_os_str().is_empty() && !parent.is_dir() => Err(CliError::Io {
            path: parent.to_path_buf(),
            message: "no such directory; create it first".to_string(),
        }),
        _ => Ok(()),
    }
}
