use std::iter::once;

use opentab_model::{BeatId, Note, NoteEffects, NoteId, NoteKind, Score};

use crate::beat::find_beat;
use crate::ids::{check_restorable, next_id, put_extensions, reserve, take_extensions};
use crate::locate::{beat_in, beat_ref, not_found, note_in};
use crate::rules::check_note_in;
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
    let target = beat_ref(&score.tracks, path).ok_or(not_found(BEAT, beat))?;
    check_note_in(
        track,
        beat,
        &target.notes,
        NoteKind::Fretted { string, fret },
    )?;
    reserve(score, 1)?;
    let Score { tracks, ids, .. } = score;
    let target = beat_in(tracks, path).ok_or(not_found(BEAT, beat))?;
    let id: NoteId = next_id(ids)?;
    target.notes.push(Note {
        id,
        kind: NoteKind::Fretted { string, fret },
        effects: NoteEffects::default(),
    });
    Ok(Command::RemoveNote { note: id })
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
    let track = score.tracks.get(path.track).ok_or(not_found(BEAT, beat))?;
    let target = beat_ref(&score.tracks, path).ok_or(not_found(BEAT, beat))?;
    let len = target.notes.len();
    if index > len {
        return Err(CommandError::IndexOutOfRange {
            what: NOTE,
            index,
            len,
        });
    }
    check_note_in(track, beat, &target.notes, note.kind)?;
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
