use crate::oracle::error::OracleError;
use crate::oracle::fixtures::{Fixture, discover};
use crate::oracle::hash::sha256_hex;
use crate::oracle::layout::{MANIFEST_FILE, list_files};
use crate::oracle::manifest::Manifest;
use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Finding {
    Stale(String),
    Missing(String),
    Orphan(String),
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stale(what) => write!(f, "stale: {what}"),
            Self::Missing(what) => write!(f, "missing: {what}"),
            Self::Orphan(what) => write!(f, "orphan: {what}"),
        }
    }
}

fn read(path: &Path) -> Result<Vec<u8>, OracleError> {
    std::fs::read(path)
        .map_err(|error| OracleError::io(format!("cannot read {}", path.display()), &error))
}

pub fn check_fixtures(
    fixtures: &[Fixture],
    manifest: &Manifest,
    goldens: &Path,
) -> Result<Vec<Finding>, OracleError> {
    let mut findings = Vec::new();
    let mut claimed: BTreeSet<String> = BTreeSet::new();
    claimed.insert(MANIFEST_FILE.to_owned());
    for entry in &manifest.entries {
        let dir = fixture_golden_dir(&entry.fixture);
        for output in &entry.outputs {
            claimed.insert(format!("{dir}/{}", output.path));
        }
    }
    let keys: BTreeSet<&str> = fixtures
        .iter()
        .map(|fixture| fixture.key.as_str())
        .collect();
    for fixture in fixtures {
        let Some(entry) = manifest.entry(&fixture.key) else {
            findings.push(Finding::Missing(fixture.key.clone()));
            continue;
        };
        if sha256_hex(&read(&fixture.path)?) != entry.sha256 {
            findings.push(Finding::Stale(fixture.key.clone()));
            continue;
        }
        let dir = goldens.join(fixture.golden_dir());
        for output in &entry.outputs {
            let path = dir.join(&output.path);
            if !path.is_file() {
                findings.push(Finding::Missing(format!(
                    "{}: {}",
                    fixture.key, output.path
                )));
            } else if output.byte_identical && Some(sha256_hex(&read(&path)?)) != output.sha256 {
                findings.push(Finding::Stale(format!(
                    "{}: {} differs from the manifest",
                    fixture.key, output.path
                )));
            }
        }
    }
    for entry in &manifest.entries {
        if !keys.contains(entry.fixture.as_str()) {
            findings.push(Finding::Orphan(entry.fixture.clone()));
        }
    }
    for path in list_files(goldens)?.into_keys() {
        if !claimed.contains(&path) {
            findings.push(Finding::Orphan(path));
        }
    }
    findings.sort();
    Ok(findings)
}

fn fixture_golden_dir(key: &str) -> String {
    Fixture {
        key: key.to_owned(),
        path: key.into(),
    }
    .golden_dir()
}

pub fn check(input: &Path, goldens: &Path) -> Result<Vec<Finding>, OracleError> {
    let fixtures = discover(input)?;
    let manifest_path = goldens.join(MANIFEST_FILE);
    let manifest = if manifest_path.exists() {
        Manifest::load(&manifest_path)?
    } else {
        Manifest::new("")
    };
    check_fixtures(&fixtures, &manifest, goldens)
}

#[cfg(test)]
mod tests {
    use super::{Finding, check};
    use crate::oracle::hash::sha256_hex;
    use crate::oracle::manifest::{Manifest, ManifestEntry, OutputEntry};
    use std::path::Path;

    fn golden(goldens: &Path, manifest: &mut Manifest, input: &Path, key: &str) {
        let bytes = std::fs::read(input.join(key)).expect("fixture");
        let stem = key.rsplit_once('.').map_or(key, |(stem, _)| stem);
        let dir = goldens.join(stem);
        std::fs::create_dir_all(dir.join("png")).expect("mkdir");
        let mut outputs = Vec::new();
        for (path, content) in [("export.mid", b"midi".as_slice()), ("png/p1.png", b"png")] {
            std::fs::write(dir.join(path), content).expect("write");
            outputs.push(OutputEntry {
                path: path.to_owned(),
                sha256: Some(sha256_hex(content)),
                byte_identical: true,
            });
        }
        manifest.upsert(ManifestEntry {
            fixture: key.to_owned(),
            sha256: sha256_hex(&bytes),
            outputs,
        });
    }

    fn setup(names: &[&str]) -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let input = dir.path().join("in ç");
        let goldens = dir.path().join("goldens");
        std::fs::create_dir_all(&input).expect("mkdir");
        std::fs::create_dir_all(&goldens).expect("mkdir");
        let mut manifest = Manifest::new("8.1.5.31");
        for name in names {
            std::fs::write(input.join(name), format!("content of {name}")).expect("write");
            golden(&goldens, &mut manifest, &input, name);
        }
        manifest.save(&goldens.join("manifest.json")).expect("save");
        (dir, input, goldens)
    }

    #[test]
    fn fresh_goldens_have_no_findings() {
        let (_dir, input, goldens) = setup(&["a.gp", "Canção b.gp"]);
        assert_eq!(check(&input, &goldens).expect("check"), Vec::new());
    }

    #[test]
    fn one_changed_byte_with_the_same_name_is_stale() {
        let (_dir, input, goldens) = setup(&["a.gp", "b.gp"]);
        for position in 0..std::fs::read(input.join("a.gp")).expect("read").len() {
            let mut bytes = std::fs::read(input.join("a.gp")).expect("read");
            let original = bytes[position];
            bytes[position] ^= 0x01;
            std::fs::write(input.join("a.gp"), &bytes).expect("write");
            assert_eq!(
                check(&input, &goldens).expect("check"),
                vec![Finding::Stale(String::from("a.gp"))],
                "byte {position}"
            );
            bytes[position] = original;
            std::fs::write(input.join("a.gp"), &bytes).expect("write");
        }
        assert_eq!(check(&input, &goldens).expect("check"), Vec::new());
    }

    #[test]
    fn missing_and_orphan_are_reported() {
        let (_dir, input, goldens) = setup(&["a.gp", "b.gp"]);
        std::fs::remove_file(input.join("b.gp")).expect("remove");
        std::fs::write(input.join("c.gp"), b"new").expect("write");
        std::fs::remove_file(goldens.join("a/png/p1.png")).expect("remove");
        std::fs::write(goldens.join("a/stray.txt"), b"x").expect("write");
        let lines: Vec<String> = check(&input, &goldens)
            .expect("check")
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            lines,
            [
                "missing: a.gp: png/p1.png",
                "missing: c.gp",
                "orphan: a/stray.txt",
                "orphan: b.gp",
            ]
        );
    }

    #[test]
    fn edited_golden_output_is_stale() {
        let (_dir, input, goldens) = setup(&["a.gp"]);
        std::fs::write(goldens.join("a/export.mid"), b"midI").expect("write");
        assert_eq!(
            check(&input, &goldens).expect("check"),
            vec![Finding::Stale(String::from(
                "a.gp: export.mid differs from the manifest"
            ))]
        );
    }

    #[test]
    fn absent_manifest_reports_every_fixture_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("x.gp"), b"x").expect("write");
        let goldens = dir.path().join("none");
        assert_eq!(
            check(dir.path(), &goldens).expect("check"),
            vec![Finding::Missing(String::from("x.gp"))]
        );
    }
}
