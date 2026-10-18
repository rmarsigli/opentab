use opentab_model::{BarId, Score, Voice, VoiceId};

use crate::ids::{check_removable, check_restorable, put_extensions, take_extensions, voice_ids};
use crate::locate::{bar_in, bar_ref, find_bar, not_found};
use crate::rules::{already_broken, check_beat};
use crate::{Command, CommandError, ExtensionEntry};

const VOICE: &str = "voice";
const BAR: &str = "bar";

pub(crate) struct Restored<'a> {
    pub(crate) bar: BarId,
    pub(crate) index: usize,
    pub(crate) voice: &'a Voice,
    pub(crate) extensions: &'a [ExtensionEntry],
}

pub(crate) fn remove(score: &mut Score, voice: VoiceId) -> Result<Command, CommandError> {
    let path = score.find_voice(voice).ok_or(not_found(VOICE, voice))?;
    let track = score
        .tracks
        .get(path.track)
        .ok_or(not_found(VOICE, voice))?;
    let bar = track
        .staves
        .get(path.staff)
        .and_then(|staff| staff.bars.get(path.bar))
        .ok_or(not_found(VOICE, voice))?;
    let current = bar.voices.get(path.voice).ok_or(not_found(VOICE, voice))?;
    let unique = find_bar(score, bar.id).is_some_and(|found| {
        found.track == path.track && found.staff == path.staff && found.bar == path.bar
    });
    if !unique {
        return Err(CommandError::invalid(format!(
            "bar id {} is not unique, so the removal could not be undone; run diagnose",
            bar.id.0.0
        )));
    }
    for beat in &current.beats {
        check_beat(track, beat).map_err(already_broken)?;
    }
    check_removable(score, voice_ids(current))?;
    let bar_id = bar.id;
    let target = score
        .tracks
        .get_mut(path.track)
        .and_then(|track| track.staves.get_mut(path.staff))
        .and_then(|staff| staff.bars.get_mut(path.bar))
        .ok_or(not_found(VOICE, voice))?;
    if path.voice >= target.voices.len() {
        return Err(not_found(VOICE, voice));
    }
    let removed = target.voices.remove(path.voice);
    let extensions = take_extensions(&mut score.extensions, voice_ids(&removed));
    Ok(Command::RestoreVoice {
        bar: bar_id,
        index: path.voice,
        voice: removed,
        extensions,
    })
}

pub(crate) fn restore(score: &mut Score, restored: Restored<'_>) -> Result<Command, CommandError> {
    let Restored {
        bar,
        index,
        voice,
        extensions,
    } = restored;
    let path = find_bar(score, bar).ok_or(not_found(BAR, bar))?;
    let current = bar_ref(&score.tracks, path).ok_or(not_found(BAR, bar))?;
    let len = current.voices.len();
    if index > len {
        return Err(CommandError::IndexOutOfRange {
            what: VOICE,
            index,
            len,
        });
    }
    let after_previous = index
        .checked_sub(1)
        .and_then(|previous| current.voices.get(previous))
        .is_none_or(|previous| previous.slot < voice.slot);
    let before_next = current
        .voices
        .get(index)
        .is_none_or(|next| voice.slot < next.slot);
    if !after_previous || !before_next {
        return Err(CommandError::invalid(format!(
            "voice slot {} does not fit at index {index} of bar {}; slots must stay strictly increasing",
            voice.slot.get(),
            bar.0.0
        )));
    }
    let track = score.tracks.get(path.track).ok_or(not_found(BAR, bar))?;
    for beat in &voice.beats {
        check_beat(track, beat)?;
    }
    check_restorable(score, voice_ids(voice), extensions)?;
    let target = bar_in(&mut score.tracks, path).ok_or(not_found(BAR, bar))?;
    target.voices.insert(index, voice.clone());
    put_extensions(&mut score.extensions, extensions);
    Ok(Command::RemoveVoice { voice: voice.id })
}
