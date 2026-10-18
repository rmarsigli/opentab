use crate::oracle::error::OracleError;
use std::collections::BTreeMap;
use std::path::Path;

pub const MANIFEST_FILE: &str = "manifest.json";
pub const PNG_DIR: &str = "png";
pub const REQUIRED_OUTPUTS: [&str; 6] = [
    "resave.gp",
    "export.mid",
    "export.musicxml",
    "export.pdf",
    "export.gp5",
    "export.gpx",
];
pub const NON_BYTE_IDENTICAL_OUTPUTS: [&str; 1] = ["export.gp5"];

pub fn is_byte_identical(path: &str) -> bool {
    !NON_BYTE_IDENTICAL_OUTPUTS.contains(&path)
}

pub fn is_output_path(path: &str) -> bool {
    REQUIRED_OUTPUTS.contains(&path)
        || path
            .strip_prefix("png/")
            .is_some_and(|rest| !rest.contains('/') && rest.to_ascii_lowercase().ends_with(".png"))
}

pub fn list_files(root: &Path) -> Result<BTreeMap<String, std::path::PathBuf>, OracleError> {
    let mut files = BTreeMap::new();
    if root.is_dir() {
        collect(root, "", &mut files)?;
    }
    Ok(files)
}

fn collect(
    dir: &Path,
    prefix: &str,
    files: &mut BTreeMap<String, std::path::PathBuf>,
) -> Result<(), OracleError> {
    let reader = std::fs::read_dir(dir)
        .map_err(|error| OracleError::io(format!("cannot list {}", dir.display()), &error))?;
    for item in reader {
        let item = item
            .map_err(|error| OracleError::io(format!("cannot list {}", dir.display()), &error))?;
        let path = item.path();
        let name = item.file_name().to_string_lossy().into_owned();
        let kind = std::fs::symlink_metadata(&path)
            .map_err(|error| OracleError::io(format!("cannot stat {}", path.display()), &error))?
            .file_type();
        let key = format!("{prefix}{name}");
        if kind.is_dir() {
            collect(&path, &format!("{key}/"), files)?;
        } else {
            files.insert(key, path);
        }
    }
    Ok(())
}

pub fn select_outputs(job_out: &Path) -> Result<BTreeMap<String, std::path::PathBuf>, OracleError> {
    let all = list_files(job_out)?;
    let selected: BTreeMap<String, std::path::PathBuf> = all
        .into_iter()
        .filter(|(path, _)| is_output_path(path))
        .collect();
    let mut missing: Vec<String> = REQUIRED_OUTPUTS
        .iter()
        .filter(|name| !selected.contains_key(**name))
        .map(|name| (*name).to_owned())
        .collect();
    if !selected.keys().any(|path| path.starts_with("png/")) {
        missing.push(String::from("png/*.png"));
    }
    if missing.is_empty() {
        Ok(selected)
    } else {
        Err(OracleError::new(format!(
            "Guitar Pro did not produce: {}",
            missing.join(", ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::{is_output_path, select_outputs};

    #[test]
    fn output_paths_follow_the_golden_layout() {
        assert!(is_output_path("export.pdf"));
        assert!(is_output_path("png/amazing-grace 1.PNG"));
        assert!(!is_output_path("png/sub/x.png"));
        assert!(!is_output_path("result.json"));
        assert!(!is_output_path("diagnostics/fail-open.png"));
    }

    #[test]
    fn missing_outputs_are_named() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("export.mid"), b"m").expect("write");
        std::fs::write(dir.path().join("result.json"), b"{}").expect("write");
        let error = select_outputs(dir.path()).expect_err("incomplete");
        assert!(error.message().contains("resave.gp"));
        assert!(error.message().contains("png/*.png"));
        assert!(!error.message().contains("export.mid"));
    }
}
