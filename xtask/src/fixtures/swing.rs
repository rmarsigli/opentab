use opentab_model::{
    Duration, Fraction, NoteValue, Score, TRIPLET_FEELS, Tempo, TempoChange, TempoUnit,
};

use crate::fixtures::FixtureError;
use crate::fixtures::sketch::{
    BeatSketch, Sketch, TrackSetup, beat, dotted, duration, fretted, rest, tuplet, voice,
};

const STRING: u8 = 5;
const PROGRAM: u8 = 25;
const SWING_BPM: f64 = 120.0;

#[derive(Debug, Clone, Copy)]
enum Length {
    Quarter,
    DottedQuarter,
    Eighth,
    DottedEighth,
    TripletEighth,
    Sixteenth,
    TripletSixteenth,
    ThirtySecond,
}

#[derive(Debug, Clone, Copy)]
enum Hit {
    Note(Length),
    Rest(Length),
}

struct Rhythm {
    numerator: u8,
    denominator: u8,
    hits: &'static [Hit],
}

use Hit::{Note as N, Rest as R};
use Length::{
    DottedEighth as DE, DottedQuarter as DQ, Eighth as E, Quarter as Q, Sixteenth as S,
    ThirtySecond as T, TripletEighth as TE, TripletSixteenth as TS,
};

const fn common(hits: &'static [Hit]) -> Rhythm {
    Rhythm {
        numerator: 4,
        denominator: 4,
        hits,
    }
}

const fn compound(hits: &'static [Hit]) -> Rhythm {
    Rhythm {
        numerator: 6,
        denominator: 8,
        hits,
    }
}

const RHYTHMS: [Rhythm; 16] = [
    common(&[N(E); 8]),
    common(&[N(S); 16]),
    common(&[
        N(E),
        N(S),
        N(S),
        N(E),
        N(S),
        N(S),
        N(E),
        N(S),
        N(S),
        N(E),
        N(S),
        N(S),
    ]),
    common(&[
        N(S),
        N(S),
        N(E),
        N(S),
        N(S),
        N(E),
        N(S),
        N(S),
        N(E),
        N(S),
        N(S),
        N(E),
    ]),
    common(&[N(DE), N(S), N(DE), N(S), N(DE), N(S), N(DE), N(S)]),
    common(&[N(S), N(DE), N(S), N(DE), N(S), N(DE), N(S), N(DE)]),
    common(&[N(Q), N(E), N(Q), N(E), N(Q)]),
    common(&[N(DQ), N(E), N(DQ), N(E)]),
    common(&[N(TE); 12]),
    common(&[
        N(S),
        N(E),
        N(S),
        N(S),
        N(E),
        N(S),
        N(S),
        N(E),
        N(S),
        N(S),
        N(E),
        N(S),
    ]),
    common(&[
        N(T),
        N(T),
        N(T),
        N(T),
        N(T),
        N(T),
        N(T),
        N(T),
        N(Q),
        N(S),
        N(S),
        N(S),
        N(S),
        N(E),
        N(E),
    ]),
    common(&[R(E), N(E), R(E), N(E), R(E), N(E), R(E), N(E)]),
    common(&[
        N(TE),
        N(TE),
        N(TE),
        N(TS),
        N(TS),
        N(TS),
        N(TS),
        N(TS),
        N(TS),
        N(E),
        N(E),
        N(Q),
    ]),
    compound(&[N(E); 6]),
    compound(&[N(S); 12]),
    compound(&[N(Q), N(E), N(Q), N(E)]),
];

fn length(length: Length) -> Result<Duration, FixtureError> {
    match length {
        Length::Quarter => duration(NoteValue::Quarter),
        Length::DottedQuarter => dotted(NoteValue::Quarter, 1),
        Length::Eighth => duration(NoteValue::Eighth),
        Length::DottedEighth => dotted(NoteValue::Eighth, 1),
        Length::TripletEighth => tuplet(NoteValue::Eighth, 3, 2),
        Length::Sixteenth => duration(NoteValue::Sixteenth),
        Length::TripletSixteenth => tuplet(NoteValue::Sixteenth, 3, 2),
        Length::ThirtySecond => duration(NoteValue::ThirtySecond),
    }
}

fn line(rhythm: &Rhythm) -> Result<Vec<BeatSketch>, FixtureError> {
    let mut beats = Vec::with_capacity(rhythm.hits.len());
    for (index, hit) in rhythm.hits.iter().enumerate() {
        let fret = u8::try_from(index)
            .map_err(|_| FixtureError::new(format!("beat {index} has no fret")))?;
        beats.push(match hit {
            Hit::Note(value) => beat(length(*value)?, vec![fretted(STRING, fret)]),
            Hit::Rest(value) => rest(length(*value)?),
        });
    }
    Ok(beats)
}

pub fn triplet_feel() -> Result<Score, FixtureError> {
    let mut sketch = Sketch::new("Triplet feel");
    for feel in TRIPLET_FEELS {
        for rhythm in &RHYTHMS {
            let bar = sketch.master_bar(rhythm.numerator, rhythm.denominator)?;
            sketch.score.master_bars[bar].triplet_feel = feel;
        }
    }
    sketch.score.master_bars[0].tempo_changes = vec![TempoChange {
        position: Fraction::from_integer(0),
        bpm: Tempo::new(SWING_BPM)?,
        unit: TempoUnit::Quarter,
        linear: false,
    }];
    let mut setup = TrackSetup::guitar("Guitar", "Gtr");
    setup.program = PROGRAM;
    let track = sketch.track(setup)?;
    for bar in 0..sketch.score.master_bars.len() {
        let rhythm = &RHYTHMS[bar % RHYTHMS.len()];
        sketch.fill(track, bar, vec![voice(0, line(rhythm)?)])?;
    }
    Ok(sketch.finish())
}
