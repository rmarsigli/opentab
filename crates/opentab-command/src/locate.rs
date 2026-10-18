use opentab_model::{Beat, BeatPath, ElementId, Note, NotePath, Track, Voice, VoicePath};

use crate::CommandError;

pub(crate) fn not_found(what: &'static str, id: impl Into<ElementId>) -> CommandError {
    CommandError::NotFound {
        what,
        id: id.into().0,
    }
}

fn voice_path(path: BeatPath) -> VoicePath {
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
