use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use opentab_gp::{GpError, Refusal, read_gp, write_gp};
use opentab_model::{NoteKind, OpaquePayload, Score};
use opentab_testkit::{ScoreGenConfig, arb_score, arb_score_with};
use proptest::prelude::*;
use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};

use crate::support::documented;

const CASES: u32 = 256;

fn config() -> Config {
    Config {
        cases: CASES,
        failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
        ..Config::default()
    }
}

fn writable() -> ScoreGenConfig {
    ScoreGenConfig {
        gp_writable: true,
        ..ScoreGenConfig::default()
    }
}

fn round_trip(score: &Score) -> Result<bool, TestCaseError> {
    match write_gp(score) {
        Ok(bytes) => {
            let again = write_gp(score).map_err(|error| TestCaseError::fail(error.to_string()))?;
            prop_assert_eq!(&bytes, &again);
            let read = read_gp(&bytes).map_err(|error| TestCaseError::fail(error.to_string()))?;
            prop_assert_eq!(&read, score);
            Ok(true)
        }
        Err(GpError::Unsupported { reason, .. }) => {
            prop_assert!(documented(&reason), "{reason:?}");
            Ok(false)
        }
        Err(other) => Err(TestCaseError::fail(format!("unexpected error {other}"))),
    }
}

#[derive(Default)]
struct Coverage {
    notes: Cell<usize>,
    percussion: Cell<usize>,
    payloads: Cell<usize>,
    tempo_changes: Cell<usize>,
    containers: Cell<usize>,
    second_staves: Cell<usize>,
    markup: Cell<usize>,
    chords: Cell<usize>,
}

impl Coverage {
    fn add(cell: &Cell<usize>, amount: usize) {
        cell.set(cell.get() + amount);
    }

    fn record(&self, score: &Score) {
        let payloads: usize = score.extensions.0.values().map(Vec::len).sum();
        Coverage::add(&self.payloads, payloads + score.document_extensions.len());
        Coverage::add(&self.containers, usize::from(!score.container.is_empty()));
        Coverage::add(
            &self.markup,
            usize::from(score.meta.title.contains(['<', '&', ']'])),
        );
        for master in &score.master_bars {
            Coverage::add(&self.tempo_changes, master.tempo_changes.len());
        }
        for track in &score.tracks {
            Coverage::add(&self.second_staves, track.staves.len().saturating_sub(1));
            for staff in &track.staves {
                for bar in &staff.bars {
                    for voice in &bar.voices {
                        for beat in &voice.beats {
                            let chord = beat.effects.chord_name.is_some();
                            Coverage::add(&self.chords, usize::from(chord));
                            for note in &beat.notes {
                                Coverage::add(&self.notes, 1);
                                let drum = matches!(note.kind, NoteKind::Percussion { .. });
                                Coverage::add(&self.percussion, usize::from(drum));
                            }
                        }
                    }
                }
            }
        }
    }

    fn counts(&self) -> [(&'static str, usize); 8] {
        [
            ("notes", self.notes.get()),
            ("percussion notes", self.percussion.get()),
            ("payloads", self.payloads.get()),
            ("tempo changes", self.tempo_changes.get()),
            ("containers", self.containers.get()),
            ("second staves", self.second_staves.get()),
            ("markup titles", self.markup.get()),
            ("chord names", self.chords.get()),
        ]
    }
}

struct Outcome {
    accepted: usize,
    seen: usize,
    coverage: Coverage,
}

fn run<S: Strategy<Value = Score>>(strategy: S) -> Outcome {
    let seen = Cell::new(0usize);
    let accepted = Cell::new(0usize);
    let coverage = Coverage::default();
    let mut runner = TestRunner::new(config());
    let outcome = runner.run(&strategy, |score| {
        seen.set(seen.get() + 1);
        if round_trip(&score)? {
            accepted.set(accepted.get() + 1);
            coverage.record(&score);
        }
        Ok(())
    });
    if let Err(error) = outcome {
        panic!("{error}");
    }
    Outcome {
        accepted: accepted.get(),
        seen: seen.get(),
        coverage,
    }
}

#[test]
fn writable_scores_read_back_equal() {
    let outcome = run(arb_score_with(writable()));
    println!(
        "acceptance gp-writable: {}/{} {:?}",
        outcome.accepted,
        outcome.seen,
        outcome.coverage.counts()
    );
    assert_eq!(outcome.accepted, outcome.seen);
    for (name, count) in outcome.coverage.counts() {
        assert!(count > 0, "no accepted score had {name}");
    }
}

const GENUINE_LIMITS: [&str; 5] = [
    "Character",
    "HarmonicFret",
    "Pitch",
    "RightHandOpen",
    "TempoPosition",
];
const MAX_ROUNDS: usize = 8;
const GRID: i64 = 3840;
const MAX_MIDI: u16 = 127;

fn kind_of(reason: &Refusal) -> String {
    let debug = format!("{reason:?}");
    debug
        .split([' ', '{'])
        .next()
        .unwrap_or_default()
        .to_string()
}

fn xml_char(character: char) -> bool {
    matches!(character,
        '\t' | '\n' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..)
}

fn clean(text: &mut String) -> bool {
    if text.chars().all(xml_char) {
        return false;
    }
    *text = text
        .chars()
        .map(|character| if xml_char(character) { character } else { '?' })
        .collect();
    true
}

fn clean_texts(score: &mut Score) -> bool {
    let meta = &mut score.meta;
    let mut changed = false;
    for text in [
        &mut meta.title,
        &mut meta.subtitle,
        &mut meta.artist,
        &mut meta.album,
        &mut meta.words,
        &mut meta.music,
        &mut meta.copyright,
        &mut meta.tab_author,
        &mut meta.instructions,
        &mut meta.notices,
    ] {
        changed |= clean(text);
    }
    for master in &mut score.master_bars {
        if let Some(marker) = &mut master.marker {
            changed |= clean(&mut marker.text);
            changed |= clean(&mut marker.letter);
        }
    }
    for track in &mut score.tracks {
        changed |= clean(&mut track.name);
        changed |= clean(&mut track.short_name);
        for beat in beats_of(track) {
            if let Some(text) = &mut beat.effects.text {
                changed |= clean(text);
            }
            if let Some(name) = &mut beat.effects.chord_name {
                changed |= clean(name);
            }
        }
    }
    changed
}

fn beats_of(track: &mut opentab_model::Track) -> impl Iterator<Item = &mut opentab_model::Beat> {
    track.staves.iter_mut().flat_map(|staff| {
        staff.bars.iter_mut().flat_map(|bar| {
            bar.voices
                .iter_mut()
                .flat_map(|voice| voice.beats.iter_mut())
        })
    })
}

fn terminating(denom: i64) -> bool {
    let mut rest = denom;
    while rest % 2 == 0 {
        rest /= 2;
    }
    while rest % 5 == 0 {
        rest /= 5;
    }
    rest == 1
}

fn clean_kind(score: &mut Score, kind: &str) -> bool {
    let mut changed = false;
    match kind {
        "Character" => changed = clean_texts(score),
        "TempoPosition" => {
            for master in &mut score.master_bars {
                let bar = master.time_signature.bar_length();
                let before = master.tempo_changes.len();
                master.tempo_changes.retain(|change| {
                    (change.position * GRID).is_integer() && change.position < bar
                });
                changed |= master.tempo_changes.len() != before;
            }
        }
        "HarmonicFret" | "RightHandOpen" => {
            for track in &mut score.tracks {
                for beat in beats_of(track) {
                    for note in &mut beat.notes {
                        let effects = &mut note.effects;
                        if kind == "HarmonicFret"
                            && effects
                                .harmonic
                                .is_some_and(|harmonic| !terminating(*harmonic.fret.denom()))
                        {
                            effects.harmonic = None;
                            changed = true;
                        }
                        if kind == "RightHandOpen"
                            && effects.right_hand_finger == Some(opentab_model::Finger::Open)
                        {
                            effects.right_hand_finger = None;
                            changed = true;
                        }
                    }
                }
            }
        }
        "Pitch" => {
            for track in &mut score.tracks {
                let tuning: Vec<u16> = track
                    .tuning
                    .strings
                    .iter()
                    .map(|pitch| u16::from(pitch.0))
                    .collect();
                let capo = u16::from(track.capo);
                let mut lower_capo = false;
                for beat in beats_of(track) {
                    for note in &mut beat.notes {
                        if let NoteKind::Fretted { string, fret } = &mut note.kind {
                            let open = tuning.get(usize::from(*string)).copied().unwrap_or(0);
                            if open + capo > MAX_MIDI {
                                lower_capo = true;
                            } else if open + capo + u16::from(*fret) > MAX_MIDI {
                                *fret = 0;
                                changed = true;
                            }
                        }
                    }
                }
                if lower_capo {
                    track.capo = 0;
                    changed = true;
                }
            }
        }
        _ => {}
    }
    changed
}

fn stripped(mut score: Score) -> Score {
    score.container.clear();
    score.extensions = opentab_model::Extensions::default();
    score.document_extensions.clear();
    score
}

#[test]
fn default_scores_are_refused_only_for_genuine_gp_limits() {
    let accepted = Cell::new(0usize);
    let seen = Cell::new(0usize);
    let observed: RefCell<BTreeMap<String, usize>> = RefCell::new(BTreeMap::new());
    let mut runner = TestRunner::new_with_rng(
        config(),
        proptest::test_runner::TestRng::deterministic_rng(config().rng_algorithm),
    );
    let outcome = runner.run(&arb_score().prop_map(stripped), |original| {
        seen.set(seen.get() + 1);
        let mut score = original.clone();
        for round in 0..MAX_ROUNDS {
            match write_gp(&score) {
                Ok(bytes) => {
                    let read =
                        read_gp(&bytes).map_err(|error| TestCaseError::fail(error.to_string()))?;
                    prop_assert_eq!(&read, &score);
                    if round == 0 {
                        accepted.set(accepted.get() + 1);
                    }
                    return Ok(());
                }
                Err(GpError::Unsupported { path, reason }) => {
                    let kind = kind_of(&reason);
                    prop_assert!(
                        GENUINE_LIMITS.contains(&kind.as_str()),
                        "unexpected refusal {reason:?} at {path}"
                    );
                    *observed.borrow_mut().entry(kind.clone()).or_default() += 1;
                    prop_assert!(
                        clean_kind(&mut score, &kind),
                        "refused for {kind} at {path} but nothing of that kind is present"
                    );
                }
                Err(other) => return Err(TestCaseError::fail(other.to_string())),
            }
        }
        Err(TestCaseError::fail(
            "still refused after removing every genuine limit",
        ))
    });
    if let Err(error) = outcome {
        panic!("{error}");
    }
    let observed = observed.into_inner();
    println!(
        "acceptance default (stripped): {}/{} refusals {observed:?}",
        accepted.get(),
        seen.get()
    );
    let kinds: Vec<&str> = observed.keys().map(String::as_str).collect();
    assert_eq!(kinds, GENUINE_LIMITS.to_vec());
}

const PARENTS: [&str; 14] = [
    "",
    "Properties",
    "Properties/Property@Tuning",
    "Sounds/Sound#0/MIDI",
    "Sounds/Sound#2",
    "*Rhythm",
    "*Rhythm/PrimaryTuplet",
    "*Tempo#0",
    "*Tempo#7/Value",
    "*Other",
    "Bars",
    "a/",
    "/",
    "XProperties/XProperty@687935490",
];
const NODES: [&str; 9] = [
    "<A/>",
    "<Rhythm ref=\"9\"/>",
    "<Bar id=\"4\"><Voices>0</Voices></Bar>",
    "<Property name=\"Tuning\"><Pitches>x</Pitches></Property>",
    "<Property name=\"CapoFret\"><Fret>3</Fret></Property>",
    "<![CDATA[]]>",
    "text &amp; more",
    "<!--c-->",
    "<A/><B/>",
];
const GUARDS: [&str; 8] = [
    "",
    " guard=\"clef:G2\"",
    " guard=\"dynamic:MF\"",
    " guard=\"pitch:percussion 0\"",
    " guard=\"tempo-position:0/1;time 4/4\"",
    " guard=\"flag:Slapped false\"",
    " guard=\"flag:Nope true\"",
    " guard=\"nonsense\"",
];

fn arb_payload() -> impl Strategy<Value = OpaquePayload> {
    (
        proptest::sample::select(PARENTS.to_vec()),
        0usize..6,
        proptest::sample::select(GUARDS.to_vec()),
        proptest::sample::select(NODES.to_vec()),
        any::<bool>(),
    )
        .prop_map(|(parent, index, guard, node, attribute)| {
            let data = if attribute {
                format!("<KeepAttribute parent=\"{parent}\" index=\"{index}\"{guard} name=\"x\" value=\"{node}\"/>")
            } else {
                format!("<Keep parent=\"{parent}\" index=\"{index}\"{guard}>{node}</Keep>")
            };
            OpaquePayload {
                format: "gpif".to_string(),
                data,
            }
        })
}

#[test]
fn arbitrary_payloads_never_panic_the_writer() {
    let base = crate::support::Doc {
        master_track: "<Automations><Automation><Type>Tempo</Type><Bar>0</Bar><Value>90 2</Value></Automation></Automations>".to_string(),
        ..crate::support::Doc::default()
    }
    .read();
    let mut owners: Vec<opentab_model::ElementId> = vec![base.master_bars[0].id.0];
    let track = &base.tracks[0];
    owners.push(track.id.0);
    owners.push(track.staves[0].id.0);
    let bar = &track.staves[0].bars[0];
    owners.push(bar.id.0);
    owners.push(bar.voices[0].id.0);
    owners.push(bar.voices[0].beats[0].id.0);
    owners.push(bar.voices[0].beats[0].notes[0].id.0);
    let strategy = (
        proptest::collection::vec((0usize..8, arb_payload()), 0..6),
        any::<u64>(),
    );
    let mut runner = TestRunner::new(config());
    let outcome = runner.run(&strategy, |(payloads, _)| {
        let mut score = base.clone();
        for (owner, payload) in payloads {
            match owners.get(owner) {
                Some(id) => score.extensions.0.entry(*id).or_default().push(payload),
                None => score.document_extensions.push(payload),
            }
        }
        match write_gp(&score) {
            Ok(bytes) => {
                let _ = read_gp(&bytes);
            }
            Err(GpError::Unsupported { reason, .. }) => prop_assert!(documented(&reason)),
            Err(other) => prop_assert!(false, "unexpected error {other}"),
        }
        Ok(())
    });
    if let Err(error) = outcome {
        panic!("{error}");
    }
}
