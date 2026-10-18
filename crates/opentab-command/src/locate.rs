use opentab_model::{
    Bar, BarId, Beat, BeatPath, ElementId, Note, NotePath, Score, StaffId, Track, TrackId, Voice,
    VoicePath,
};

use crate::CommandError;

pub(crate) fn not_found(what: &'static str, id: impl Into<ElementId>) -> CommandError {
    CommandError::NotFound {
        what,
        id: id.into().0,
    }
}

pub(crate) fn voice_path(path: BeatPath) -> VoicePath {
    VoicePath {
        track: path.track,
        staff: path.staff,
        bar: path.bar,
        voice: path.voice,
    }
}

pub(crate) fn voice_ref(tracks: &[Track], path: VoicePath) -> Option<&Voice> {
    tracks
        .get(path.track)?
        .staves
        .get(path.staff)?
        .bars
        .get(path.bar)?
        .voices
        .get(path.voice)
}

pub(crate) fn voice_in(tracks: &mut [Track], path: VoicePath) -> Option<&mut Voice> {
    tracks
        .get_mut(path.track)?
        .staves
        .get_mut(path.staff)?
        .bars
        .get_mut(path.bar)?
        .voices
        .get_mut(path.voice)
}

pub(crate) fn beat_ref(tracks: &[Track], path: BeatPath) -> Option<&Beat> {
    voice_ref(tracks, voice_path(path))?.beats.get(path.beat)
}

pub(crate) fn beat_in(tracks: &mut [Track], path: BeatPath) -> Option<&mut Beat> {
    voice_in(tracks, voice_path(path))?.beats.get_mut(path.beat)
}

pub(crate) fn note_in(tracks: &mut [Track], path: NotePath) -> Option<&mut Note> {
    beat_in(tracks, path.beat)?.notes.get_mut(path.note)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct StaffPath {
    pub(crate) track: usize,
    pub(crate) staff: usize,
}

pub(crate) fn find_staff(
    score: &Score,
    track: TrackId,
    staff: StaffId,
) -> Result<StaffPath, CommandError> {
    let track_index = score
        .tracks
        .iter()
        .position(|candidate| candidate.id == track)
        .ok_or(not_found("track", track))?;
    let staff_index = score
        .tracks
        .get(track_index)
        .and_then(|found| {
            found
                .staves
                .iter()
                .position(|candidate| candidate.id == staff)
        })
        .ok_or(not_found("staff", staff))?;
    Ok(StaffPath {
        track: track_index,
        staff: staff_index,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BarPath {
    pub(crate) track: usize,
    pub(crate) staff: usize,
    pub(crate) bar: usize,
}

pub(crate) fn find_bar(score: &Score, id: BarId) -> Option<BarPath> {
    for (track, owner) in score.tracks.iter().enumerate() {
        for (staff, held) in owner.staves.iter().enumerate() {
            if let Some(bar) = held.bars.iter().position(|bar| bar.id == id) {
                return Some(BarPath { track, staff, bar });
            }
        }
    }
    None
}

pub(crate) fn bar_ref(tracks: &[Track], path: BarPath) -> Option<&Bar> {
    tracks
        .get(path.track)?
        .staves
        .get(path.staff)?
        .bars
        .get(path.bar)
}

pub(crate) fn bar_in(tracks: &mut [Track], path: BarPath) -> Option<&mut Bar> {
    tracks
        .get_mut(path.track)?
        .staves
        .get_mut(path.staff)?
        .bars
        .get_mut(path.bar)
}
