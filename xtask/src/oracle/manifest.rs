use crate::oracle::error::OracleError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputEntry {
    pub path: String,
    pub sha256: Option<String>,
    pub byte_identical: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestEntry {
    pub fixture: String,
    pub sha256: String,
    pub outputs: Vec<OutputEntry>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoldenSet {
    #[default]
    Full,
    Published,
}

impl GoldenSet {
    pub fn is_full(&self) -> bool {
        *self == GoldenSet::Full
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub gp_version: String,
    #[serde(default, skip_serializing_if = "GoldenSet::is_full")]
    pub set: GoldenSet,
    pub entries: Vec<ManifestEntry>,
}

fn has_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn is_safe_relative(path: &str, separators: &[char]) -> bool {
    !path.is_empty()
        && !path.starts_with(separators)
        && !path.contains(':')
        && !has_drive_prefix(path)
        && path
            .split(separators)
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

pub fn is_safe_output_path(path: &str) -> bool {
    is_safe_relative(path, &['/', '\\'])
}

pub fn is_safe_fixture_key(key: &str) -> bool {
    is_safe_relative(key, &['/']) && !key.contains('\\')
}

impl Manifest {
    pub fn new(gp_version: impl Into<String>) -> Self {
        Self {
            gp_version: gp_version.into(),
            set: GoldenSet::Full,
            entries: Vec::new(),
        }
    }

    pub fn sorted(&self) -> Self {
        let mut copy = self.clone();
        for entry in &mut copy.entries {
            entry.outputs.sort_by(|a, b| a.path.cmp(&b.path));
        }
        copy.entries.sort_by(|a, b| a.fixture.cmp(&b.fixture));
        copy
    }

    pub fn entry(&self, fixture: &str) -> Option<&ManifestEntry> {
        self.entries.iter().find(|entry| entry.fixture == fixture)
    }

    pub fn upsert(&mut self, entry: ManifestEntry) {
        self.entries
            .retain(|existing| existing.fixture != entry.fixture);
        self.entries.push(entry);
    }

    pub fn validate(&self) -> Result<(), OracleError> {
        let mut seen = BTreeSet::new();
        for entry in &self.entries {
            if !is_safe_fixture_key(&entry.fixture) {
                return Err(OracleError::new(format!(
                    "invalid manifest: unsafe fixture name {:?}",
                    entry.fixture
                )));
            }
            if !seen.insert(entry.fixture.as_str()) {
                return Err(OracleError::new(format!(
                    "invalid manifest: fixture {} listed twice",
                    entry.fixture
                )));
            }
            for output in &entry.outputs {
                if !is_safe_output_path(&output.path) {
                    return Err(OracleError::new(format!(
                        "invalid manifest: unsafe output path {:?} in {}",
                        output.path, entry.fixture
                    )));
                }
            }
        }
        Ok(())
    }

    pub fn to_json(&self) -> Result<Vec<u8>, OracleError> {
        self.validate()?;
        let mut bytes = serde_json::to_vec_pretty(&self.sorted())
            .map_err(|error| OracleError::new(format!("cannot serialise manifest: {error}")))?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, OracleError> {
        let manifest: Self = serde_json::from_slice(bytes)
            .map_err(|error| OracleError::new(format!("invalid manifest: {error}")))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn load(path: &Path) -> Result<Self, OracleError> {
        let bytes = std::fs::read(path)
            .map_err(|error| OracleError::io(format!("cannot read {}", path.display()), &error))?;
        Self::from_json(&bytes)
            .map_err(|error| OracleError::new(format!("{}: {error}", path.display())))
    }

    pub fn save(&self, path: &Path) -> Result<(), OracleError> {
        let bytes = self.to_json()?;
        let temp = temp_path(path);
        let written = write_synced(&temp, &bytes).and_then(|()| std::fs::rename(&temp, path));
        written.map_err(|error| {
            let _ = std::fs::remove_file(&temp);
            OracleError::io(format!("cannot write {}", path.display()), &error)
        })
    }
}

fn temp_path(path: &Path) -> std::path::PathBuf {
    let mut name = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    name.push(format!(".tmp-{}", std::process::id()));
    path.with_file_name(name)
}

fn write_synced(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::{Manifest, ManifestEntry, OutputEntry};

    fn entry(fixture: &str, seed: u8) -> ManifestEntry {
        let outputs = [
            "png/page-2.png",
            "export.mid",
            "png/page-1.png",
            "resave.gp",
        ]
        .iter()
        .map(|path| OutputEntry {
            path: (*path).to_owned(),
            sha256: Some(format!("{seed:02x}{path}")),
            byte_identical: true,
        })
        .collect();
        ManifestEntry {
            fixture: fixture.to_owned(),
            sha256: format!("{seed:064x}"),
            outputs,
        }
    }

    fn permutations(items: &[ManifestEntry]) -> Vec<Vec<ManifestEntry>> {
        if items.len() <= 1 {
            return vec![items.to_vec()];
        }
        let mut all = Vec::new();
        for index in 0..items.len() {
            let mut rest = items.to_vec();
            let head = rest.remove(index);
            for mut tail in permutations(&rest) {
                tail.insert(0, head.clone());
                all.push(tail);
            }
        }
        all
    }

    fn shuffled_outputs(mut entry: ManifestEntry, rotation: usize) -> ManifestEntry {
        let len = entry.outputs.len();
        entry.outputs.rotate_left(rotation % len);
        entry
    }

    #[test]
    fn manifests_from_shuffled_input_are_byte_identical() {
        let base = vec![
            entry("songs/Canção x.gp", 1),
            entry("a b.gp", 2),
            entry("z/last.gp5", 3),
            entry("b.gpx", 4),
        ];
        let mut reference: Option<Vec<u8>> = None;
        for (index, order) in permutations(&base).into_iter().enumerate() {
            let mut manifest = Manifest::new("8.1.5.31");
            for (offset, item) in order.into_iter().enumerate() {
                manifest.upsert(shuffled_outputs(item, index + offset));
            }
            let bytes = manifest.to_json().expect("serialises");
            match &reference {
                Some(expected) => assert_eq!(&bytes, expected),
                None => reference = Some(bytes),
            }
        }
        let text = String::from_utf8(reference.expect("at least one order")).expect("utf8");
        let a = text.find("\"a b.gp\"").expect("a");
        let b = text.find("\"b.gpx\"").expect("b");
        let s = text.find("\"songs/Canção x.gp\"").expect("s");
        let z = text.find("\"z/last.gp5\"").expect("z");
        assert!(a < b && b < s && s < z);
    }

    #[test]
    fn round_trip_through_disk_is_lossless() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("manifest.json");
        let mut manifest = Manifest::new("8.1.5.31");
        manifest.upsert(entry("x.gp", 9));
        manifest.upsert(entry("ação/é.gp", 8));
        manifest.save(&path).expect("save");
        let loaded = Manifest::load(&path).expect("load");
        assert_eq!(loaded, manifest.sorted());
        let first = std::fs::read(&path).expect("read");
        loaded.save(&path).expect("save again");
        assert_eq!(std::fs::read(&path).expect("read"), first);
    }

    #[test]
    fn json_has_spec_shape_and_no_timestamps() {
        let mut manifest = Manifest::new("8.1.5.31");
        manifest.upsert(entry("x.gp", 1));
        let value: serde_json::Value =
            serde_json::from_slice(&manifest.to_json().expect("serialises")).expect("json");
        let top: Vec<&String> = value.as_object().expect("object").keys().collect();
        assert_eq!(top, ["entries", "gp_version"]);
        let first = &value["entries"][0];
        let keys: Vec<&String> = first.as_object().expect("object").keys().collect();
        assert_eq!(keys, ["fixture", "outputs", "sha256"]);
        let text = String::from_utf8(manifest.to_json().expect("serialises")).expect("utf8");
        assert!(!text.contains("time"));
        assert!(!text.contains("date"));
    }

    #[test]
    fn upsert_replaces_existing_fixture() {
        let mut manifest = Manifest::new("8.1.5.31");
        manifest.upsert(entry("x.gp", 1));
        manifest.upsert(entry("x.gp", 2));
        assert_eq!(manifest.entries.len(), 1);
        assert_eq!(manifest.entry("x.gp"), Some(&entry("x.gp", 2)));
    }

    #[test]
    fn duplicate_and_garbage_manifests_are_errors() {
        let mut manifest = Manifest::new("8.1.5.31");
        manifest.entries.push(entry("x.gp", 1));
        manifest.entries.push(entry("x.gp", 2));
        assert!(manifest.to_json().is_err());
        let forged = serde_json::to_vec(&manifest).expect("raw json");
        assert!(Manifest::from_json(&forged).is_err());
        assert!(Manifest::from_json(b"\xff\x00garbage").is_err());
        assert!(Manifest::from_json(b"{\"gp_version\":\"8\"}").is_err());
    }

    fn listing(dir: &std::path::Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .expect("list")
            .map(|item| {
                item.expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }

    fn manifest_with_output(path: &str) -> Manifest {
        let mut manifest = Manifest::new("8.1.5.31");
        let mut item = entry("x.gp", 1);
        item.outputs.push(OutputEntry {
            path: path.to_owned(),
            sha256: None,
            byte_identical: false,
        });
        manifest.upsert(item);
        manifest
    }

    #[test]
    fn unsafe_output_paths_are_rejected_everywhere() {
        let bad = [
            "../escape.mid",
            "png/../../escape.png",
            "a/./b.png",
            "/abs/export.mid",
            "\\share\\export.mid",
            "C:\\x\\export.mid",
            "C:export.mid",
            "d:/export.mid",
            "..\\export.mid",
            "png//p.png",
            "",
            "png/",
        ];
        let dir = tempfile::tempdir().expect("tempdir");
        for path in bad {
            let manifest = manifest_with_output(path);
            assert!(manifest.to_json().is_err(), "{path}");
            assert!(manifest.save(&dir.path().join("m.json")).is_err(), "{path}");
            assert!(!dir.path().join("m.json").exists(), "{path}");
            let mut valid = manifest_with_output("export.mid");
            valid.entries[0].outputs.pop();
            let text = String::from_utf8(valid.to_json().expect("valid")).expect("utf8");
            let forged = text.replace("export.mid", &path.replace('\\', "\\\\"));
            assert!(Manifest::from_json(forged.as_bytes()).is_err(), "{path}");
        }
        assert!(manifest_with_output("png/page 1.png").to_json().is_ok());
    }

    #[test]
    fn unsafe_fixture_names_are_rejected() {
        for key in ["../x.gp", "/x.gp", "a/../b.gp", "C:/x.gp", "a\\b.gp", ""] {
            let mut manifest = Manifest::new("8");
            manifest.entries.push(entry(key, 1));
            assert!(manifest.to_json().is_err(), "{key}");
        }
    }

    #[test]
    fn save_is_atomic_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("manifest.json");
        let mut first = Manifest::new("8.1.5.31");
        first.upsert(entry("a.gp", 1));
        first.save(&path).expect("save");
        let before = std::fs::read(&path).expect("read");
        let mut second = Manifest::new("8.1.5.31");
        second.upsert(entry("b.gp", 2));
        second.save(&path).expect("replace");
        assert_ne!(std::fs::read(&path).expect("read"), before);
        assert_eq!(listing(dir.path()), ["manifest.json"]);
    }

    #[test]
    fn failed_save_keeps_the_old_manifest_and_cleans_up() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("manifest.json");
        let mut good = Manifest::new("8.1.5.31");
        good.upsert(entry("a.gp", 1));
        good.save(&path).expect("save");
        let before = std::fs::read(&path).expect("read");
        let bad = manifest_with_output("../x");
        assert!(bad.save(&path).is_err());
        assert_eq!(std::fs::read(&path).expect("read"), before);
        let blocked = dir.path().join("blocked");
        std::fs::create_dir(&blocked).expect("mkdir");
        std::fs::write(blocked.join("keep"), b"x").expect("write");
        assert!(good.save(&blocked).is_err());
        let mut names = listing(dir.path());
        names.sort();
        assert_eq!(names, ["blocked", "manifest.json"]);
        assert!(!names.iter().any(|name| name.contains(".tmp-")));
    }
}
