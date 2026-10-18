use std::collections::BTreeSet;

use opentab_model::{Bar, Beat, Score, Staff, Track, Voice};
use serde::Serialize;

use crate::ids::score_ids;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IdTree {
    pub master_bars: Vec<u64>,
    pub tracks: Vec<TrackIds>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrackIds {
    pub id: u64,
    pub staves: Vec<StaffIds>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StaffIds {
    pub id: u64,
    pub bars: Vec<BarIds>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BarIds {
    pub id: u64,
    pub voices: Vec<VoiceIds>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VoiceIds {
    pub id: u64,
    pub beats: Vec<BeatIds>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BeatIds {
    pub id: u64,
    pub notes: Vec<u64>,
}

impl IdTree {
    pub fn of(score: &Score) -> IdTree {
        IdTree {
            master_bars: score
                .master_bars
                .iter()
                .map(|master_bar| master_bar.id.0.0)
                .collect(),
            tracks: score.tracks.iter().map(track).collect(),
        }
    }
}

fn track(track: &Track) -> TrackIds {
    TrackIds {
        id: track.id.0.0,
        staves: track.staves.iter().map(staff).collect(),
    }
}

fn staff(staff: &Staff) -> StaffIds {
    StaffIds {
        id: staff.id.0.0,
        bars: staff.bars.iter().map(bar).collect(),
    }
}

fn bar(bar: &Bar) -> BarIds {
    BarIds {
        id: bar.id.0.0,
        voices: bar.voices.iter().map(voice).collect(),
    }
}

fn voice(voice: &Voice) -> VoiceIds {
    VoiceIds {
        id: voice.id.0.0,
        beats: voice.beats.iter().map(beat).collect(),
    }
}

fn beat(beat: &Beat) -> BeatIds {
    BeatIds {
        id: beat.id.0.0,
        notes: beat.notes.iter().map(|note| note.id.0.0).collect(),
    }
}

pub fn element_ids(score: &Score) -> BTreeSet<u64> {
    score_ids(score).map(|id| id.0).collect()
}
