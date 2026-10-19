use std::collections::BTreeMap;

use opentab_command::{Command, CommandError, apply};
use opentab_model::{
    ElementId, Fraction, Note, NoteEffects, NoteId, NoteKind, Score, Tempo, TempoChange, TempoUnit,
    diagnose,
};
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::TestRunner;

use crate::checks::{rejected, round_trip};
use crate::generator::arb_case;
use crate::support::{Fixture, fixture};

fn invalid(error: CommandError, expected: &str) {
    let CommandError::InvalidValue { message } = error else {
        panic!("expected invalid value containing {expected:?}, got {error:?}");
    };
    assert!(message.contains(expected), "{message}");
}

fn tempo_outside(fx: &mut Fixture, master_bar: usize) {
    fx.score.master_bars[master_bar]
        .tempo_changes
        .push(TempoChange {
            position: Fraction::new(1, 1),
            bpm: Tempo::new(90.0).unwrap(),
            unit: TempoUnit::Quarter,
            linear: false,
        });
    assert!(!diagnose(&fx.score).is_empty());
}

#[test]
fn remove_master_bar_with_a_tempo_change_outside_the_bar_is_rejected() {
    let mut fx = fixture();
    tempo_outside(&mut fx, 1);
    let error = rejected(&mut fx.score, &Command::RemoveMasterBar { index: 1 });
    invalid(error, "outside the bar");
    round_trip(&mut fx.score, &Command::RemoveMasterBar { index: 0 });
}

#[test]
fn set_tempo_on_a_bar_with_a_tempo_change_outside_it_is_rejected() {
    let mut fx = fixture();
    tempo_outside(&mut fx, 0);
    let command = Command::SetTempo {
        master_bar: 0,
        bpm: 100.0,
    };
    invalid(rejected(&mut fx.score, &command), "outside the bar");
    let restore = Command::RestoreTempoChanges {
        master_bar: 0,
        tempo_changes: Vec::new(),
    };
    invalid(rejected(&mut fx.score, &restore), "outside the bar");
    round_trip(
        &mut fx.score,
        &Command::SetTempo {
            master_bar: 1,
            bpm: 100.0,
        },
    );
}

#[test]
fn remove_master_bar_accepts_a_bar_without_voices() {
    let mut fx = fixture();
    fx.score.tracks[1].staves[0].bars[1].voices.clear();
    round_trip(&mut fx.score, &Command::RemoveMasterBar { index: 1 });
}

#[test]
fn remove_beat_with_duplicate_strings_is_rejected() {
    let mut fx = fixture();
    let id: NoteId = fx.score.alloc().unwrap();
    let beat = &mut fx.score.tracks[0].staves[0].bars[0].voices[0].beats[3];
    let mut copy = beat.notes[0].clone();
    copy.id = id;
    beat.notes.push(copy);
    let command = Command::RemoveBeat { beat: fx.beats[3] };
    invalid(
        rejected(&mut fx.score, &command),
        "already has a note on string",
    );
    invalid(
        rejected(&mut fx.score, &Command::RemoveNote { note: id }),
        "already has a note on string",
    );
    invalid(
        rejected(&mut fx.score, &Command::RemoveMasterBar { index: 0 }),
        "already has a note on string",
    );
}

#[test]
fn removing_a_note_of_the_wrong_kind_or_string_is_rejected() {
    let mut fx = fixture();
    let id: NoteId = fx.score.alloc().unwrap();
    fx.score.tracks[0].staves[0].bars[0].voices[0].beats[1]
        .notes
        .push(Note {
            id,
            kind: NoteKind::Percussion { articulation: 38 },
            effects: NoteEffects::default(),
        });
    invalid(
        rejected(&mut fx.score, &Command::RemoveNote { note: id }),
        "percussion note",
    );
    invalid(
        rejected(&mut fx.score, &Command::RemoveBeat { beat: fx.beats[1] }),
        "percussion note",
    );
    let note = &mut fx.score.tracks[0].staves[0].bars[0].voices[0].beats[3].notes[0];
    note.kind = NoteKind::Fretted {
        string: 200,
        fret: 1,
    };
    invalid(
        rejected(&mut fx.score, &Command::RemoveBeat { beat: fx.beats[3] }),
        "out of range",
    );
}

#[test]
fn removing_a_subtree_with_a_shared_or_unallocated_id_is_rejected() {
    let mut fx = fixture();
    fx.score.tracks[0].staves[0].bars[0].voices[0].beats[1].id = fx.beats[2];
    invalid(
        rejected(&mut fx.score, &Command::RemoveBeat { beat: fx.beats[2] }),
        "appears more than once in the score",
    );
    let mut fx = fixture();
    let above = NoteId(ElementId(fx.score.ids.peek().0 + 3));
    fx.score.tracks[0].staves[0].bars[0].voices[0].beats[3].notes[0].id = above;
    invalid(
        rejected(&mut fx.score, &Command::RemoveNote { note: above }),
        "allocator",
    );
    invalid(
        rejected(&mut fx.score, &Command::RemoveMasterBar { index: 0 }),
        "allocator",
    );
}

const CASES: u32 = 256;
const COVERAGE_SAMPLES: usize = 512;
const MAX_COMMANDS_PER_CASE: usize = 48;
const RUN_DIAGNOSE: &str = "run diagnose";
const GUARDS: [&str; 8] = [
    RUN_DIAGNOSE,
    "is not unique",
    "outside the bar",
    "already has a note on string",
    "percussion note",
    "out of range",
    "appears more than once in the score",
    "allocator",
];

fn every_command(score: &Score) -> Vec<Command> {
    let mut commands = Vec::new();
    for index in 0..score.master_bars.len() {
        commands.push(Command::RemoveMasterBar { index });
        commands.push(Command::SetTempo {
            master_bar: index,
            bpm: 100.0,
        });
    }
    commands.push(Command::InsertMasterBar {
        index: 0,
        time_signature: crate::support::four_four(),
    });
    for track in &score.tracks {
        for staff in &track.staves {
            commands.push(Command::PadStaffBars {
                track: track.id,
                staff: staff.id,
            });
            for len in 0..staff.bars.len() {
                commands.push(Command::TrimStaffBars {
                    track: track.id,
                    staff: staff.id,
                    len,
                });
            }
            for bar in &staff.bars {
                commands.push(Command::InsertBeat {
                    voice: None,
                    bar: Some(bar.id),
                    index: 0,
                    duration: crate::support::duration(opentab_model::NoteValue::Eighth),
                });
                for voice in &bar.voices {
                    commands.push(Command::RemoveVoice { voice: voice.id });
                    commands.push(Command::InsertBeat {
                        voice: Some(voice.id),
                        bar: None,
                        index: 0,
                        duration: crate::support::duration(opentab_model::NoteValue::Eighth),
                    });
                    for beat in &voice.beats {
                        commands.push(Command::RemoveBeat { beat: beat.id });
                        commands.push(Command::AddNote {
                            beat: beat.id,
                            string: 0,
                            fret: 2,
                        });
                        for note in &beat.notes {
                            commands.push(Command::RemoveNote { note: note.id });
                        }
                    }
                }
            }
        }
    }
    commands
}

fn same_except_ids(left: &Score, right: &Score) -> bool {
    left.meta == right.meta
        && left.master_bars == right.master_bars
        && left.tracks == right.tracks
        && left.extensions == right.extensions
}

fn check_undoable(
    score: &Score,
    command: &Command,
    guards: &mut BTreeMap<&'static str, usize>,
) -> Result<(), TestCaseError> {
    let mut edited = score.clone();
    match apply(&mut edited, command) {
        Ok(inverse) => {
            let after = edited.clone();
            let forward = apply(&mut edited, &inverse);
            prop_assert!(
                forward.is_ok(),
                "{command:?} succeeded but its inverse failed: {forward:?}"
            );
            prop_assert!(
                same_except_ids(&edited, score),
                "undo of {command:?} differs"
            );
            let forward = forward.unwrap_or_else(|_| command.clone());
            prop_assert!(
                apply(&mut edited, &forward).is_ok(),
                "redo of {command:?} failed"
            );
            prop_assert_eq!(&edited, &after);
        }
        Err(error) => {
            prop_assert_eq!(&edited, score);
            if let CommandError::InvalidValue { message } = &error {
                prop_assert!(
                    !message.contains(RUN_DIAGNOSE) || !diagnose(score).is_empty(),
                    "{command:?} refused with {message:?} but diagnose reports nothing"
                );
                for guard in GUARDS {
                    if message.contains(guard) {
                        *guards.entry(guard).or_default() += 1;
                    }
                }
            }
        }
    }
    Ok(())
}

fn check_sampled_commands(
    score: &Score,
    offset: usize,
    cap: usize,
    guards: &mut BTreeMap<&'static str, usize>,
) -> Result<(), TestCaseError> {
    let commands = every_command(score);
    let stride = commands.len().div_ceil(cap.max(1)).max(1);
    let start = offset % stride;
    for command in commands.iter().skip(start).step_by(stride) {
        check_undoable(score, command, guards)?;
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn every_successful_command_on_any_case_is_undoable(
        case in arb_case(),
        offset in any::<usize>(),
    ) {
        let mut guards = BTreeMap::new();
        check_sampled_commands(&case.score, offset, MAX_COMMANDS_PER_CASE, &mut guards)?;
    }
}

#[test]
fn undo_guards_fire_on_corrupted_scores() {
    let mut runner = TestRunner::deterministic();
    let strategy = arb_case();
    let mut guards: BTreeMap<&'static str, usize> =
        GUARDS.iter().map(|guard| (*guard, 0)).collect();
    let mut corrupted = 0;
    for offset in 0..COVERAGE_SAMPLES {
        let case = strategy.new_tree(&mut runner).expect("case").current();
        if case.corruption.is_none() {
            continue;
        }
        corrupted += 1;
        check_sampled_commands(&case.score, offset, usize::MAX, &mut guards).expect("undoable");
    }
    assert!(corrupted > 0);
    for (guard, count) in &guards {
        assert!(*count > 0, "guard {guard:?} never fired: {guards:?}");
    }
}

#[test]
fn removals_whose_parent_id_is_shared_are_rejected() {
    let mut fx = fixture();
    let shared = fx.beats[0];
    fx.score.tracks[0].staves[0].bars[1].voices[0].beats[0].id = shared;
    let note = fx.score.tracks[0].staves[0].bars[1].voices[0].beats[0].notes[0].id;
    invalid(
        rejected(&mut fx.score, &Command::RemoveNote { note }),
        "is not unique",
    );
    let mut fx = fixture();
    let voice = fx.voice;
    fx.score.tracks[0].staves[0].bars[1].voices[0].id = voice;
    let beat = fx.score.tracks[0].staves[0].bars[1].voices[0].beats[0].id;
    invalid(
        rejected(&mut fx.score, &Command::RemoveBeat { beat }),
        "is not unique",
    );
}

#[test]
fn a_duplicate_string_refusal_points_to_a_diagnostic() {
    let mut fx = fixture();
    let id: NoteId = fx.score.alloc().unwrap();
    let beat = &mut fx.score.tracks[0].staves[0].bars[0].voices[0].beats[3];
    let mut copy = beat.notes[0].clone();
    copy.id = id;
    beat.notes.push(copy);
    let command = Command::RemoveBeat { beat: fx.beats[3] };
    invalid(rejected(&mut fx.score, &command), RUN_DIAGNOSE);
    assert_eq!(
        diagnose(&fx.score),
        vec![opentab_model::Diagnostic::DuplicateString {
            beat: fx.beats[3],
            string: 4
        }]
    );
}
