use std::path::Path;

use opentab_model::{
    Bar, BarId, Beat, BeatEffects, BeatId, Clef, Duration, ElementId, Fraction, Instrument,
    InstrumentKind, KeySignature, MasterBar, MasterBarId, NoteValue, Pitch, Score, Staff, StaffId,
    Tempo, TempoChange, TempoUnit, TimeSignature, Track, TrackId, Transpose, Tuning, Voice,
    VoiceId, VoiceSlot,
};
use serde::Serialize;

use crate::error::CliError;
use crate::files::{self, Overwrite};
use crate::output::Rendered;
use crate::text::escape_controls;

const GUITAR_PROGRAM: u8 = 25;
const STANDARD_TUNING: [u8; 6] = [64, 59, 55, 50, 45, 40];
const GUITAR_OCTAVE: i8 = -1;
const DEFAULT_BPM: f64 = 120.0;

#[derive(Serialize)]
pub struct Created {
    created: String,
}

pub fn run(out: &Path, title: Option<String>) -> Result<Rendered<Created>, CliError> {
    files::require_format(out)?;
    let mut score = blank_score()?;
    score.meta.title = title.unwrap_or_default();
    files::write_score(out, &score, Overwrite::Refused)?;
    let created = out.display().to_string();
    let human = format!("created {}", escape_controls(&created));
    Ok(Rendered {
        wire: Created { created },
        human,
    })
}

fn blank_score() -> Result<Score, CliError> {
    let mut score = Score::new();
    let time_signature = TimeSignature::new(4, 4).map_err(CliError::internal)?;
    let bpm = Tempo::new(DEFAULT_BPM).map_err(CliError::internal)?;
    let master_bar = MasterBar {
        id: alloc::<MasterBarId>(&mut score)?,
        time_signature,
        key_signature: KeySignature::default(),
        repeat_start: false,
        repeat_end: None,
        alternate_endings: 0,
        double_bar: false,
        marker: None,
        tempo_changes: vec![TempoChange {
            position: Fraction::from_integer(0),
            bpm,
            unit: TempoUnit::Quarter,
        }],
    };
    let track = guitar_track(&mut score)?;
    score.master_bars.push(master_bar);
    score.tracks.push(track);
    Ok(score)
}

fn guitar_track(score: &mut Score) -> Result<Track, CliError> {
    let rest = Beat {
        id: alloc::<BeatId>(score)?,
        duration: Duration::new(NoteValue::Whole, 0, None).map_err(CliError::internal)?,
        notes: Vec::new(),
        effects: BeatEffects::default(),
    };
    let voice = Voice {
        id: alloc::<VoiceId>(score)?,
        slot: VoiceSlot::default(),
        beats: vec![rest],
    };
    let bar = Bar {
        id: alloc::<BarId>(score)?,
        clef: Clef::Treble,
        voices: vec![voice],
    };
    Ok(Track {
        id: alloc::<TrackId>(score)?,
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
        transpose: Transpose::new(0, GUITAR_OCTAVE).map_err(CliError::internal)?,
        staves: vec![Staff {
            id: alloc::<StaffId>(score)?,
            bars: vec![bar],
        }],
    })
}

fn alloc<T: From<ElementId>>(score: &mut Score) -> Result<T, CliError> {
    score.alloc().ok_or_else(CliError::exhausted)
}
