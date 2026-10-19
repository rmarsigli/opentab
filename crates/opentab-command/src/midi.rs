use std::collections::BTreeMap;

use opentab_model::{
    AUTOMATION_GRID, Automation, AutomationValue, DrumKit, Fraction, InstrumentKind, MasterBar,
    MasterBarId, NoteKind, Score, SoundOverride, Track, TrackId, TrackMidi, automations_in_order,
    on_automation_grid,
};

use crate::rules::{already_broken, check_articulation};
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

fn not_percussion(track: TrackId) -> CommandError {
    CommandError::invalid(format!(
        "track {} is not a percussion track; a drum kit belongs only to a percussion track",
        track.0.0
    ))
}

pub(crate) fn set_drum_kit(
    score: &mut Score,
    track: TrackId,
    drum_kit: Option<&DrumKit>,
) -> Result<Command, CommandError> {
    let target = track_mut(score, track)?;
    let percussion = target.instrument.kind == InstrumentKind::Percussion;
    if !percussion && target.drum_kit.is_some() {
        return Err(already_broken(not_percussion(track)));
    }
    if !percussion && drum_kit.is_some() {
        return Err(not_percussion(track));
    }
    let articulations: Vec<u16> = target
        .staves
        .iter()
        .flat_map(|staff| &staff.bars)
        .flat_map(|bar| &bar.voices)
        .flat_map(|voice| &voice.beats)
        .flat_map(|beat| &beat.notes)
        .filter_map(|note| match note.kind {
            NoteKind::Percussion { articulation } => Some(articulation),
            NoteKind::Fretted { .. } => None,
        })
        .collect();
    if percussion {
        for articulation in &articulations {
            check_articulation(target, *articulation).map_err(already_broken)?;
        }
    }
    let old = std::mem::replace(&mut target.drum_kit, drum_kit.cloned());
    if percussion {
        for articulation in &articulations {
            if let Err(error) = check_articulation(target, *articulation) {
                target.drum_kit = old;
                return Err(error);
            }
        }
    }
    Ok(Command::SetDrumKit {
        track,
        drum_kit: old,
    })
}

fn invalid_position(position: Fraction, end: Fraction) -> CommandError {
    CommandError::invalid(format!(
        "automation position {}/{} is outside the bar of length {}/{} or off the 1/{} grid",
        position.numer(),
        position.denom(),
        end.numer(),
        end.denom(),
        AUTOMATION_GRID
    ))
}

fn check_value(track: &Track, automation: &Automation) -> Result<(), CommandError> {
    let AutomationValue::Sound(sound) = automation.value else {
        return Ok(());
    };
    let Some(named) = track.sounds.get(sound.0) else {
        return Err(CommandError::invalid(format!(
            "track {} has no sound {}",
            track.id.0.0, sound.0
        )));
    };
    let twins = track
        .sounds
        .iter()
        .filter(|other| other.name == named.name)
        .count();
    if twins > 1 {
        return Err(CommandError::invalid(format!(
            "sound {} of track {} shares its name with another sound",
            sound.0, track.id.0.0
        )));
    }
    Ok(())
}

pub(crate) fn check_automation_in(
    track: &Track,
    automation: &Automation,
    bar_length: Fraction,
) -> Result<(), CommandError> {
    let position = automation.position;
    if *position.denom() <= 0
        || position < Fraction::from_integer(0)
        || position > bar_length
        || !on_automation_grid(position)
    {
        return Err(invalid_position(position, bar_length));
    }
    check_value(track, automation)
}

fn bar_length(score: &Score, master_bar: MasterBarId) -> Result<Fraction, CommandError> {
    score
        .find_master_bar_index(master_bar)
        .and_then(|index| score.master_bars.get(index))
        .map(|found| found.time_signature.bar_length())
        .ok_or(CommandError::NotFound {
            what: "master bar",
            id: master_bar.0.0,
        })
}

fn check_order(track: &Track) -> Result<(), CommandError> {
    if automations_in_order(&track.automations) {
        return Ok(());
    }
    Err(already_broken(CommandError::invalid(format!(
        "track {} lists a sound automation before a volume or pan automation",
        track.id.0.0
    ))))
}

fn check_placement(
    automations: &[Automation],
    index: usize,
    automation: &Automation,
) -> Result<(), CommandError> {
    let mut placed = automations.to_vec();
    placed.insert(index.min(placed.len()), automation.clone());
    if automations_in_order(&placed) {
        return Ok(());
    }
    Err(CommandError::invalid(format!(
        "index {index} would put a volume or pan automation after a sound automation"
    )))
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
    check_order(target)?;
    check_automation_in(
        target,
        automation,
        bar_length(score, automation.master_bar)?,
    )?;
    check_placement(&target.automations, index, automation)?;
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
    check_order(target)?;
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

fn check_overrides(score: &Score, overrides: &[SoundOverride]) -> Result<(), CommandError> {
    for (index, entry) in overrides.iter().enumerate() {
        track_index(score, entry.track)?;
        if overrides[..index]
            .iter()
            .any(|other| other.track == entry.track)
        {
            return Err(CommandError::invalid(format!(
                "track {} has more than one sound override",
                entry.track.0.0
            )));
        }
    }
    Ok(())
}

pub(crate) fn set_sound_override(
    score: &mut Score,
    track: TrackId,
    bank: Option<&str>,
) -> Result<Command, CommandError> {
    check_overrides(score, &score.sound_overrides).map_err(already_broken)?;
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

pub(crate) fn restore_sound_overrides(
    score: &mut Score,
    overrides: &[SoundOverride],
) -> Result<Command, CommandError> {
    check_overrides(score, overrides)?;
    let old = std::mem::replace(&mut score.sound_overrides, overrides.to_vec());
    Ok(Command::RestoreSoundOverrides {
        sound_overrides: old,
    })
}

pub(crate) fn check_removable_automations(
    score: &Score,
    master_bar: MasterBarId,
) -> Result<(), CommandError> {
    let length = bar_length(score, master_bar)?;
    for track in &score.tracks {
        check_order(track)?;
        for automation in &track.automations {
            if automation.master_bar == master_bar {
                check_automation_in(track, automation, length).map_err(already_broken)?;
            }
        }
    }
    Ok(())
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
    master_bar: &MasterBar,
    entries: &[AutomationEntry],
) -> Result<(), CommandError> {
    let mut placed: BTreeMap<TrackId, Vec<Automation>> = BTreeMap::new();
    let length = master_bar.time_signature.bar_length();
    for entry in entries {
        if entry.automation.master_bar != master_bar.id {
            return Err(CommandError::invalid(format!(
                "a restored automation names master bar {}, not the restored master bar {}",
                entry.automation.master_bar.0.0, master_bar.id.0.0
            )));
        }
        let position = track_index(score, entry.track)?;
        let track = score.tracks.get(position).ok_or(CommandError::NotFound {
            what: "track",
            id: entry.track.0.0,
        })?;
        check_order(track)?;
        check_automation_in(track, &entry.automation, length)?;
        let list = placed
            .entry(entry.track)
            .or_insert_with(|| track.automations.clone());
        if entry.index > list.len() {
            return Err(CommandError::IndexOutOfRange {
                what: AUTOMATION,
                index: entry.index,
                len: list.len(),
            });
        }
        check_placement(list, entry.index, &entry.automation)?;
        list.insert(entry.index, entry.automation.clone());
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
