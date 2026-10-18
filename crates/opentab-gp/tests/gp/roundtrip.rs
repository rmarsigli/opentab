use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use opentab_gp::{GpError, Refusal, read_gp, write_gp};
use opentab_model::{NoteKind, OpaquePayload, Score};
use opentab_testkit::{ScoreGenConfig, arb_score, arb_score_with};
use proptest::prelude::*;
use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};

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

fn documented(reason: &Refusal) -> bool {
    matches!(
        reason,
        Refusal::Character { .. }
            | Refusal::ScoreEntries { .. }
            | Refusal::ScoreEntryName { .. }
            | Refusal::DuplicateEntry { .. }
            | Refusal::EntryName { .. }
            | Refusal::DirectoryData { .. }
            | Refusal::PayloadFormat { .. }
            | Refusal::MalformedPayload { .. }
            | Refusal::DanglingExtension { .. }
            | Refusal::BarCount { .. }
            | Refusal::VoiceSlots
            | Refusal::NoteKind
            | Refusal::String { .. }
            | Refusal::Pitch
            | Refusal::TempoPosition { .. }
            | Refusal::HarmonicFret { .. }
            | Refusal::RightHandOpen
            | Refusal::ChordWithoutDiagram { .. }
    )
}

fn round_trip(
    score: &Score,
    refusals: &RefCell<BTreeMap<String, usize>>,
) -> Result<bool, TestCaseError> {
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
            let kind = format!("{reason:?}");
            let kind = kind
                .split([' ', '{'])
                .next()
                .unwrap_or_default()
                .to_string();
            *refusals.borrow_mut().entry(kind).or_default() += 1;
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

    fn counts(&self) -> [(&'static str, usize); 7] {
        [
            ("notes", self.notes.get()),
            ("percussion notes", self.percussion.get()),
            ("payloads", self.payloads.get()),
            ("tempo changes", self.tempo_changes.get()),
            ("containers", self.containers.get()),
            ("second staves", self.second_staves.get()),
            ("markup titles", self.markup.get()),
        ]
    }
}

struct Outcome {
    accepted: usize,
    seen: usize,
    coverage: Coverage,
    refusals: BTreeMap<String, usize>,
}

fn run<S: Strategy<Value = Score>>(strategy: S) -> Outcome {
    let seen = Cell::new(0usize);
    let accepted = Cell::new(0usize);
    let coverage = Coverage::default();
    let refusals = RefCell::new(BTreeMap::new());
    let mut runner = TestRunner::new(config());
    let outcome = runner.run(&strategy, |score| {
        seen.set(seen.get() + 1);
        if round_trip(&score, &refusals)? {
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
        refusals: refusals.into_inner(),
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

#[test]
fn any_score_reads_back_equal_or_is_refused_for_a_documented_reason() {
    let outcome = run(arb_score());
    println!(
        "acceptance default: {}/{} {:?}",
        outcome.accepted, outcome.seen, outcome.refusals
    );
    assert_eq!(outcome.seen, CASES as usize);
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
    " guard=\"tempo-position:0/1\"",
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
