use opentab_model::{Diagnostic, InstrumentKind, Pitch, Score, diagnose};
use serde::Serialize;

use crate::id_tree::IdTree;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TrackInfo {
    pub id: u64,
    pub name: String,
    pub kind: &'static str,
    pub strings: Vec<Pitch>,
    pub capo: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TimeSignatureCount(pub String, pub usize);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Info {
    pub title: String,
    pub artist: String,
    pub tracks: Vec<TrackInfo>,
    pub master_bars: usize,
    pub time_signatures: Vec<TimeSignatureCount>,
    pub diagnostics: Vec<Diagnostic>,
    pub ids: IdTree,
}

impl Info {
    pub fn of(score: &Score) -> Info {
        Info {
            title: score.meta.title.clone(),
            artist: score.meta.artist.clone(),
            tracks: score
                .tracks
                .iter()
                .map(|track| TrackInfo {
                    id: track.id.0.0,
                    name: track.name.clone(),
                    kind: match track.instrument.kind {
                        InstrumentKind::Fretted => "fretted",
                        InstrumentKind::Percussion => "percussion",
                    },
                    strings: track.tuning.strings.clone(),
                    capo: track.capo,
                })
                .collect(),
            master_bars: score.master_bars.len(),
            time_signatures: time_signatures(score),
            diagnostics: diagnose(score),
            ids: IdTree::of(score),
        }
    }
}

fn time_signatures(score: &Score) -> Vec<TimeSignatureCount> {
    let mut counts: Vec<TimeSignatureCount> = Vec::new();
    for master_bar in &score.master_bars {
        let label = format!(
            "{}/{}",
            master_bar.time_signature.numerator(),
            master_bar.time_signature.denominator()
        );
        match counts.iter_mut().find(|seen| seen.0 == label) {
            Some(seen) => seen.1 += 1,
            None => counts.push(TimeSignatureCount(label, 1)),
        }
    }
    counts
}
