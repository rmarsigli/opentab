use std::iter::once;

use opentab_model::{BeatId, Note, NoteEffects, NoteId, NoteKind, Score, Trill};

use crate::beat::find_beat;
use crate::ids::{
    check_removable, check_restorable, next_id, put_extensions, reserve, take_extensions,
};
use crate::locate::{beat_in, beat_ref, not_found, note_in};
use crate::rules::{already_broken, check_note_in, check_trill};
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
    let track = score
        .tracks
        .get(path.beat.track)
        .ok_or(not_found(NOTE, note))?;
    let owner = beat_ref(&score.tracks, path.beat).ok_or(not_found(NOTE, note))?;
    let current = owner.notes.get(path.note).ok_or(not_found(NOTE, note))?;
    let before = owner.notes.get(..path.note).unwrap_or_default();
    let after = owner
        .notes
        .get(path.note.saturating_add(1)..)
        .unwrap_or_default();
    if score.find_beat(owner.id) != Some(path.beat) {
        return Err(CommandError::invalid(format!(
            "beat id {} is not unique, so the removal could not be undone; run diagnose",
            owner.id.0.0
        )));
    }
    check_note_in(track, owner.id, before, current.kind).map_err(already_broken)?;
    check_note_in(track, owner.id, after, current.kind).map_err(already_broken)?;
    check_trill(track, current).map_err(already_broken)?;
    check_removable(score, once(current.id.0))?;
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
    check_trill(track, note)?;
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

pub(crate) fn set_trill(
    score: &mut Score,
    note: NoteId,
    trill: Option<Trill>,
) -> Result<Command, CommandError> {
    let path = score.find_note(note).ok_or(not_found(NOTE, note))?;
    let track = score
        .tracks
        .get(path.beat.track)
        .ok_or(not_found(NOTE, note))?;
    let owner = beat_ref(&score.tracks, path.beat).ok_or(not_found(NOTE, note))?;
    let current = owner.notes.get(path.note).ok_or(not_found(NOTE, note))?;
    check_trill(track, current).map_err(already_broken)?;
    let mut changed = current.clone();
    changed.effects.trill = trill;
    check_trill(track, &changed)?;
    let target = note_in(&mut score.tracks, path).ok_or(not_found(NOTE, note))?;
    let old = std::mem::replace(&mut target.effects.trill, trill);
    Ok(Command::SetTrill { note, trill: old })
}
