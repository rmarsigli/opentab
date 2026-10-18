use std::collections::BTreeMap;
use std::path::PathBuf;

use opentab_gp::read_gp;
use opentab_model::{
    Accent, Clef, Diagnostic, Dynamic, Score, TempoUnit, diagnose, from_json, to_json,
};

const CORPUS_VARIABLE: &str = "OPENTAB_CORPUS";
const EXPECTED_FILES: usize = 27;
const EXPECTED_TOTALS: [(&str, usize); 20] = [
    ("tracks", 64),
    ("master_bars", 1260),
    ("bars", 2392),
    ("voices", 3090),
    ("beats", 15224),
    ("notes", 17222),
    ("bends", 52),
    ("slides", 263),
    ("tie_origins", 932),
    ("strokes", 55),
    ("staccato", 557),
    ("accent_normal", 216),
    ("accent_heavy", 206),
    ("hopo_destination_payloads", 964),
    ("harmonics", 117),
    ("chord_names", 654),
    ("grace_beats", 69),
    ("dynamics_not_mf", 4998),
    ("clefs_not_treble", 618),
    ("tempo_units_not_quarter", 2),
];
const EXPECTED_DIAGNOSTICS: [(&str, usize); 2] = [("BarWithoutVoices", 59), ("IncompleteBar", 14)];

fn diagnostic_kind(diagnostic: &Diagnostic) -> String {
    let debug = format!("{diagnostic:?}");
    debug
        .split([' ', '{', '('])
        .next()
        .unwrap_or_default()
        .to_string()
}

fn count(totals: &mut BTreeMap<String, usize>, name: &str, amount: usize) {
    *totals.entry(name.to_string()).or_default() += amount;
}

fn tally(score: &Score, totals: &mut BTreeMap<String, usize>) {
    count(totals, "tracks", score.tracks.len());
    count(totals, "master_bars", score.master_bars.len());
    for master_bar in &score.master_bars {
        let units = master_bar
            .tempo_changes
            .iter()
            .filter(|change| change.unit != TempoUnit::Quarter)
            .count();
        count(totals, "tempo_units_not_quarter", units);
    }
    for track in &score.tracks {
        for staff in &track.staves {
            count(totals, "bars", staff.bars.len());
            for bar in &staff.bars {
                count(totals, "voices", bar.voices.len());
                count(
                    totals,
                    "clefs_not_treble",
                    usize::from(bar.clef != Clef::Treble),
                );
                for voice in &bar.voices {
                    count(totals, "beats", voice.beats.len());
                    for beat in &voice.beats {
                        let effects = &beat.effects;
                        let chord = effects.chord_name.is_some();
                        count(totals, "strokes", usize::from(effects.stroke.is_some()));
                        count(totals, "chord_names", usize::from(chord));
                        count(totals, "grace_beats", usize::from(effects.grace.is_some()));
                        let loud = effects.dynamic != Dynamic::Mf;
                        count(totals, "dynamics_not_mf", usize::from(loud));
                        count(totals, "notes", beat.notes.len());
                        for note in &beat.notes {
                            let effects = &note.effects;
                            count(totals, "bends", usize::from(effects.bend.is_some()));
                            count(totals, "harmonics", usize::from(effects.harmonic.is_some()));
                            count(totals, "slides", usize::from(effects.slide.is_some()));
                            count(totals, "tie_origins", usize::from(effects.tie_origin));
                            count(totals, "staccato", usize::from(effects.staccato));
                            let normal = effects.accent == Some(Accent::Normal);
                            let heavy = effects.accent == Some(Accent::Heavy);
                            count(totals, "accent_normal", usize::from(normal));
                            count(totals, "accent_heavy", usize::from(heavy));
                            let destinations =
                                score.extensions.0.get(&note.id.0).map_or(0, |payloads| {
                                    payloads
                                        .iter()
                                        .filter(|payload| {
                                            payload.data.contains("name=\"HopoDestination\"")
                                        })
                                        .count()
                                });
                            count(totals, "hopo_destination_payloads", destinations);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn every_corpus_example_reads() {
    let Ok(directory) = std::env::var(CORPUS_VARIABLE) else {
        return;
    };
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&directory)
        .expect("corpus directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "gp"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), EXPECTED_FILES);
    let mut failures = Vec::new();
    let mut totals = BTreeMap::new();
    let mut diagnostics: BTreeMap<String, usize> = BTreeMap::new();
    for path in &paths {
        let bytes = std::fs::read(path).expect("corpus file");
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        match read_gp(&bytes) {
            Ok(score) => {
                let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
                for diagnostic in diagnose(&score) {
                    *kinds.entry(diagnostic_kind(&diagnostic)).or_default() += 1;
                }
                for (kind, amount) in &kinds {
                    *diagnostics.entry(kind.clone()).or_default() += amount;
                }
                tally(&score, &mut totals);
                let json = to_json(&score).expect("json");
                assert_eq!(from_json(&json).expect("json back"), score, "{name}");
                println!("corpus {name}: {kinds:?}");
            }
            Err(error) => failures.push(format!("{name}: {error}")),
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
    let expected: BTreeMap<String, usize> = EXPECTED_TOTALS
        .iter()
        .map(|(name, amount)| (name.to_string(), *amount))
        .collect();
    assert_eq!(totals, expected);
    let expected: BTreeMap<String, usize> = EXPECTED_DIAGNOSTICS
        .iter()
        .map(|(name, amount)| (name.to_string(), *amount))
        .collect();
    assert_eq!(diagnostics, expected);
}
