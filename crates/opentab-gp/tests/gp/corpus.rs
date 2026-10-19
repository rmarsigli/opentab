use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use opentab_gp::{read_gp, write_gp};

use crate::anchors::gpif_text;
use crate::inventory::{
    first_difference as subtree_difference, inventory, master_bar_children, ordered_directions,
};
use opentab_model::{
    Accent, AutomationValue, Clef, Diagnostic, Dynamic, Fraction, Score, TempoUnit, diagnose,
    from_json, to_json,
};

const CORPUS_VARIABLE: &str = "OPENTAB_CORPUS";
const EXPECTED_FILES: usize = 27;
const EXPECTED_TOTALS: [(&str, usize); 36] = [
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
    ("linear_tempos", 4),
    ("inexact_tempos", 0),
    ("tempos_beyond_three_decimals", 1),
    ("sounds", 66),
    ("sound_automations", 67),
    ("volume_automations", 79),
    ("pan_automations", 65),
    ("instrument_sets", 64),
    ("midi_connections_not_default", 54),
    ("direction_bars", 8),
    ("target_segno", 2),
    ("target_coda", 2),
    ("jump_da_coda", 2),
    ("jump_da_segno", 1),
    ("jump_da_segno_al_coda", 1),
    ("direction_payloads", 0),
];
const THOUSANDTHS: i64 = 1000;
const EXPECTED_DIAGNOSTICS: [(&str, usize); 1] = [("IncompleteBar", 14)];

fn diagnostic_kind(diagnostic: &Diagnostic) -> String {
    let debug = format!("{diagnostic:?}");
    debug
        .split([' ', '{', '('])
        .next()
        .unwrap_or_default()
        .to_string()
}

fn snake<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
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
        for change in &master_bar.tempo_changes {
            count(totals, "linear_tempos", usize::from(change.linear));
            let exact = change.bpm.exact();
            count(totals, "inexact_tempos", usize::from(exact.is_none()));
            let beyond = exact.is_some_and(|bpm| THOUSANDTHS % *bpm.denom() != 0);
            count(totals, "tempos_beyond_three_decimals", usize::from(beyond));
        }
        let directions = &master_bar.directions;
        count(
            totals,
            "direction_bars",
            usize::from(!directions.is_empty()),
        );
        for target in &directions.targets {
            count(totals, &format!("target_{}", snake(target)), 1);
        }
        for jump in &directions.jumps {
            count(totals, &format!("jump_{}", snake(jump)), 1);
        }
        let replayed = score
            .extensions
            .0
            .get(&master_bar.id.0)
            .map_or(0, |payloads| {
                payloads
                    .iter()
                    .filter(|payload| payload.data.contains("<Directions"))
                    .count()
            });
        count(totals, "direction_payloads", replayed);
    }
    for track in &score.tracks {
        count(totals, "sounds", track.sounds.len());
        count(
            totals,
            "instrument_sets",
            usize::from(track.instrument.set.is_some()),
        );
        count(
            totals,
            "midi_connections_not_default",
            usize::from(track.midi.secondary != track.midi.primary),
        );
        for automation in &track.automations {
            let name = match automation.value {
                AutomationValue::Sound(_) => "sound_automations",
                AutomationValue::Volume(_) => "volume_automations",
                AutomationValue::Pan(_) => "pan_automations",
            };
            count(totals, name, 1);
        }
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

pub fn corpus_paths() -> Option<Vec<PathBuf>> {
    let directory = std::env::var(CORPUS_VARIABLE).ok()?;
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&directory)
        .expect("corpus directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "gp"))
        .collect();
    paths.sort();
    Some(paths)
}

fn first_difference(left: &Score, right: &Score) -> String {
    let left = to_json(left).unwrap_or_default();
    let right = to_json(right).unwrap_or_default();
    let at = left
        .chars()
        .zip(right.chars())
        .position(|(a, b)| a != b)
        .unwrap_or(left.len().min(right.len()));
    let start = at.saturating_sub(300);
    let left: String = left.chars().skip(start).take(600).collect();
    let right: String = right.chars().skip(start).take(600).collect();
    format!("\nsource: {left}\nreread: {right}")
}

#[test]
fn every_corpus_example_survives_a_write() {
    let Some(paths) = corpus_paths() else {
        return;
    };
    assert_eq!(paths.len(), EXPECTED_FILES);
    let mut failures = Vec::new();
    for path in &paths {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let bytes = std::fs::read(path).expect("corpus file");
        let score = read_gp(&bytes).expect("corpus reads");
        let written = match write_gp(&score) {
            Ok(written) => written,
            Err(error) => {
                failures.push(format!("{name}: write: {error}"));
                continue;
            }
        };
        assert_eq!(write_gp(&score).as_ref(), Ok(&written), "{name}");
        if let Some(difference) = subtree_difference(
            &inventory(&gpif_text(&bytes)),
            &inventory(&gpif_text(&written)),
        ) {
            failures.push(format!("{name}: typed subtree changed: {difference}"));
            continue;
        }
        let (source, rewritten) = (gpif_text(&bytes), gpif_text(&written));
        if ordered_directions(&source) != ordered_directions(&rewritten) {
            failures.push(format!("{name}: the order inside Directions changed"));
            continue;
        }
        if master_bar_children(&source) != master_bar_children(&rewritten) {
            failures.push(format!("{name}: the master bar child order changed"));
            continue;
        }
        match read_gp(&written) {
            Ok(again) if again == score => {
                println!("corpus {name}: {} -> {} bytes", bytes.len(), written.len());
            }
            Ok(again) => failures.push(format!(
                "{name}: differs{}",
                first_difference(&score, &again)
            )),
            Err(error) => failures.push(format!("{name}: reread: {error}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

const POSITION_HITS: [(&str, usize, usize); 3] =
    [("tempo", 4, 1), ("strip", 4, 2), ("sound", 1, 0)];
const QUARTERS_PER_WHOLE: i64 = 4;

fn onsets(score: &Score, index: usize) -> BTreeSet<Fraction> {
    let mut found = BTreeSet::new();
    for track in &score.tracks {
        for staff in &track.staves {
            let Some(bar) = staff.bars.get(index) else {
                continue;
            };
            for voice in &bar.voices {
                let mut onset = Fraction::from_integer(0);
                for beat in &voice.beats {
                    found.insert(onset);
                    if beat.effects.grace.is_none() {
                        onset += beat.duration.length();
                    }
                }
            }
        }
    }
    found
}

struct Hits {
    ours: usize,
    other: usize,
}

fn tally_hits(hits: &mut BTreeMap<&'static str, Hits>, kind: &'static str, on: [bool; 2]) {
    let entry = hits.entry(kind).or_insert(Hits { ours: 0, other: 0 });
    entry.ours += usize::from(on[0]);
    entry.other += usize::from(on[1]);
}

#[test]
fn corpus_automations_inside_a_bar_land_on_beats_in_the_units_we_read() {
    let Some(paths) = corpus_paths() else {
        return;
    };
    let mut hits: BTreeMap<&'static str, Hits> = BTreeMap::new();
    for path in &paths {
        let score = read_gp(&std::fs::read(path).expect("corpus file")).expect("reads");
        let zero = Fraction::from_integer(0);
        for (index, master) in score.master_bars.iter().enumerate() {
            let length = master.time_signature.bar_length();
            let beats = onsets(&score, index);
            for change in &master.tempo_changes {
                if change.position > zero && change.position < length {
                    let quarters = change.position / length / QUARTERS_PER_WHOLE;
                    let on = [beats.contains(&change.position), beats.contains(&quarters)];
                    tally_hits(&mut hits, "tempo", on);
                }
            }
        }
        for track in &score.tracks {
            for automation in &track.automations {
                let Some(index) = score.find_master_bar_index(automation.master_bar) else {
                    continue;
                };
                let length = score.master_bars[index].time_signature.bar_length();
                let position = automation.position;
                if position <= zero || position >= length {
                    continue;
                }
                let beats = onsets(&score, index);
                let (kind, other) = match automation.value {
                    AutomationValue::Sound(_) => ("sound", position * QUARTERS_PER_WHOLE * length),
                    _ => ("strip", position / length / QUARTERS_PER_WHOLE),
                };
                tally_hits(
                    &mut hits,
                    kind,
                    [beats.contains(&position), beats.contains(&other)],
                );
            }
        }
    }
    for (kind, ours, other) in POSITION_HITS {
        let found = hits
            .get(kind)
            .expect("the corpus has this kind inside a bar");
        assert_eq!((found.ours, found.other), (ours, other), "{kind}");
        assert!(found.ours > found.other, "{kind}");
    }
}
