use opentab_command::{Command, CommandError, apply};
use opentab_model::{
    Fraction, Note, NoteEffects, NoteId, NoteKind, Score, Tempo, TempoChange, TempoUnit,
};

use crate::checks::{rejected, round_trip};
use crate::support::{Fixture, fixture};

const OUT_OF_RANGE: &str = "out of range";
const OCCUPIED: &str = "already has a note on string";
const PERCUSSION: &str = "percussion note";
const NO_VOICES: &str = "no voices";
const OUTSIDE: &str = "outside the bar";

fn invalid(error: CommandError, expected: &str) {
    let CommandError::InvalidValue { message } = error else {
        panic!("expected invalid value containing {expected:?}, got {error:?}");
    };
    assert!(message.contains(expected), "{message}");
}

fn wrong_instrument(error: CommandError, fx: &Fixture) {
    assert_eq!(error, CommandError::WrongInstrument { track: fx.drums });
}

fn set_string(note: &mut Note, string: u8) {
    let NoteKind::Fretted { fret, .. } = note.kind else {
        panic!("fretted note expected");
    };
    note.kind = NoteKind::Fretted { string, fret };
}

fn spare_note(score: &mut Score, string: u8) -> Note {
    let id: NoteId = score.alloc().unwrap();
    Note {
        id,
        kind: NoteKind::Fretted { string, fret: 1 },
        effects: NoteEffects::default(),
    }
}

fn change(numer: i64, denom: i64) -> TempoChange {
    TempoChange {
        position: Fraction::new(numer, denom),
        bpm: Tempo::new(90.0).unwrap(),
        unit: TempoUnit::Quarter,
    }
}

fn removed(score: &mut Score, command: Command) -> Command {
    apply(score, &command).unwrap()
}

#[test]
fn restore_beat_rejects_a_string_beyond_the_tuning() {
    let mut fx = fixture();
    let mut restore = removed(&mut fx.score, Command::RemoveBeat { beat: fx.beats[3] });
    let Command::RestoreBeat { beat, .. } = &mut restore else {
        panic!("restore_beat expected");
    };
    set_string(&mut beat.notes[0], 6);
    invalid(rejected(&mut fx.score, &restore), OUT_OF_RANGE);
}

#[test]
fn restore_beat_rejects_duplicate_strings() {
    let mut fx = fixture();
    let mut restore = removed(&mut fx.score, Command::RemoveBeat { beat: fx.beats[0] });
    let Command::RestoreBeat { beat, .. } = &mut restore else {
        panic!("restore_beat expected");
    };
    set_string(&mut beat.notes[1], 0);
    invalid(rejected(&mut fx.score, &restore), OCCUPIED);
}

#[test]
fn restore_beat_of_fretted_notes_into_drums_is_the_wrong_instrument() {
    let mut fx = fixture();
    let drum_voice = fx.score.tracks[1].staves[0].bars[0].voices[0].id;
    let mut restore = removed(&mut fx.score, Command::RemoveBeat { beat: fx.beats[3] });
    let Command::RestoreBeat { voice, index, .. } = &mut restore else {
        panic!("restore_beat expected");
    };
    *voice = drum_voice;
    *index = 0;
    wrong_instrument(rejected(&mut fx.score, &restore), &fx);
}

#[test]
fn restore_beat_of_drum_notes_into_a_guitar_is_invalid() {
    let mut fx = fixture();
    let mut restore = removed(&mut fx.score, Command::RemoveBeat { beat: fx.drum_beat });
    let Command::RestoreBeat { voice, index, .. } = &mut restore else {
        panic!("restore_beat expected");
    };
    *voice = fx.voice;
    *index = 0;
    invalid(rejected(&mut fx.score, &restore), PERCUSSION);
}

#[test]
fn restore_note_rejects_a_string_beyond_the_tuning() {
    let mut fx = fixture();
    let mut restore = removed(&mut fx.score, Command::RemoveNote { note: fx.notes[0] });
    let Command::RestoreNote { note, .. } = &mut restore else {
        panic!("restore_note expected");
    };
    set_string(note, 6);
    invalid(rejected(&mut fx.score, &restore), OUT_OF_RANGE);
}

#[test]
fn restore_note_rejects_an_occupied_string() {
    let mut fx = fixture();
    let mut restore = removed(&mut fx.score, Command::RemoveNote { note: fx.notes[0] });
    let Command::RestoreNote { note, .. } = &mut restore else {
        panic!("restore_note expected");
    };
    set_string(note, 2);
    invalid(rejected(&mut fx.score, &restore), OCCUPIED);
}

#[test]
fn restore_note_of_a_fretted_note_on_drums_is_the_wrong_instrument() {
    let mut fx = fixture();
    let mut restore = removed(&mut fx.score, Command::RemoveNote { note: fx.notes[0] });
    let Command::RestoreNote { beat, index, .. } = &mut restore else {
        panic!("restore_note expected");
    };
    *beat = fx.drum_beat;
    *index = 0;
    wrong_instrument(rejected(&mut fx.score, &restore), &fx);
}

#[test]
fn restore_note_of_a_drum_note_on_a_guitar_is_invalid() {
    let mut fx = fixture();
    let mut restore = removed(&mut fx.score, Command::RemoveNote { note: fx.drum_note });
    let Command::RestoreNote { beat, index, .. } = &mut restore else {
        panic!("restore_note expected");
    };
    *beat = fx.beats[1];
    *index = 0;
    invalid(rejected(&mut fx.score, &restore), PERCUSSION);
}

#[test]
fn a_drum_note_and_a_drum_beat_still_round_trip() {
    let mut fx = fixture();
    round_trip(&mut fx.score, &Command::RemoveNote { note: fx.drum_note });
    round_trip(&mut fx.score, &Command::RemoveBeat { beat: fx.drum_beat });
}

#[test]
fn restore_master_bar_rejects_a_bar_without_voices() {
    let mut fx = fixture();
    let mut restore = removed(&mut fx.score, Command::RemoveMasterBar { index: 1 });
    let Command::RestoreMasterBar {
        bars, extensions, ..
    } = &mut restore
    else {
        panic!("restore_master_bar expected");
    };
    bars[1].voices.clear();
    extensions.clear();
    invalid(rejected(&mut fx.score, &restore), NO_VOICES);
}

#[test]
fn restore_master_bar_rejects_bars_on_the_wrong_staves() {
    let mut fx = fixture();
    let mut restore = removed(&mut fx.score, Command::RemoveMasterBar { index: 1 });
    let Command::RestoreMasterBar { bars, .. } = &mut restore else {
        panic!("restore_master_bar expected");
    };
    bars.swap(0, 1);
    wrong_instrument(rejected(&mut fx.score, &restore), &fx);
}

#[test]
fn restore_master_bar_rejects_a_string_beyond_the_tuning() {
    let mut fx = fixture();
    let mut restore = removed(&mut fx.score, Command::RemoveMasterBar { index: 1 });
    let Command::RestoreMasterBar { bars, .. } = &mut restore else {
        panic!("restore_master_bar expected");
    };
    set_string(&mut bars[0].voices[0].beats[0].notes[0], 6);
    invalid(rejected(&mut fx.score, &restore), OUT_OF_RANGE);
}

#[test]
fn restore_master_bar_rejects_duplicate_strings() {
    let mut fx = fixture();
    let mut restore = removed(&mut fx.score, Command::RemoveMasterBar { index: 1 });
    let extra = spare_note(&mut fx.score, 5);
    let Command::RestoreMasterBar { bars, .. } = &mut restore else {
        panic!("restore_master_bar expected");
    };
    bars[0].voices[0].beats[0].notes.push(extra);
    invalid(rejected(&mut fx.score, &restore), OCCUPIED);
}

#[test]
fn restore_master_bar_accepts_a_spare_note_on_a_free_string() {
    let mut fx = fixture();
    let mut restore = removed(&mut fx.score, Command::RemoveMasterBar { index: 1 });
    let extra = spare_note(&mut fx.score, 4);
    let Command::RestoreMasterBar { bars, .. } = &mut restore else {
        panic!("restore_master_bar expected");
    };
    bars[0].voices[0].beats[0].notes.push(extra);
    apply(&mut fx.score, &restore).expect("a valid subtree restores");
}

#[test]
fn restore_master_bar_rejects_tempo_changes_outside_the_bar() {
    for position in [change(1, 1), change(5, 4)] {
        let mut fx = fixture();
        let mut restore = removed(&mut fx.score, Command::RemoveMasterBar { index: 1 });
        let Command::RestoreMasterBar { master_bar, .. } = &mut restore else {
            panic!("restore_master_bar expected");
        };
        master_bar.tempo_changes.push(position);
        invalid(rejected(&mut fx.score, &restore), OUTSIDE);
    }
}

#[test]
fn restore_master_bar_accepts_a_tempo_change_inside_the_bar() {
    let mut fx = fixture();
    let mut restore = removed(&mut fx.score, Command::RemoveMasterBar { index: 1 });
    let Command::RestoreMasterBar { master_bar, .. } = &mut restore else {
        panic!("restore_master_bar expected");
    };
    master_bar.tempo_changes.push(change(15, 16));
    apply(&mut fx.score, &restore).expect("an inside tempo change restores");
}

#[test]
fn restore_tempo_changes_rejects_positions_at_or_past_the_bar_end() {
    let mut fx = fixture();
    for position in [change(1, 1), change(9, 8)] {
        let command = Command::RestoreTempoChanges {
            master_bar: 0,
            tempo_changes: vec![change(0, 1), position],
        };
        invalid(rejected(&mut fx.score, &command), OUTSIDE);
    }
    let inside = Command::RestoreTempoChanges {
        master_bar: 0,
        tempo_changes: vec![change(0, 1), change(15, 16)],
    };
    round_trip(&mut fx.score, &inside);
}
