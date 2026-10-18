use opentab_model::{
    Bar, Beat, BeatId, Fraction, IdAllocator, InstrumentKind, Note, NoteKind, TempoChange,
    TimeSignature, Track, Voice,
};

use crate::CommandError;
use crate::ids::next_id;

pub(crate) fn check_note_in(
    track: &Track,
    beat: BeatId,
    notes: &[Note],
    kind: NoteKind,
) -> Result<(), CommandError> {
    match (track.instrument.kind, kind) {
        (InstrumentKind::Fretted, NoteKind::Fretted { string, .. }) => {
            let strings = track.tuning.strings.len();
            if usize::from(string) >= strings {
                return Err(CommandError::invalid(format!(
                    "string {string} is out of range for track {} with {strings} strings",
                    track.id.0.0
                )));
            }
            let occupied = notes.iter().any(
                |note| matches!(note.kind, NoteKind::Fretted { string: used, .. } if used == string),
            );
            if occupied {
                return Err(CommandError::invalid(format!(
                    "beat {} already has a note on string {string}",
                    beat.0.0
                )));
            }
            Ok(())
        }
        (InstrumentKind::Percussion, NoteKind::Percussion { .. }) => Ok(()),
        (InstrumentKind::Percussion, NoteKind::Fretted { .. }) => {
            Err(CommandError::WrongInstrument { track: track.id })
        }
        (InstrumentKind::Fretted, NoteKind::Percussion { .. }) => {
            Err(CommandError::invalid(format!(
                "a percussion note does not belong on fretted track {}",
                track.id.0.0
            )))
        }
    }
}

pub(crate) fn check_beat(track: &Track, beat: &Beat) -> Result<(), CommandError> {
    for (index, note) in beat.notes.iter().enumerate() {
        let before = beat.notes.get(..index).unwrap_or_default();
        check_note_in(track, beat.id, before, note.kind)?;
    }
    Ok(())
}

pub(crate) fn check_bar(track: &Track, bar: &Bar) -> Result<(), CommandError> {
    if bar.voices.is_empty() {
        return Err(CommandError::invalid(format!(
            "bar {} has no voices; every bar needs at least one",
            bar.id.0.0
        )));
    }
    for voice in &bar.voices {
        for beat in &voice.beats {
            check_beat(track, beat)?;
        }
    }
    Ok(())
}

pub(crate) fn check_empty_bar(bar: &Bar) -> Result<(), CommandError> {
    if bar.voices.iter().any(|voice| !voice.beats.is_empty()) {
        return Err(CommandError::invalid(format!(
            "bar {} holds beats; only empty bars can be trimmed or restored at the end of a staff",
            bar.id.0.0
        )));
    }
    Ok(())
}

pub(crate) fn empty_bar(ids: &mut IdAllocator) -> Result<Bar, CommandError> {
    Ok(Bar {
        id: next_id(ids)?,
        voices: vec![Voice {
            id: next_id(ids)?,
            beats: Vec::new(),
        }],
    })
}

pub(crate) fn check_tempo_changes(
    time_signature: TimeSignature,
    tempo_changes: &[TempoChange],
) -> Result<(), CommandError> {
    let start = Fraction::from_integer(0);
    let end = time_signature.bar_length();
    for change in tempo_changes {
        let position = change.position;
        if *position.denom() <= 0 || position < start || position >= end {
            return Err(CommandError::invalid(format!(
                "tempo change position {}/{} is outside the bar of length {}/{}",
                position.numer(),
                position.denom(),
                end.numer(),
                end.denom()
            )));
        }
    }
    Ok(())
}
