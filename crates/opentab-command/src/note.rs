use std::iter::once;

use opentab_model::{BeatId, InstrumentKind, Note, NoteEffects, NoteId, NoteKind, Score};

use crate::beat::find_beat;
use crate::ids::{check_restorable, put_extensions, reserve, take_extensions};
use crate::locate::{beat_in, beat_ref, not_found, note_in};
use crate::{Command, CommandError, ExtensionEntry};

const BEAT: &str = "beat";
const NOTE: &str = "note";

pub(crate) fn add(
    score: &mut Score,
    beat: BeatId,
    string: u8,
    fret: u8,
) -> Result<Command, CommandError> {
    let path = find_beat(score, beat)?;
    let track = score.tracks.get(path.track).ok_or(not_found(BEAT, beat))?;
    if track.instrument.kind != InstrumentKind::Fretted {
        return Err(CommandError::WrongInstrument { track: track.id });
    }
    let strings = track.tuning.strings.len();
    if usize::from(string) >= strings {
        return Err(CommandError::invalid(format!(
            "string {string} is out of range for track {} with {strings} strings",
            track.id.0.0
        )));
    }
    let target = beat_ref(&score.tracks, path).ok_or(not_found(BEAT, beat))?;
    let occupied = target
        .notes
        .iter()
        .any(|note| matches!(note.kind, NoteKind::Fretted { string: used, .. } if used == string));
    if occupied {
        return Err(CommandError::invalid(format!(
            "beat {} already has a note on string {string}",
            beat.0.0
        )));
    }
    reserve(score, 1)?;
    let Score { tracks, ids, .. } = score;
    let target = beat_in(tracks, path).ok_or(not_found(BEAT, beat))?;
    let before = ids.clone();
    let id = NoteId(ids.next());
    target.notes.push(Note {
        id,
        kind: NoteKind::Fretted { string, fret },
        effects: NoteEffects::default(),
    });
    Ok(Command::RestoreIds {
        ids: before,
        then: Box::new(Command::RemoveNote { note: id }),
    })
}

pub(crate) fn remove(score: &mut Score, note: NoteId) -> Result<Command, CommandError> {
    let path = score.find_note(note).ok_or(not_found(NOTE, note))?;
    let target = beat_in(&mut score.tracks, path.beat).ok_or(not_found(NOTE, note))?;
    if path.note >= target.notes.len() {
        return Err(not_found(NOTE, note));
    }
    let beat = target.id;
    let removed = target.notes.remove(path.note);
    let extensions = take_extensions(&mut score.extensions, once(removed.id.0));
    Ok(Command::RestoreNote {
        beat,
        index: path.note,
        note: removed,
        extensions,
    })
}

pub(crate) fn restore(
    score: &mut Score,
    beat: BeatId,
    index: usize,
    note: &Note,
    extensions: &[ExtensionEntry],
) -> Result<Command, CommandError> {
    let path = find_beat(score, beat)?;
    let len = beat_ref(&score.tracks, path)
        .ok_or(not_found(BEAT, beat))?
        .notes
        .len();
    if index > len {
        return Err(CommandError::IndexOutOfRange {
            what: NOTE,
            index,
            len,
        });
    }
    check_restorable(score, once(note.id.0), extensions)?;
    let target = beat_in(&mut score.tracks, path).ok_or(not_found(BEAT, beat))?;
    target.notes.insert(index, note.clone());
    put_extensions(&mut score.extensions, extensions);
    Ok(Command::RemoveNote { note: note.id })
}

pub(crate) fn set_fret(score: &mut Score, note: NoteId, fret: u8) -> Result<Command, CommandError> {
    let path = score.find_note(note).ok_or(not_found(NOTE, note))?;
    let track = score
        .tracks
        .get(path.beat.track)
        .map(|track| track.id)
        .ok_or(not_found(NOTE, note))?;
    let target = note_in(&mut score.tracks, path).ok_or(not_found(NOTE, note))?;
    match &mut target.kind {
        NoteKind::Fretted { fret: current, .. } => {
            let old = std::mem::replace(current, fret);
            Ok(Command::SetFret { note, fret: old })
        }
        NoteKind::Percussion { .. } => Err(CommandError::WrongInstrument { track }),
    }
}
