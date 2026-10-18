use crate::oracle::error::OracleError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputEntry {
    pub path: String,
    pub sha256: String,
    pub byte_identical: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestEntry {
    pub fixture: String,
    pub sha256: String,
    pub outputs: Vec<OutputEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub gp_version: String,
    pub entries: Vec<ManifestEntry>,
}

impl Manifest {
    pub fn new(gp_version: impl Into<String>) -> Self {
        Self {
            gp_version: gp_version.into(),
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

    pub fn to_json(&self) -> Vec<u8> {
        let mut bytes = serde_json::to_vec_pretty(&self.sorted()).unwrap_or_default();
        bytes.push(b'\n');
        bytes
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, OracleError> {
        let manifest: Self = serde_json::from_slice(bytes)
            .map_err(|error| OracleError::new(format!("invalid manifest: {error}")))?;
        let mut seen = BTreeSet::new();
        for entry in &manifest.entries {
            if !seen.insert(entry.fixture.as_str()) {
                return Err(OracleError::new(format!(
                    "invalid manifest: fixture {} listed twice",
                    entry.fixture
                )));
            }
        }
        Ok(manifest)
    }

    pub fn load(path: &Path) -> Result<Self, OracleError> {
        let bytes = std::fs::read(path)
            .map_err(|error| OracleError::io(format!("cannot read {}", path.display()), &error))?;
        Self::from_json(&bytes)
            .map_err(|error| OracleError::new(format!("{}: {error}", path.display())))
    }

    pub fn save(&self, path: &Path) -> Result<(), OracleError> {
        std::fs::write(path, self.to_json())
            .map_err(|error| OracleError::io(format!("cannot write {}", path.display()), &error))
    }
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
            sha256: format!("{seed:02x}{path}"),
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
            let bytes = manifest.to_json();
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
        let value: serde_json::Value = serde_json::from_slice(&manifest.to_json()).expect("json");
        let top: Vec<&String> = value.as_object().expect("object").keys().collect();
        assert_eq!(top, ["entries", "gp_version"]);
        let first = &value["entries"][0];
        let keys: Vec<&String> = first.as_object().expect("object").keys().collect();
        assert_eq!(keys, ["fixture", "outputs", "sha256"]);
        let text = String::from_utf8(manifest.to_json()).expect("utf8");
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
        assert!(Manifest::from_json(&manifest.to_json()).is_err());
        assert!(Manifest::from_json(b"\xff\x00garbage").is_err());
        assert!(Manifest::from_json(b"{\"gp_version\":\"8\"}").is_err());
    }
}
