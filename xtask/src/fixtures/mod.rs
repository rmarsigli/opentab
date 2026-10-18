mod effects;
mod music;
mod sketch;

use std::fmt;
use std::path::{Path, PathBuf};

use opentab_gp::{GpError, write_gp};
use opentab_model::{ModelError, Score};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureError {
    message: String,
}

impl FixtureError {
    pub fn new(message: String) -> FixtureError {
        FixtureError { message }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for FixtureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl From<ModelError> for FixtureError {
    fn from(error: ModelError) -> Self {
        FixtureError::new(format!("model: {error}"))
    }
}

impl From<GpError> for FixtureError {
    fn from(error: GpError) -> Self {
        FixtureError::new(format!("gp writer: {error}"))
    }
}

pub struct Family {
    pub file: &'static str,
    pub score: Score,
}

type Builder = fn() -> Result<Score, FixtureError>;

const BUILDERS: [(&str, Builder); 11] = [
    ("metadata.gp", music::metadata),
    ("signatures.gp", music::signatures),
    ("repeats.gp", music::repeats),
    ("tempo.gp", music::tempo),
    ("tunings.gp", music::tunings),
    ("durations.gp", music::durations),
    ("beat-effects.gp", effects::beat_effects),
    ("note-effects.gp", effects::note_effects),
    ("multi-voice.gp", music::multi_voice),
    ("multi-track.gp", music::multi_track),
    ("from-scratch.gp", music::from_scratch),
];

pub fn families() -> Result<Vec<Family>, FixtureError> {
    let mut families = Vec::new();
    for (file, build) in BUILDERS {
        let score = build().map_err(|error| FixtureError::new(format!("{file}: {error}")))?;
        families.push(Family { file, score });
    }
    Ok(families)
}

pub struct Written {
    pub file: &'static str,
    pub bytes: Vec<u8>,
}

pub fn render() -> Result<Vec<Written>, FixtureError> {
    let mut written = Vec::new();
    for family in families()? {
        let bytes = write_gp(&family.score)
            .map_err(|error| FixtureError::new(format!("{}: {error}", family.file)))?;
        written.push(Written {
            file: family.file,
            bytes,
        });
    }
    Ok(written)
}

pub fn write_fixtures(out: &Path) -> Result<Vec<PathBuf>, FixtureError> {
    let rendered = render()?;
    std::fs::create_dir_all(out)
        .map_err(|error| FixtureError::new(format!("cannot create {}: {error}", out.display())))?;
    let mut paths = Vec::new();
    for item in rendered {
        let path = out.join(item.file);
        std::fs::write(&path, &item.bytes).map_err(|error| {
            FixtureError::new(format!("cannot write {}: {error}", path.display()))
        })?;
        paths.push(path);
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::{families, render, write_fixtures};
    use opentab_gp::read_gp;
    use opentab_model::{Diagnostic, diagnose};
    use std::path::Path;

    #[test]
    fn rendering_twice_gives_identical_bytes() {
        let first = render().expect("render");
        let second = render().expect("render");
        assert_eq!(first.len(), second.len());
        for (one, two) in first.iter().zip(&second) {
            assert_eq!(one.file, two.file);
            assert_eq!(one.bytes, two.bytes, "{}", one.file);
        }
    }

    #[test]
    fn committed_fixtures_match_the_generator() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("fixtures")
            .join("gp");
        let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
            .expect("fixtures/gp")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        on_disk.sort();
        let rendered = render().expect("render");
        let mut names: Vec<String> = rendered.iter().map(|item| item.file.to_string()).collect();
        names.sort();
        assert_eq!(on_disk, names);
        for item in rendered {
            let committed = std::fs::read(dir.join(item.file)).expect("fixture");
            assert!(
                committed == item.bytes,
                "{} is stale; run cargo xtask fixtures",
                item.file
            );
        }
    }

    #[test]
    fn every_family_rewrites_identically_and_is_well_formed() {
        for family in families().expect("families") {
            let bytes = opentab_gp::write_gp(&family.score).expect("write");
            let again = opentab_gp::write_gp(&read_gp(&bytes).expect("read")).expect("rewrite");
            assert!(again == bytes, "{}", family.file);
            let diagnostics = diagnose(&family.score);
            if family.file == "multi-voice.gp" {
                assert!(
                    matches!(
                        diagnostics.as_slice(),
                        [Diagnostic::BarWithoutVoices { .. }]
                    ),
                    "{diagnostics:?}"
                );
            } else {
                assert_eq!(diagnostics, Vec::new(), "{}", family.file);
            }
        }
    }

    #[test]
    fn write_fixtures_writes_one_file_per_family() {
        let dir = tempfile::tempdir().expect("tempdir");
        let out = dir.path().join("nested").join("gp");
        let paths = write_fixtures(&out).expect("write");
        assert_eq!(paths.len(), families().expect("families").len());
        for path in paths {
            assert!(path.is_file(), "{}", path.display());
        }
    }
}
