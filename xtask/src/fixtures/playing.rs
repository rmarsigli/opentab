use opentab_model::{
    BeatEffects, BendPoint, Fraction, NoteEffects, NoteValue, Score, Tempo, TempoChange, TempoUnit,
    TremoloBar, Trill, TrillSpeed,
};

use crate::fixtures::FixtureError;
use crate::fixtures::sketch::{
    BeatSketch, Sketch, TrackSetup, affected, beat, duration, fretted, voice, with_effects,
};

const RAMP_BARS: usize = 6;
const WHAMMY_BARS: usize = 4;
const TRILL_CAPO: u8 = 2;

struct Ramp {
    bar: usize,
    position: Fraction,
    bpm: f64,
    unit: TempoUnit,
    linear: bool,
}

fn ramp(bar: usize, position: Fraction, bpm: f64, unit: TempoUnit, linear: bool) -> Ramp {
    Ramp {
        bar,
        position,
        bpm,
        unit,
        linear,
    }
}

fn scale(sketch: &mut Sketch, track: usize, bars: usize) -> Result<(), FixtureError> {
    let quarter = duration(NoteValue::Quarter)?;
    for bar in 0..bars {
        let step = u8::try_from(bar).unwrap_or_default();
        let line: Vec<BeatSketch> = (0..4u8)
            .map(|index| beat(quarter, vec![fretted(2, step + index)]))
            .collect();
        sketch.fill(track, bar, vec![voice(0, line)])?;
    }
    Ok(())
}

pub fn tempo_ramp() -> Result<Score, FixtureError> {
    let mut sketch = Sketch::new("Tempo ramps");
    sketch.master_bars(RAMP_BARS, 4, 4)?;
    let zero = Fraction::from_integer(0);
    let ramps = [
        ramp(0, zero, 60.0, TempoUnit::Quarter, true),
        ramp(2, zero, 120.0, TempoUnit::Quarter, true),
        ramp(3, Fraction::new(1, 2), 90.0, TempoUnit::Quarter, false),
        ramp(4, zero, 80.0, TempoUnit::DottedQuarter, true),
        ramp(5, zero, 140.5, TempoUnit::Quarter, true),
    ];
    for item in ramps {
        let master = sketch
            .score
            .master_bars
            .get_mut(item.bar)
            .ok_or_else(|| FixtureError::new(format!("no master bar {}", item.bar)))?;
        master.tempo_changes.push(TempoChange {
            position: item.position,
            bpm: Tempo::new(item.bpm)?,
            unit: item.unit,
            linear: item.linear,
        });
    }
    let track = sketch.track(TrackSetup::guitar("Guitar", "Gtr"))?;
    scale(&mut sketch, track, RAMP_BARS)?;
    Ok(sketch.finish())
}

type Curve = [(u8, i16); 4];

const CURVES: [Curve; 8] = [
    [(0, 0), (35, -100), (35, -100), (99, -200)],
    [(0, 0), (50, -200), (50, -200), (100, 0)],
    [(0, 0), (50, 100), (50, 100), (100, 100)],
    [(0, -200), (0, -200), (0, -200), (100, 0)],
    [(0, 0), (50, 25), (50, 25), (100, 0)],
    [(0, 0), (25, -50), (75, -50), (100, -150)],
    [(0, 0), (60, -400), (60, -400), (100, -400)],
    [(0, 0), (50, 0), (50, 0), (100, 0)],
];

fn curve(points: Curve) -> Result<TremoloBar, FixtureError> {
    let [origin, start, end, destination] =
        points.map(|(offset, value)| BendPoint::new(offset, value));
    Ok(TremoloBar::new(origin?, start?, end?, destination?)?)
}

pub fn tremolo_bar() -> Result<Score, FixtureError> {
    let mut sketch = Sketch::new("Tremolo bar");
    sketch.master_bars(WHAMMY_BARS, 4, 4)?;
    let track = sketch.track(TrackSetup::guitar("Guitar", "Gtr"))?;
    let half = duration(NoteValue::Half)?;
    let mut curves = CURVES.into_iter();
    for bar in 0..WHAMMY_BARS {
        let mut beats = Vec::new();
        for index in 0..2u8 {
            let shape = curves
                .next()
                .ok_or_else(|| FixtureError::new("too few curves".to_string()))?;
            let effects = BeatEffects {
                tremolo_bar: Some(curve(shape)?),
                ..BeatEffects::default()
            };
            beats.push(with_effects(half, vec![fretted(1, 5 + index)], effects));
        }
        sketch.fill(track, bar, vec![voice(0, beats)])?;
    }
    Ok(sketch.finish())
}

const TRILLS: [(u8, u8, u8, NoteValue); 4] = [
    (0, 5, 7, NoteValue::ThirtySecond),
    (1, 3, 5, NoteValue::Sixteenth),
    (2, 2, 4, NoteValue::Eighth),
    (3, 0, 2, NoteValue::Quarter),
];

pub fn trill() -> Result<Score, FixtureError> {
    let mut sketch = Sketch::new("Trills");
    sketch.master_bars(TRILLS.len(), 4, 4)?;
    let mut setup = TrackSetup::guitar("Guitar", "Gtr");
    setup.capo = TRILL_CAPO;
    let track = sketch.track(setup)?;
    let whole = duration(NoteValue::Whole)?;
    for (bar, (string, fret, auxiliary, speed)) in TRILLS.into_iter().enumerate() {
        let effects = NoteEffects {
            trill: Some(Trill {
                fret: auxiliary,
                speed: TrillSpeed::new(speed)?,
            }),
            ..NoteEffects::default()
        };
        sketch.fill(
            track,
            bar,
            vec![voice(
                0,
                vec![beat(whole, vec![affected(string, fret, effects)])],
            )],
        )?;
    }
    Ok(sketch.finish())
}
