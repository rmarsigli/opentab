use opentab_model::Score;

use crate::{Command, CommandError, beat, master_bar, meta, note, staff, tempo};

pub fn apply(score: &mut Score, command: &Command) -> Result<Command, CommandError> {
    match command {
        Command::SetMeta { field, value } => Ok(meta::set(score, *field, value)),
        Command::InsertMasterBar {
            index,
            time_signature,
        } => master_bar::insert(score, *index, *time_signature),
        Command::RemoveMasterBar { index } => master_bar::remove(score, *index),
        Command::RestoreMasterBar {
            index,
            master_bar,
            bars,
            extensions,
        } => master_bar::restore(
            score,
            *index,
            master_bar::Snapshot {
                master_bar,
                bars,
                extensions,
            },
        ),
        Command::InsertBeat {
            voice,
            index,
            duration,
        } => beat::insert(score, *voice, *index, *duration),
        Command::RemoveBeat { beat } => beat::remove(score, *beat),
        Command::RestoreBeat {
            voice,
            index,
            beat,
            extensions,
        } => beat::restore(score, *voice, *index, beat, extensions),
        Command::SetBeatDuration { beat, duration } => beat::set_duration(score, *beat, *duration),
        Command::AddNote { beat, string, fret } => note::add(score, *beat, *string, *fret),
        Command::RemoveNote { note } => note::remove(score, *note),
        Command::RestoreNote {
            beat,
            index,
            note,
            extensions,
        } => note::restore(score, *beat, *index, note, extensions),
        Command::SetFret { note, fret } => note::set_fret(score, *note, *fret),
        Command::SetTempo { master_bar, bpm } => tempo::set(score, *master_bar, *bpm),
        Command::RestoreTempoChanges {
            master_bar,
            tempo_changes,
        } => tempo::restore(score, *master_bar, tempo_changes),
        Command::PadStaffBars { track, staff } => staff::pad(score, *track, *staff),
        Command::TrimStaffBars { track, staff, len } => staff::trim(score, *track, *staff, *len),
        Command::RestoreStaffBars {
            track,
            staff,
            bars,
            extensions,
        } => staff::restore(
            score,
            staff::Appended {
                track: *track,
                staff: *staff,
                bars,
                extensions,
            },
        ),
    }
}
