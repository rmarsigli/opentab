use opentab_model::Score;

use crate::{
    Command, CommandError, beat, directions, master_bar, meta, midi, note, staff, tempo, voice,
};

pub fn apply(score: &mut Score, command: &Command) -> Result<Command, CommandError> {
    match command {
        Command::SetMeta { field, value } => Ok(meta::set(score, *field, value)),
        Command::SetAnacrusis { value } => Ok(meta::set_anacrusis(score, *value)),
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
            automations,
        } => master_bar::restore(
            score,
            *index,
            master_bar::Snapshot {
                master_bar,
                bars,
                extensions,
                automations,
            },
        ),
        Command::SetTrackMidi { track, midi } => midi::set_track_midi(score, *track, *midi),
        Command::InsertAutomation {
            track,
            index,
            automation,
        } => midi::insert_automation(score, *track, *index, automation),
        Command::RemoveAutomation { track, index } => {
            midi::remove_automation(score, *track, *index)
        }
        Command::SetSoundOverride { track, bank } => {
            midi::set_sound_override(score, *track, bank.as_deref())
        }
        Command::RestoreSoundOverrides { sound_overrides } => {
            midi::restore_sound_overrides(score, sound_overrides)
        }
        Command::InsertBeat {
            voice,
            bar,
            index,
            duration,
        } => beat::insert(score, beat::target(*voice, *bar)?, *index, *duration),
        Command::RemoveBeat { beat } => beat::remove(score, *beat),
        Command::RemoveVoice { voice } => voice::remove(score, *voice),
        Command::RestoreVoice {
            bar,
            index,
            voice,
            extensions,
        } => voice::restore(
            score,
            voice::Restored {
                bar: *bar,
                index: *index,
                voice,
                extensions,
            },
        ),
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
        Command::SetDirections {
            master_bar,
            directions,
        } => directions::set(score, *master_bar, directions),
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
