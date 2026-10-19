use opentab_model::{
    Clef, InstrumentKind, InstrumentSet, NoteEffects, NoteKind, NoteValue, Score, gm_drum_kit,
};

use crate::fixtures::FixtureError;
use crate::fixtures::sketch::{BeatSketch, NoteSketch, Sketch, TrackSetup, beat, duration, voice};

const HITS_PER_BEAT: usize = 4;
const BEATS_PER_BAR: usize = 4;
const DUMMY_STRINGS: usize = 6;

fn hit(articulation: u16) -> NoteSketch {
    NoteSketch {
        kind: NoteKind::Percussion { articulation },
        effects: NoteEffects::default(),
    }
}

pub fn drums() -> Result<Score, FixtureError> {
    let kit = gm_drum_kit();
    let count = u16::try_from(kit.articulations().len())
        .map_err(|_| FixtureError::new("the drum kit is too large".to_string()))?;
    let articulations: Vec<u16> = (0..count).collect();
    let quarter = duration(NoteValue::Quarter)?;
    let beats: Vec<Vec<u16>> = articulations
        .chunks(HITS_PER_BEAT)
        .map(<[u16]>::to_vec)
        .collect();
    let bars = beats.len().div_ceil(BEATS_PER_BAR);
    let mut sketch = Sketch::new("Drums");
    sketch.master_bars(bars, 4, 4)?;
    let track = sketch.track(TrackSetup {
        name: "Drums",
        short_name: "Drs",
        kind: InstrumentKind::Percussion,
        set: Some(InstrumentSet::DrumKit),
        program: 0,
        tuning: vec![0; DUMMY_STRINGS],
        capo: 0,
        chromatic: 0,
        octave: 0,
        staves: 1,
        clef: Clef::Neutral,
    })?;
    if let Some(drums) = sketch.score.tracks.get_mut(track) {
        drums.drum_kit = Some(kit);
    }
    for (bar, chords) in beats.chunks(BEATS_PER_BAR).enumerate() {
        let mut line: Vec<BeatSketch> = chords
            .iter()
            .map(|chord| beat(quarter, chord.iter().copied().map(hit).collect()))
            .collect();
        while line.len() < BEATS_PER_BAR {
            line.push(beat(quarter, Vec::new()));
        }
        sketch.fill(track, bar, vec![voice(0, line)])?;
    }
    Ok(sketch.finish())
}
