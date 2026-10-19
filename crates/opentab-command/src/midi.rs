use std::collections::BTreeMap;

use opentab_model::{
    Automation, AutomationValue, Fraction, MasterBarId, Score, SoundOverride, Track, TrackId,
    TrackMidi,
};

use crate::rules::already_broken;
use crate::{AutomationEntry, Command, CommandError};

const AUTOMATION: &str = "automation";

fn track_index(score: &Score, track: TrackId) -> Result<usize, CommandError> {
    score
        .tracks
        .iter()
        .position(|candidate| candidate.id == track)
        .ok_or(CommandError::NotFound {
            what: "track",
            id: track.0.0,
        })
}

fn track_mut(score: &mut Score, track: TrackId) -> Result<&mut Track, CommandError> {
    let index = track_index(score, track)?;
    score.tracks.get_mut(index).ok_or(CommandError::NotFound {
        what: "track",
        id: track.0.0,
    })
}

pub(crate) fn set_track_midi(
    score: &mut Score,
    track: TrackId,
    midi: TrackMidi,
) -> Result<Command, CommandError> {
    let target = track_mut(score, track)?;
    if let Some(first) = target.sounds.first()
        && first.bank != target.midi.bank
    {
        return Err(already_broken(CommandError::invalid(format!(
            "track {} has a first sound whose bank differs from the track's bank",
            track.0.0
        ))));
    }
    let old = std::mem::replace(&mut target.midi, midi);
    if let Some(first) = target.sounds.first_mut() {
        first.bank = midi.bank;
    }
    Ok(Command::SetTrackMidi { track, midi: old })
}

pub(crate) fn check_automation(
    score: &Score,
    track: &Track,
    automation: &Automation,
) -> Result<(), CommandError> {
    let master_bar = score
        .find_master_bar_index(automation.master_bar)
        .and_then(|index| score.master_bars.get(index))
        .ok_or(CommandError::NotFound {
            what: "master bar",
            id: automation.master_bar.0.0,
        })?;
    let position = automation.position;
    let end = master_bar.time_signature.bar_length();
    if *position.denom() <= 0 || position < Fraction::from_integer(0) || position > end {
        return Err(CommandError::invalid(format!(
            "automation position {}/{} is outside the bar of length {}/{}",
            position.numer(),
            position.denom(),
            end.numer(),
            end.denom()
        )));
    }
    if let AutomationValue::Sound(sound) = automation.value
        && track.sounds.get(sound.0).is_none()
    {
        return Err(CommandError::invalid(format!(
            "track {} has no sound {}",
            track.id.0.0, sound.0
        )));
    }
    Ok(())
}

pub(crate) fn insert_automation(
    score: &mut Score,
    track: TrackId,
    index: usize,
    automation: &Automation,
) -> Result<Command, CommandError> {
    let position = track_index(score, track)?;
    let target = score.tracks.get(position).ok_or(CommandError::NotFound {
        what: "track",
        id: track.0.0,
    })?;
    let len = target.automations.len();
    if index > len {
        return Err(CommandError::IndexOutOfRange {
            what: AUTOMATION,
            index,
            len,
        });
    }
    check_automation(score, target, automation)?;
    track_mut(score, track)?
        .automations
        .insert(index, automation.clone());
    Ok(Command::RemoveAutomation { track, index })
}

pub(crate) fn remove_automation(
    score: &mut Score,
    track: TrackId,
    index: usize,
) -> Result<Command, CommandError> {
    let target = track_mut(score, track)?;
    let len = target.automations.len();
    if index >= len {
        return Err(CommandError::IndexOutOfRange {
            what: AUTOMATION,
            index,
            len,
        });
    }
    let automation = target.automations.remove(index);
    Ok(Command::InsertAutomation {
        track,
        index,
        automation,
    })
}

pub(crate) fn set_sound_override(
    score: &mut Score,
    track: TrackId,
    bank: Option<&str>,
) -> Result<Command, CommandError> {
    let old = score.sound_overrides.clone();
    match bank {
        Some(bank) => {
            track_index(score, track)?;
            match score
                .sound_overrides
                .iter_mut()
                .find(|entry| entry.track == track)
            {
                Some(entry) => entry.bank = bank.to_owned(),
                None => score.sound_overrides.push(SoundOverride {
                    track,
                    bank: bank.to_owned(),
                }),
            }
        }
        None => score.sound_overrides.retain(|entry| entry.track != track),
    }
    Ok(Command::RestoreSoundOverrides {
        sound_overrides: old,
    })
}

pub(crate) fn restore_sound_overrides(score: &mut Score, overrides: &[SoundOverride]) -> Command {
    let old = std::mem::replace(&mut score.sound_overrides, overrides.to_vec());
    Command::RestoreSoundOverrides {
        sound_overrides: old,
    }
}

pub(crate) fn take_automations(score: &mut Score, master_bar: MasterBarId) -> Vec<AutomationEntry> {
    let mut taken = Vec::new();
    for track in &mut score.tracks {
        let id = track.id;
        let mut index = 0usize;
        track.automations.retain(|automation| {
            let keep = automation.master_bar != master_bar;
            if !keep {
                taken.push(AutomationEntry {
                    track: id,
                    index,
                    automation: automation.clone(),
                });
            }
            index += 1;
            keep
        });
    }
    taken
}

pub(crate) fn check_restorable_automations(
    score: &Score,
    master_bar: MasterBarId,
    entries: &[AutomationEntry],
) -> Result<(), CommandError> {
    let mut lengths: BTreeMap<TrackId, usize> = BTreeMap::new();
    for entry in entries {
        if entry.automation.master_bar != master_bar {
            return Err(CommandError::invalid(format!(
                "a restored automation names master bar {}, not the restored master bar {}",
                entry.automation.master_bar.0.0, master_bar.0.0
            )));
        }
        let position = track_index(score, entry.track)?;
        let start = score
            .tracks
            .get(position)
            .map_or(0, |track| track.automations.len());
        let len = lengths.entry(entry.track).or_insert(start);
        if entry.index > *len {
            return Err(CommandError::IndexOutOfRange {
                what: AUTOMATION,
                index: entry.index,
                len: *len,
            });
        }
        *len += 1;
    }
    Ok(())
}

pub(crate) fn put_automations(score: &mut Score, entries: &[AutomationEntry]) {
    for entry in entries {
        if let Some(track) = score
            .tracks
            .iter_mut()
            .find(|track| track.id == entry.track)
        {
            let at = entry.index.min(track.automations.len());
            track.automations.insert(at, entry.automation.clone());
        }
    }
}
