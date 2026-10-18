use opentab_command::{Command, CommandError, MetaField, apply};
use opentab_model::{BeatId, ElementId, NoteId, NoteValue, Score, VoiceId, diagnose};

use crate::checks::{rejected, round_trip};
use crate::support::{duration, fixture, four_four};

#[test]
fn a_note_id_used_as_a_beat_is_not_found() {
    let mut fx = fixture();
    let raw = ElementId::from(fx.notes[0]);
    let beat = BeatId(raw);
    let commands = [
        Command::RemoveBeat { beat },
        Command::SetBeatDuration {
            beat,
            duration: duration(NoteValue::Half),
        },
        Command::AddNote {
            beat,
            string: 3,
            fret: 1,
        },
    ];
    for command in &commands {
        assert_eq!(
            rejected(&mut fx.score, command),
            CommandError::NotFound {
                what: "beat",
                id: raw.0
            }
        );
    }
}

#[test]
fn ids_of_other_kinds_are_not_found() {
    let mut fx = fixture();
    let beat_raw = ElementId::from(fx.beats[0]);
    let voice_raw = ElementId::from(fx.voice);
    let track_raw = ElementId::from(fx.guitar);
    let cases = [
        (
            Command::InsertBeat {
                voice: Some(VoiceId(beat_raw)),
                bar: None,
                index: 0,
                duration: duration(NoteValue::Quarter),
            },
            CommandError::NotFound {
                what: "voice",
                id: beat_raw.0,
            },
        ),
        (
            Command::RemoveNote {
                note: NoteId(beat_raw),
            },
            CommandError::NotFound {
                what: "note",
                id: beat_raw.0,
            },
        ),
        (
            Command::SetFret {
                note: NoteId(voice_raw),
                fret: 2,
            },
            CommandError::NotFound {
                what: "note",
                id: voice_raw.0,
            },
        ),
        (
            Command::RemoveBeat {
                beat: BeatId(track_raw),
            },
            CommandError::NotFound {
                what: "beat",
                id: track_raw.0,
            },
        ),
    ];
    for (command, expected) in &cases {
        assert_eq!(&rejected(&mut fx.score, command), expected);
    }
}

fn every_forward_command() -> Vec<Command> {
    let id = ElementId(1);
    vec![
        Command::RemoveMasterBar { index: 0 },
        Command::InsertBeat {
            voice: Some(VoiceId(id)),
            bar: None,
            index: 0,
            duration: duration(NoteValue::Quarter),
        },
        Command::RemoveBeat { beat: BeatId(id) },
        Command::SetBeatDuration {
            beat: BeatId(id),
            duration: duration(NoteValue::Quarter),
        },
        Command::AddNote {
            beat: BeatId(id),
            string: 0,
            fret: 0,
        },
        Command::RemoveNote { note: NoteId(id) },
        Command::SetFret {
            note: NoteId(id),
            fret: 0,
        },
        Command::SetTempo {
            master_bar: 0,
            bpm: 120.0,
        },
        Command::RestoreTempoChanges {
            master_bar: 0,
            tempo_changes: Vec::new(),
        },
        Command::RestoreNote {
            beat: BeatId(id),
            index: 0,
            note: fixture().score.tracks[0].staves[0].bars[0].voices[0].beats[0].notes[0].clone(),
            extensions: Vec::new(),
        },
    ]
}

#[test]
fn an_empty_score_rejects_commands_without_panicking() {
    let mut score = Score::new();
    for command in every_forward_command() {
        rejected(&mut score, &command);
    }
}

#[test]
fn an_empty_score_accepts_metadata_and_a_first_master_bar() {
    let mut score = Score::new();
    round_trip(
        &mut score,
        &Command::SetMeta {
            field: MetaField::Title,
            value: "é".repeat(5000),
        },
    );
    let command = Command::InsertMasterBar {
        index: 0,
        time_signature: four_four(),
    };
    round_trip(&mut score, &command);
    apply(&mut score, &command).unwrap();
    assert_eq!(score.master_bars.len(), 1);
    assert!(score.tracks.is_empty());
    assert_eq!(diagnose(&score), Vec::new());
}

#[test]
fn a_staff_with_extra_bars_blocks_master_bar_edits_only() {
    let mut fx = fixture();
    let extra = fx.score.tracks[0].staves[0].bars[0].clone();
    fx.score.tracks[0].staves[0].bars.push(extra);
    rejected(&mut fx.score, &Command::RemoveMasterBar { index: 1 });
    rejected(
        &mut fx.score,
        &Command::InsertMasterBar {
            index: 0,
            time_signature: four_four(),
        },
    );
    round_trip(
        &mut fx.score,
        &Command::SetFret {
            note: fx.notes[0],
            fret: 1,
        },
    );
}
