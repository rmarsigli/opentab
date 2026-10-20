mod directions;
mod drums;
mod effects;
mod midi;
mod music;
mod playing;
mod sketch;
mod swing;
mod unisons;

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

struct Recipe {
    file: &'static str,
    build: Builder,
}

const fn recipe(file: &'static str, build: Builder) -> Recipe {
    Recipe { file, build }
}

const BUILDERS: [Recipe; 35] = [
    recipe("metadata.gp", music::metadata),
    recipe("signatures.gp", music::signatures),
    recipe("repeats.gp", music::repeats),
    recipe("tempo.gp", music::tempo),
    recipe("tunings.gp", music::tunings),
    recipe("durations.gp", music::durations),
    recipe("beat-effects.gp", effects::beat_effects),
    recipe("note-effects.gp", effects::note_effects),
    recipe("ghost-accent.gp", effects::ghost_accent),
    recipe("multi-voice.gp", music::multi_voice),
    recipe("multi-track.gp", music::multi_track),
    recipe("from-scratch.gp", music::from_scratch),
    recipe("sound-change.gp", midi::sound_change),
    recipe("mixer.gp", midi::mixer),
    recipe("sound-override.gp", midi::sound_override),
    recipe("directions-dc-fine.gp", directions::dc_fine),
    recipe("directions-ds-coda.gp", directions::ds_coda),
    recipe("directions-dss-double-coda.gp", directions::dss_double_coda),
    recipe("directions-repeat-jumps.gp", directions::repeat_jumps),
    recipe("directions-endings.gp", directions::endings),
    recipe("directions-coda-held.gp", directions::coda_held),
    recipe("directions-coda-free.gp", directions::coda_free),
    recipe("directions-fine-ahead.gp", directions::fine_ahead),
    recipe("directions-dangling.gp", directions::dangling),
    recipe("directions-duplicates.gp", directions::duplicates),
    recipe("directions-coda-endings.gp", directions::coda_endings),
    recipe("directions-fine-repeats.gp", directions::fine_repeats),
    recipe("tempo-ramp.gp", playing::tempo_ramp),
    recipe("tremolo-bar.gp", playing::tremolo_bar),
    recipe("trill.gp", playing::trill),
    recipe("drums.gp", drums::drums),
    recipe("tempo-self-repeats.gp", directions::tempo_self_repeats),
    recipe("tempo-forward-jump.gp", directions::tempo_forward_jump),
    recipe("triplet-feel.gp", swing::triplet_feel),
    recipe("unisons.gp", unisons::unisons),
];

pub const PLAY_ORDERS: &str = include_str!("play-orders.json");

pub fn families() -> Result<Vec<Family>, FixtureError> {
    let mut families = Vec::new();
    for Recipe { file, build } in BUILDERS {
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
    use opentab_model::diagnose;
    use std::collections::BTreeMap;
    use std::path::Path;

    struct Diagnosed {
        file: &'static str,
        count: usize,
    }

    const fn diagnosed(file: &'static str, count: usize) -> Diagnosed {
        Diagnosed { file, count }
    }

    const DIAGNOSED: [Diagnosed; 5] = [
        diagnosed("directions-dangling.gp", 3),
        diagnosed("directions-duplicates.gp", 2),
        diagnosed("directions-coda-free.gp", 2),
        diagnosed("directions-coda-endings.gp", 1),
        diagnosed("directions-endings.gp", 1),
    ];
    const DIRECTIONS_PREFIX: &str = "directions-";

    #[test]
    fn every_directions_fixture_has_its_predicted_play_order() {
        let orders: BTreeMap<String, Vec<Vec<usize>>> =
            serde_json::from_str(super::PLAY_ORDERS).expect("play orders parse");
        let families = families().expect("families");
        let directed: Vec<&super::Family> = families
            .iter()
            .filter(|family| family.file.starts_with(DIRECTIONS_PREFIX))
            .collect();
        let named: Vec<&str> = directed.iter().map(|family| family.file).collect();
        let listed: Vec<&str> = orders.keys().map(String::as_str).collect();
        let mut sorted = named.clone();
        sorted.sort_unstable();
        assert_eq!(listed, sorted);
        for family in directed {
            let alternatives = &orders[family.file];
            assert_eq!(alternatives.len(), 1, "{}", family.file);
            let bars = family.score.master_bars.len();
            for order in alternatives {
                assert_eq!(order.first(), Some(&0), "{}", family.file);
                assert!(
                    order.iter().all(|bar| *bar < bars),
                    "{}: {order:?}",
                    family.file
                );
            }
        }
    }

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
            let expected = DIAGNOSED
                .iter()
                .find(|known| known.file == family.file)
                .map_or(0, |known| known.count);
            assert_eq!(diagnose(&family.score).len(), expected, "{}", family.file);
        }
    }

    #[test]
    fn every_committed_fixture_reads_back_as_the_built_score() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("fixtures")
            .join("gp");
        for family in families().expect("families") {
            let bytes = std::fs::read(dir.join(family.file)).expect("fixture");
            assert_eq!(
                read_gp(&bytes).expect("read"),
                family.score,
                "{}",
                family.file
            );
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
