use crate::oracle::error::OracleError;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const FIXTURE_EXTENSIONS: [&str; 5] = ["gp", "gpx", "gp5", "gp4", "gp3"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fixture {
    pub key: String,
    pub path: PathBuf,
}

impl Fixture {
    pub fn file_name(&self) -> &str {
        self.key.rsplit('/').next().unwrap_or(&self.key)
    }

    pub fn golden_dir(&self) -> String {
        match self.key.rfind('.') {
            Some(dot) if dot > self.key.rfind('/').map_or(0, |slash| slash + 1) => {
                self.key[..dot].to_owned()
            }
            _ => self.key.clone(),
        }
    }
}

fn has_fixture_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            FIXTURE_EXTENSIONS
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

fn utf8_name(path: &Path) -> Result<String, OracleError> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| OracleError::new(format!("fixture name is not UTF-8: {}", path.display())))
}

fn walk(dir: &Path, prefix: &str, found: &mut Vec<Fixture>) -> Result<(), OracleError> {
    let reader = std::fs::read_dir(dir)
        .map_err(|error| OracleError::io(format!("cannot list {}", dir.display()), &error))?;
    let mut children = BTreeMap::new();
    for item in reader {
        let item = item
            .map_err(|error| OracleError::io(format!("cannot list {}", dir.display()), &error))?;
        let path = item.path();
        children.insert(utf8_name(&path)?, path);
    }
    for (name, path) in children {
        let kind = std::fs::symlink_metadata(&path)
            .map_err(|error| OracleError::io(format!("cannot stat {}", path.display()), &error))?
            .file_type();
        let key = format!("{prefix}{name}");
        if kind.is_dir() {
            walk(&path, &format!("{key}/"), found)?;
        } else if kind.is_file() && has_fixture_extension(&path) {
            found.push(Fixture { key, path });
        }
    }
    Ok(())
}

pub fn discover(input: &Path) -> Result<Vec<Fixture>, OracleError> {
    let meta = std::fs::metadata(input).map_err(|error| {
        OracleError::io(format!("cannot read input {}", input.display()), &error)
    })?;
    let mut found = Vec::new();
    if meta.is_file() {
        if !has_fixture_extension(input) {
            return Err(OracleError::new(format!(
                "{} is not a Guitar Pro file (expected one of: {})",
                input.display(),
                FIXTURE_EXTENSIONS.join(", ")
            )));
        }
        found.push(Fixture {
            key: utf8_name(input)?,
            path: input.to_path_buf(),
        });
    } else {
        walk(input, "", &mut found)?;
    }
    let mut dirs: BTreeMap<String, String> = BTreeMap::new();
    for fixture in &found {
        if let Some(previous) = dirs.insert(fixture.golden_dir(), fixture.key.clone()) {
            return Err(OracleError::new(format!(
                "fixtures {previous} and {} would share the golden directory {}",
                fixture.key,
                fixture.golden_dir()
            )));
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::{Fixture, discover};
    use std::path::PathBuf;

    #[test]
    fn golden_dir_strips_only_the_extension() {
        let make = |key: &str| Fixture {
            key: key.to_owned(),
            path: PathBuf::from(key),
        };
        assert_eq!(make("a b/Canção.gp").golden_dir(), "a b/Canção");
        assert_eq!(make("x.y/song.gp5").golden_dir(), "x.y/song");
        assert_eq!(make("dir.v2/noext").golden_dir(), "dir.v2/noext");
        assert_eq!(make("a b/Canção.gp").file_name(), "Canção.gp");
    }

    #[test]
    fn discovers_recursively_sorted_and_filters_extensions() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("in put ç");
        std::fs::create_dir_all(root.join("sub dir")).expect("mkdir");
        for name in ["b.gp", "a.GP5", "notes.txt", "sub dir/ã.gpx"] {
            std::fs::write(root.join(name), name).expect("write");
        }
        let keys: Vec<String> = discover(&root)
            .expect("discover")
            .into_iter()
            .map(|fixture| fixture.key)
            .collect();
        assert_eq!(keys, ["a.GP5", "b.gp", "sub dir/ã.gpx"]);
        let single = discover(&root.join("b.gp")).expect("single");
        assert_eq!(single[0].key, "b.gp");
        assert!(discover(&root.join("notes.txt")).is_err());
        assert!(discover(&root.join("absent.gp")).is_err());
    }

    #[test]
    fn colliding_golden_dirs_are_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("song.gp"), b"1").expect("write");
        std::fs::write(dir.path().join("song.gp5"), b"2").expect("write");
        let error = discover(dir.path()).expect_err("collision");
        assert!(error.message().contains("share the golden directory song"));
    }
}
