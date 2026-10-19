use crate::{
    Bar, Beat, BeatEffects, Clef, Directions, Duration, ElementId, Fraction, Instrument,
    InstrumentKind, InstrumentSet, KeySignature, MasterBar, ModelError, NoteValue, Pitch, Score,
    Staff, Tempo, TempoChange, TempoUnit, TimeSignature, Track, TrackMidi, Transpose, Tuning,
    Voice, VoiceSlot,
};

const GUITAR_PROGRAM: u8 = 25;
const STANDARD_TUNING: [u8; 6] = [64, 59, 55, 50, 45, 40];
const GUITAR_OCTAVE: i8 = -1;
const DEFAULT_BPM: f64 = 120.0;

fn alloc<T: From<ElementId>>(score: &mut Score) -> Result<T, ModelError> {
    score.alloc().ok_or(ModelError::IdsExhausted)
}

pub fn blank_score() -> Result<Score, ModelError> {
    let mut score = Score::new();
    let master_bar = MasterBar {
        id: alloc(&mut score)?,
        time_signature: TimeSignature::new(4, 4)?,
        key_signature: KeySignature::default(),
        repeat_start: false,
        repeat_end: None,
        alternate_endings: 0,
        triplet_feel: Default::default(),
        double_bar: false,
        marker: None,
        directions: Directions::default(),
        tempo_changes: vec![TempoChange {
            position: Fraction::from_integer(0),
            bpm: Tempo::new(DEFAULT_BPM)?,
            unit: TempoUnit::Quarter,
            linear: false,
        }],
    };
    let track = guitar_track(&mut score)?;
    score.master_bars.push(master_bar);
    score.tracks.push(track);
    Ok(score)
}

fn guitar_track(score: &mut Score) -> Result<Track, ModelError> {
    let rest = Beat {
        id: alloc(score)?,
        duration: Duration::new(NoteValue::Whole, 0, None)?,
        notes: Vec::new(),
        effects: BeatEffects::default(),
    };
    let voice = Voice {
        id: alloc(score)?,
        slot: VoiceSlot::default(),
        beats: vec![rest],
    };
    let bar = Bar {
        id: alloc(score)?,
        clef: Clef::Treble,
        voices: vec![voice],
    };
    Ok(Track {
        id: alloc(score)?,
        name: "Guitar".to_string(),
        short_name: "Gtr".to_string(),
        instrument: Instrument {
            kind: InstrumentKind::Fretted,
            midi_program: GUITAR_PROGRAM,
            set: Some(InstrumentSet::ElectricGuitar),
        },
        drum_kit: None,
        midi: TrackMidi::default_for(InstrumentKind::Fretted),
        sounds: Vec::new(),
        tuning: Tuning {
            strings: STANDARD_TUNING.iter().copied().map(Pitch).collect(),
        },
        capo: 0,
        transpose: Transpose::new(0, GUITAR_OCTAVE)?,
        staves: vec![Staff {
            id: alloc(score)?,
            bars: vec![bar],
        }],
        automations: Vec::new(),
    })
}
