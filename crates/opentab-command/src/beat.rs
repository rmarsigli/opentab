use opentab_model::{Beat, BeatEffects, BeatId, BeatPath, Duration, Score, VoiceId, VoicePath};

use crate::ids::{beat_ids, check_restorable, next_id, put_extensions, reserve, take_extensions};
use crate::locate::{beat_in, not_found, voice_in, voice_path, voice_ref};
use crate::rules::check_beat;
use crate::{Command, CommandError, ExtensionEntry};

const BEAT: &str = "beat";
const VOICE: &str = "voice";

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
    target.beats.insert(
        index,
        Beat {
            id,
            duration,
            notes: Vec::new(),
            effects: BeatEffects::default(),
        },
    );
    Ok(Command::RemoveBeat { beat: id })
}

pub(crate) fn remove(score: &mut Score, beat: BeatId) -> Result<Command, CommandError> {
    let path = find_beat(score, beat)?;
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
