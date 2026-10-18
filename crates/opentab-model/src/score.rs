use serde::{Deserialize, Serialize};

use crate::{
    Beat, BeatId, ContainerEntry, ElementId, Extensions, IdAllocator, MasterBar, MasterBarId,
    NoteId, OpaquePayload, Track, TrackId, Voice, VoiceId,
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScoreMeta {
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub album: String,
    pub words: String,
    pub music: String,
    pub copyright: String,
    pub tab_author: String,
    pub instructions: String,
    pub notices: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Score {
    pub meta: ScoreMeta,
    pub anacrusis: bool,
    pub master_bars: Vec<MasterBar>,
    pub tracks: Vec<Track>,
    pub extensions: Extensions,
    pub document_extensions: Vec<OpaquePayload>,
    pub container: Vec<ContainerEntry>,
    pub ids: IdAllocator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VoicePath {
    pub track: usize,
    pub staff: usize,
    pub bar: usize,
    pub voice: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BeatPath {
    pub track: usize,
    pub staff: usize,
    pub bar: usize,
    pub voice: usize,
    pub beat: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotePath {
    pub beat: BeatPath,
    pub note: usize,
}

impl Score {
    pub fn new() -> Score {
        Score::default()
    }

    pub fn alloc<T: From<ElementId>>(&mut self) -> Option<T> {
        self.ids.next().map(T::from)
    }

    pub fn find_track(&self, id: TrackId) -> Option<&Track> {
        self.tracks.iter().find(|track| track.id == id)
    }

    pub fn find_master_bar_index(&self, id: MasterBarId) -> Option<usize> {
        self.master_bars
            .iter()
            .position(|master_bar| master_bar.id == id)
    }

    pub fn find_voice(&self, id: VoiceId) -> Option<VoicePath> {
        self.voices()
            .find(|(_, voice)| voice.id == id)
            .map(|(path, _)| path)
    }

    pub fn find_beat(&self, id: BeatId) -> Option<BeatPath> {
        self.beats()
            .find(|(_, beat)| beat.id == id)
            .map(|(path, _)| path)
    }

    pub fn find_note(&self, id: NoteId) -> Option<NotePath> {
        self.beats().find_map(|(beat_path, beat)| {
            beat.notes
                .iter()
                .position(|note| note.id == id)
                .map(|note| NotePath {
                    beat: beat_path,
                    note,
                })
        })
    }

    fn voices(&self) -> impl Iterator<Item = (VoicePath, &Voice)> {
        self.tracks.iter().enumerate().flat_map(|(track, t)| {
            t.staves.iter().enumerate().flat_map(move |(staff, s)| {
                s.bars.iter().enumerate().flat_map(move |(bar, b)| {
                    b.voices.iter().enumerate().map(move |(voice, v)| {
                        let path = VoicePath {
                            track,
                            staff,
                            bar,
                            voice,
                        };
                        (path, v)
                    })
                })
            })
        })
    }

    fn beats(&self) -> impl Iterator<Item = (BeatPath, &Beat)> {
        self.voices().flat_map(|(path, voice)| {
            voice.beats.iter().enumerate().map(move |(beat, b)| {
                let beat_path = BeatPath {
                    track: path.track,
                    staff: path.staff,
                    bar: path.bar,
                    voice: path.voice,
                    beat,
                };
                (beat_path, b)
            })
        })
    }
}
