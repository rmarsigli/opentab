use std::collections::BTreeSet;

use opentab_model::{Bar, Beat, Score, Staff, Track, Voice};
use serde::Serialize;

#[derive(Serialize)]
pub struct IdTree {
    pub master_bars: Vec<u64>,
    pub tracks: Vec<TrackIds>,
}

#[derive(Serialize)]
pub struct TrackIds {
    pub id: u64,
    pub staves: Vec<StaffIds>,
}

#[derive(Serialize)]
pub struct StaffIds {
    pub id: u64,
    pub bars: Vec<BarIds>,
}

#[derive(Serialize)]
pub struct BarIds {
    pub id: u64,
    pub voices: Vec<VoiceIds>,
}

#[derive(Serialize)]
pub struct VoiceIds {
    pub id: u64,
    pub beats: Vec<BeatIds>,
}

#[derive(Serialize)]
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

    pub fn all(&self) -> BTreeSet<u64> {
        let mut ids: BTreeSet<u64> = self.master_bars.iter().copied().collect();
        for track in &self.tracks {
            ids.insert(track.id);
            for staff in &track.staves {
                ids.insert(staff.id);
                for bar in &staff.bars {
                    ids.insert(bar.id);
                    for voice in &bar.voices {
                        ids.insert(voice.id);
                        for beat in &voice.beats {
                            ids.insert(beat.id);
                            ids.extend(beat.notes.iter().copied());
                        }
                    }
                }
            }
        }
        ids
    }

    pub fn human(&self) -> Vec<String> {
        let mut lines = vec!["ids:".to_string()];
        lines.push(format!("  master bars: {}", join(&self.master_bars)));
        for track in &self.tracks {
            lines.push(format!("  track {}", track.id));
            for staff in &track.staves {
                lines.push(format!("    staff {}", staff.id));
                for bar in &staff.bars {
                    lines.push(format!("      bar {}", bar.id));
                    for voice in &bar.voices {
                        lines.push(format!("        voice {}", voice.id));
                        for beat in &voice.beats {
                            lines.push(beat_line(beat));
                        }
                    }
                }
            }
        }
        lines
    }
}

fn beat_line(beat: &BeatIds) -> String {
    if beat.notes.is_empty() {
        format!("          beat {}", beat.id)
    } else {
        format!("          beat {}: notes {}", beat.id, join(&beat.notes))
    }
}

fn join(ids: &[u64]) -> String {
    let texts: Vec<String> = ids.iter().map(u64::to_string).collect();
    texts.join(" ")
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
