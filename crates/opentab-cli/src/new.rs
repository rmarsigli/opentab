use std::path::Path;

use opentab_model::{
    Bar, BarId, Beat, BeatEffects, BeatId, Duration, Fraction, Instrument, InstrumentKind,
    KeySignature, MasterBar, MasterBarId, ModelError, NoteValue, Pitch, Score, Staff, StaffId,
    Tempo, TempoChange, TimeSignature, Track, TrackId, Tuning, Voice, VoiceId,
};
use serde::Serialize;

use crate::error::CliError;
use crate::files::{self, Overwrite};
use crate::output::Report;

const GUITAR_PROGRAM: u8 = 25;
const STANDARD_TUNING: [u8; 6] = [64, 59, 55, 50, 45, 40];
const DEFAULT_BPM: f64 = 120.0;

#[derive(Serialize)]
pub struct Created {
    created: String,
}

impl Report for Created {
    fn human(&self) -> String {
        format!("created {}", self.created)
    }
}

pub fn run(out: &Path, title: Option<String>) -> Result<Created, CliError> {
    files::require_format(out)?;
    let mut score = blank_score().map_err(CliError::internal)?;
    score.meta.title = title.unwrap_or_default();
    files::write_score(out, &score, Overwrite::Refused)?;
    Ok(Created {
        created: out.display().to_string(),
    })
}

fn blank_score() -> Result<Score, ModelError> {
    let mut score = Score::new();
    let time_signature = TimeSignature::new(4, 4)?;
    let bpm = Tempo::new(DEFAULT_BPM)?;
    let master_bar = MasterBar {
        id: score.alloc::<MasterBarId>(),
        time_signature,
        key_signature: KeySignature {
            accidentals: 0,
            minor: false,
        },
        repeat_start: false,
        repeat_end: None,
        alternate_endings: 0,
        marker: None,
        tempo_changes: vec![TempoChange {
            position: Fraction::from_integer(0),
            bpm,
        }],
    };
    let track = guitar_track(&mut score)?;
    score.master_bars.push(master_bar);
    score.tracks.push(track);
    Ok(score)
}

fn guitar_track(score: &mut Score) -> Result<Track, ModelError> {
    let rest = Beat {
        id: score.alloc::<BeatId>(),
        duration: Duration::new(NoteValue::Whole, 0, None)?,
        notes: Vec::new(),
        effects: BeatEffects::default(),
    };
    let voice = Voice {
        id: score.alloc::<VoiceId>(),
        beats: vec![rest],
    };
    let bar = Bar {
        id: score.alloc::<BarId>(),
        voices: vec![voice],
    };
    Ok(Track {
        id: score.alloc::<TrackId>(),
        name: "Guitar".to_string(),
        short_name: "Gtr".to_string(),
        instrument: Instrument {
            kind: InstrumentKind::Fretted,
            midi_program: GUITAR_PROGRAM,
        },
        tuning: Tuning {
            strings: STANDARD_TUNING.iter().copied().map(Pitch).collect(),
        },
        capo: 0,
        staves: vec![Staff {
            id: score.alloc::<StaffId>(),
            bars: vec![bar],
        }],
    })
}
