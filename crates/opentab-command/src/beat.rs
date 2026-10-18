use opentab_model::{
    BarId, Beat, BeatEffects, BeatId, BeatPath, Duration, Score, Voice, VoiceId, VoicePath,
    VoiceSlot,
};

use crate::ids::{
    beat_ids, check_removable, check_restorable, next_id, put_extensions, reserve, take_extensions,
};
use crate::locate::{
    bar_in, bar_ref, beat_in, beat_ref, find_bar, not_found, voice_in, voice_path, voice_ref,
};
use crate::rules::{already_broken, check_beat};
use crate::{Command, CommandError, ExtensionEntry};

const BEAT: &str = "beat";
const VOICE: &str = "voice";
const BAR: &str = "bar";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    Voice(VoiceId),
    Bar(BarId),
}

pub(crate) fn target(voice: Option<VoiceId>, bar: Option<BarId>) -> Result<Target, CommandError> {
    match (voice, bar) {
        (Some(voice), None) => Ok(Target::Voice(voice)),
        (None, Some(bar)) => Ok(Target::Bar(bar)),
        _ => Err(CommandError::invalid(
            "insert_beat needs exactly one target: a voice or a bar",
        )),
    }
}

fn new_beat(id: BeatId, duration: Duration) -> Beat {
    Beat {
        id,
        duration,
        notes: Vec::new(),
        effects: BeatEffects::default(),
    }
}

fn find_voice(score: &Score, voice: VoiceId) -> Result<(VoicePath, usize), CommandError> {
    let path = score.find_voice(voice).ok_or(not_found(VOICE, voice))?;
    let len = voice_ref(&score.tracks, path)
        .ok_or(not_found(VOICE, voice))?
        .beats
        .len();
    Ok((path, len))
}

pub(crate) fn find_beat(score: &Score, beat: BeatId) -> Result<BeatPath, CommandError> {
    score.find_beat(beat).ok_or(not_found(BEAT, beat))
}

fn check_insert_index(index: usize, len: usize) -> Result<(), CommandError> {
    if index > len {
        return Err(CommandError::IndexOutOfRange {
            what: BEAT,
            index,
            len,
        });
    }
    Ok(())
}

pub(crate) fn insert(
    score: &mut Score,
    target: Target,
    index: usize,
    duration: Duration,
) -> Result<Command, CommandError> {
    match target {
        Target::Voice(voice) => insert_into_voice(score, voice, index, duration),
        Target::Bar(bar) => insert_into_bar(score, bar, index, duration),
    }
}

fn insert_into_voice(
    score: &mut Score,
    voice: VoiceId,
    index: usize,
    duration: Duration,
) -> Result<Command, CommandError> {
    let (path, len) = find_voice(score, voice)?;
    check_insert_index(index, len)?;
    reserve(score, 1)?;
    let Score { tracks, ids, .. } = score;
    let target = voice_in(tracks, path).ok_or(not_found(VOICE, voice))?;
    let id: BeatId = next_id(ids)?;
    target.beats.insert(index, new_beat(id, duration));
    Ok(Command::RemoveBeat { beat: id })
}

fn insert_into_bar(
    score: &mut Score,
    bar: BarId,
    index: usize,
    duration: Duration,
) -> Result<Command, CommandError> {
    let path = find_bar(score, bar).ok_or(not_found(BAR, bar))?;
    let current = bar_ref(&score.tracks, path).ok_or(not_found(BAR, bar))?;
    let first = VoiceSlot::default();
    if let Some(voice) = current.voices.iter().find(|voice| voice.slot == first) {
        let voice = voice.id;
        return insert_into_voice(score, voice, index, duration);
    }
    check_insert_index(index, 0)?;
    reserve(score, 2)?;
    let Score { tracks, ids, .. } = score;
    let target = bar_in(tracks, path).ok_or(not_found(BAR, bar))?;
    let voice: VoiceId = next_id(ids)?;
    let beat: BeatId = next_id(ids)?;
    target.voices.insert(
        0,
        Voice {
            id: voice,
            slot: first,
            beats: vec![new_beat(beat, duration)],
        },
    );
    Ok(Command::RemoveVoice { voice })
}

pub(crate) fn remove(score: &mut Score, beat: BeatId) -> Result<Command, CommandError> {
    let path = find_beat(score, beat)?;
    let track = score.tracks.get(path.track).ok_or(not_found(BEAT, beat))?;
    let current = beat_ref(&score.tracks, path).ok_or(not_found(BEAT, beat))?;
    let owner = voice_ref(&score.tracks, voice_path(path)).ok_or(not_found(BEAT, beat))?;
    if score.find_voice(owner.id) != Some(voice_path(path)) {
        return Err(CommandError::invalid(format!(
            "voice id {} is not unique, so the removal could not be undone; run diagnose",
            owner.id.0.0
        )));
    }
    check_beat(track, current).map_err(already_broken)?;
    check_removable(score, beat_ids(current))?;
    let voice = voice_in(&mut score.tracks, voice_path(path)).ok_or(not_found(BEAT, beat))?;
    if path.beat >= voice.beats.len() {
        return Err(not_found(BEAT, beat));
    }
    let voice_id = voice.id;
    let removed = voice.beats.remove(path.beat);
    let extensions = take_extensions(&mut score.extensions, beat_ids(&removed));
    Ok(Command::RestoreBeat {
        voice: voice_id,
        index: path.beat,
        beat: removed,
        extensions,
    })
}

pub(crate) fn restore(
    score: &mut Score,
    voice: VoiceId,
    index: usize,
    beat: &Beat,
    extensions: &[ExtensionEntry],
) -> Result<Command, CommandError> {
    let (path, len) = find_voice(score, voice)?;
    check_insert_index(index, len)?;
    let track = score
        .tracks
        .get(path.track)
        .ok_or(not_found(VOICE, voice))?;
    check_beat(track, beat)?;
    check_restorable(score, beat_ids(beat), extensions)?;
    let target = voice_in(&mut score.tracks, path).ok_or(not_found(VOICE, voice))?;
    target.beats.insert(index, beat.clone());
    put_extensions(&mut score.extensions, extensions);
    Ok(Command::RemoveBeat { beat: beat.id })
}

pub(crate) fn set_duration(
    score: &mut Score,
    beat: BeatId,
    duration: Duration,
) -> Result<Command, CommandError> {
    let path = find_beat(score, beat)?;
    let target = beat_in(&mut score.tracks, path).ok_or(not_found(BEAT, beat))?;
    let old = std::mem::replace(&mut target.duration, duration);
    Ok(Command::SetBeatDuration {
        beat,
        duration: old,
    })
}
