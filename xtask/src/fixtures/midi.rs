use opentab_model::{
    Automation, AutomationValue, Balance, Clef, Fraction, InstrumentSet, Level, MidiBank,
    MidiChannel, NoteValue, Score, Sound, SoundOverride, SoundRef,
};

use crate::fixtures::FixtureError;
use crate::fixtures::sketch::{BeatSketch, Sketch, TrackSetup, beat, duration, fretted, voice};

const BARS: usize = 4;
const CLEAN: u8 = 27;
const OVERDRIVE: u8 = 29;
const NYLON: u8 = 24;
const BASS: u8 = 33;

struct Point {
    bar: usize,
    position: Fraction,
    linear: bool,
    visible: bool,
    value: AutomationValue,
}

fn point(bar: usize, position: Fraction, value: AutomationValue) -> Point {
    Point {
        bar,
        position,
        linear: false,
        visible: true,
        value,
    }
}

fn level(millionths: u32) -> Result<Level, FixtureError> {
    Ok(Level::new(millionths)?)
}

fn balance(millionths: u32) -> Result<Balance, FixtureError> {
    Ok(Balance::new(millionths)?)
}

fn automate(sketch: &mut Sketch, track: usize, points: Vec<Point>) -> Result<(), FixtureError> {
    let mut automations = Vec::new();
    for point in points {
        let master_bar = sketch
            .score
            .master_bars
            .get(point.bar)
            .map(|master| master.id)
            .ok_or_else(|| FixtureError::new(format!("no master bar {}", point.bar)))?;
        automations.push(Automation {
            master_bar,
            position: point.position,
            linear: point.linear,
            visible: point.visible,
            value: point.value,
        });
    }
    let target = sketch
        .score
        .tracks
        .get_mut(track)
        .ok_or_else(|| FixtureError::new(format!("no track {track}")))?;
    target.automations = automations;
    Ok(())
}

fn scale(sketch: &mut Sketch, track: usize, string: u8) -> Result<(), FixtureError> {
    let quarter = duration(NoteValue::Quarter)?;
    for bar in 0..BARS {
        let step = u8::try_from(bar).unwrap_or_default();
        let line: Vec<BeatSketch> = (0..4u8)
            .map(|index| beat(quarter, vec![fretted(string, step + index)]))
            .collect();
        sketch.fill(track, bar, vec![voice(0, line)])?;
    }
    Ok(())
}

fn track_mut(sketch: &mut Sketch, track: usize) -> Result<&mut opentab_model::Track, FixtureError> {
    sketch
        .score
        .tracks
        .get_mut(track)
        .ok_or_else(|| FixtureError::new(format!("no track {track}")))
}

pub fn sound_change() -> Result<Score, FixtureError> {
    let mut sketch = Sketch::new("Sound changes");
    sketch.master_bars(BARS, 4, 4)?;
    let mut setup = TrackSetup::guitar("Guitar", "Gtr");
    setup.program = CLEAN;
    let guitar = sketch.track(setup)?;
    scale(&mut sketch, guitar, 2)?;
    track_mut(&mut sketch, guitar)?.sounds = vec![
        Sound {
            name: "Clean".to_string(),
            program: CLEAN,
            bank: MidiBank::default(),
        },
        Sound {
            name: "Overdrive".to_string(),
            program: OVERDRIVE,
            bank: MidiBank::default(),
        },
        Sound {
            name: "Nylon".to_string(),
            program: NYLON,
            bank: MidiBank::default(),
        },
    ];
    let half = Fraction::new(1, 2);
    let start = Fraction::from_integer(0);
    automate(
        &mut sketch,
        guitar,
        vec![
            point(0, start, AutomationValue::Sound(SoundRef(0))),
            point(1, start, AutomationValue::Sound(SoundRef(1))),
            point(2, half, AutomationValue::Sound(SoundRef(2))),
            point(3, start, AutomationValue::Sound(SoundRef(0))),
        ],
    )?;
    Ok(sketch.finish())
}

pub fn mixer() -> Result<Score, FixtureError> {
    let mut sketch = Sketch::new("Volume and pan");
    sketch.master_bars(BARS, 4, 4)?;
    let mut lead = TrackSetup::guitar("Guitar", "Gtr");
    lead.program = CLEAN;
    let guitar = sketch.track(lead)?;
    let mut low = TrackSetup::guitar("Bass", "Bs");
    low.set = Some(InstrumentSet::ElectricBass);
    low.tuning = vec![43, 38, 33, 28];
    low.program = BASS;
    low.clef = Clef::Bass;
    let bass = sketch.track(low)?;
    scale(&mut sketch, guitar, 1)?;
    scale(&mut sketch, bass, 3)?;
    let guitar_midi = &mut track_mut(&mut sketch, guitar)?.midi;
    guitar_midi.secondary = MidiChannel::new(1)?;
    guitar_midi.volume = level(800_000)?;
    guitar_midi.pan = balance(250_000)?;
    let bass_midi = &mut track_mut(&mut sketch, bass)?.midi;
    bass_midi.primary = MidiChannel::new(2)?;
    bass_midi.secondary = MidiChannel::new(3)?;
    bass_midi.volume = level(600_000)?;
    bass_midi.pan = balance(750_000)?;
    let start = Fraction::from_integer(0);
    let half = Fraction::new(1, 2);
    let mut ramp = point(2, start, AutomationValue::Volume(level(1_000_000)?));
    ramp.linear = true;
    automate(
        &mut sketch,
        guitar,
        vec![
            point(1, start, AutomationValue::Volume(level(400_000)?)),
            ramp,
            point(1, half, AutomationValue::Pan(balance(0)?)),
            point(3, start, AutomationValue::Pan(balance(1_000_000)?)),
        ],
    )?;
    let mut hidden = point(2, start, AutomationValue::Pan(balance(500_000)?));
    hidden.visible = false;
    automate(
        &mut sketch,
        bass,
        vec![
            point(1, start, AutomationValue::Volume(level(812_500)?)),
            hidden,
        ],
    )?;
    Ok(sketch.finish())
}

pub fn sound_override() -> Result<Score, FixtureError> {
    let mut sketch = Sketch::new("Sound bank override");
    sketch.master_bars(2, 4, 4)?;
    let mut lead = TrackSetup::guitar("Guitar", "Gtr");
    lead.program = CLEAN;
    let guitar = sketch.track(lead)?;
    let mut low = TrackSetup::guitar("Bass", "Bs");
    low.set = Some(InstrumentSet::ElectricBass);
    low.tuning = vec![43, 38, 33, 28];
    low.program = BASS;
    low.clef = Clef::Bass;
    let bass = sketch.track(low)?;
    let whole = duration(NoteValue::Whole)?;
    for bar in 0..2 {
        sketch.fill(
            guitar,
            bar,
            vec![voice(0, vec![beat(whole, vec![fretted(2, 2)])])],
        )?;
        sketch.fill(
            bass,
            bar,
            vec![voice(0, vec![beat(whole, vec![fretted(3, 0)])])],
        )?;
    }
    let ids: Vec<_> = sketch.score.tracks.iter().map(|track| track.id).collect();
    sketch.score.sound_overrides = ids
        .into_iter()
        .zip(["fluid-r3-gm", "musescore-general"])
        .map(|(track, bank)| SoundOverride {
            track,
            bank: bank.to_string(),
        })
        .collect();
    Ok(sketch.finish())
}
