use opentab_model::{
    BeatEffects, Bend, BendPoint, Duration, Dynamic, NoteEffects, NoteValue, Score, Vibrato,
};

use crate::fixtures::FixtureError;
use crate::fixtures::sketch::{
    BeatSketch, NoteSketch, Sketch, TrackSetup, affected, beat, dotted, duration, fretted, rest,
    voice, with_effects,
};

const BARS: usize = 12;
const FULL_BEND: i16 = 200;
const HALFWAY: u8 = 50;
const END: u8 = 100;

fn lasting(length: Duration, note: NoteSketch, dynamic: Dynamic) -> BeatSketch {
    with_effects(
        length,
        vec![note],
        BeatEffects {
            dynamic,
            ..BeatEffects::default()
        },
    )
}

fn struck(
    value: NoteValue,
    note: NoteSketch,
    dynamic: Dynamic,
) -> Result<BeatSketch, FixtureError> {
    Ok(lasting(duration(value)?, note, dynamic))
}

fn whole(note: NoteSketch, dynamic: Dynamic) -> Result<Vec<BeatSketch>, FixtureError> {
    Ok(vec![struck(NoteValue::Whole, note, dynamic)?])
}

fn half(note: NoteSketch, dynamic: Dynamic) -> Result<Vec<BeatSketch>, FixtureError> {
    Ok(vec![
        struck(NoteValue::Half, note, dynamic)?,
        rest(duration(NoteValue::Half)?),
    ])
}

fn bent(string: u8, fret: u8) -> Result<NoteSketch, FixtureError> {
    Ok(affected(
        string,
        fret,
        NoteEffects {
            bend: Some(Bend::new(
                BendPoint::new(0, 0)?,
                BendPoint::new(HALFWAY, FULL_BEND)?,
                BendPoint::new(HALFWAY, FULL_BEND)?,
                BendPoint::new(END, FULL_BEND)?,
            )?),
            ..NoteEffects::default()
        },
    ))
}

fn vibrating(string: u8, fret: u8) -> NoteSketch {
    affected(
        string,
        fret,
        NoteEffects {
            vibrato: Some(Vibrato::Slight),
            ..NoteEffects::default()
        },
    )
}

fn tied(string: u8, fret: u8, destination: bool, vibrato: Option<Vibrato>) -> NoteSketch {
    affected(
        string,
        fret,
        NoteEffects {
            tie_origin: !destination,
            tie_destination: destination,
            vibrato,
            ..NoteEffects::default()
        },
    )
}

fn two_voices(
    sketch: &mut Sketch,
    track: usize,
    bar: usize,
    first: Vec<BeatSketch>,
    second: Vec<BeatSketch>,
) -> Result<(), FixtureError> {
    sketch.fill(track, bar, vec![voice(0, first), voice(1, second)])
}

pub fn unisons() -> Result<Score, FixtureError> {
    let mut sketch = Sketch::new("Unisons of unequal velocity");
    sketch.master_bars(BARS, 4, 4)?;
    let mut setup = TrackSetup::guitar("Guitar", "Gtr");
    setup.staves = 2;
    let guitar = sketch.track(setup)?;
    let second = sketch.track(TrackSetup::guitar("Second guitar", "Gtr 2"))?;
    two_voices(
        &mut sketch,
        guitar,
        0,
        whole(fretted(2, 5), Dynamic::F)?,
        whole(fretted(1, 1), Dynamic::P)?,
    )?;
    two_voices(
        &mut sketch,
        guitar,
        1,
        whole(fretted(2, 7), Dynamic::P)?,
        whole(fretted(1, 3), Dynamic::F)?,
    )?;
    two_voices(
        &mut sketch,
        guitar,
        2,
        whole(fretted(1, 5), Dynamic::Ff)?,
        half(fretted(0, 0), Dynamic::Pp)?,
    )?;
    two_voices(
        &mut sketch,
        guitar,
        3,
        half(fretted(1, 6), Dynamic::Ff)?,
        whole(fretted(0, 1), Dynamic::Pp)?,
    )?;
    sketch.fill(guitar, 4, vec![voice(0, whole(fretted(1, 8), Dynamic::F)?)])?;
    sketch.fill_staff(
        guitar,
        1,
        4,
        vec![voice(0, whole(fretted(0, 3), Dynamic::P)?)],
    )?;
    sketch.fill(
        guitar,
        5,
        vec![voice(0, whole(fretted(1, 10), Dynamic::P)?)],
    )?;
    sketch.fill(second, 5, vec![voice(0, whole(fretted(0, 5), Dynamic::F)?)])?;
    two_voices(
        &mut sketch,
        guitar,
        6,
        whole(bent(2, 9)?, Dynamic::P)?,
        whole(fretted(0, 0), Dynamic::F)?,
    )?;
    two_voices(
        &mut sketch,
        guitar,
        7,
        whole(vibrating(2, 10), Dynamic::F)?,
        whole(fretted(1, 6), Dynamic::P)?,
    )?;
    let dotted_half = dotted(NoteValue::Half, 1)?;
    two_voices(
        &mut sketch,
        guitar,
        8,
        whole(fretted(2, 5), Dynamic::F)?,
        vec![
            rest(duration(NoteValue::Quarter)?),
            lasting(dotted_half, fretted(1, 1), Dynamic::P),
        ],
    )?;
    let half = duration(NoteValue::Half)?;
    sketch.fill(
        guitar,
        9,
        vec![voice(
            0,
            vec![rest(half), beat(half, vec![tied(2, 5, false, None)])],
        )],
    )?;
    sketch.fill(
        guitar,
        10,
        vec![voice(
            0,
            vec![
                beat(half, vec![tied(2, 5, true, Some(Vibrato::Slight))]),
                rest(half),
            ],
        )],
    )?;
    let quarter = duration(NoteValue::Quarter)?;
    sketch.fill(
        guitar,
        11,
        vec![voice(
            0,
            vec![
                beat(quarter, vec![tied(1, 1, false, None)]),
                beat(half, vec![tied(1, 1, true, Some(Vibrato::Wide))]),
                rest(quarter),
            ],
        )],
    )?;
    Ok(sketch.finish())
}
