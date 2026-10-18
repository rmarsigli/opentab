use opentab_model::{
    Clef, Fraction, KeySignature, Marker, NoteValue, Score, Tempo, TempoChange, TempoUnit,
    blank_score,
};

use crate::fixtures::FixtureError;
use crate::fixtures::sketch::{
    BeatSketch, NoteSketch, Sketch, TrackSetup, beat, dotted, duration, fretted, rest, tuplet,
    voice,
};

pub fn metadata() -> Result<Score, FixtureError> {
    let mut sketch = Sketch::new("Título <one> & \"two\" 'três' 🎸");
    let meta = &mut sketch.score.meta;
    meta.subtitle = "Sous-titre — ünïcödé 日本語".to_string();
    meta.artist = "The OpenTab Fixture Band & Friends".to_string();
    meta.album = "Synthetic <Album> 2026".to_string();
    meta.words = "Words by A. Writer".to_string();
    meta.music = "Music by B. Composer".to_string();
    meta.copyright = "© 2026 OpenTab contributors".to_string();
    meta.tab_author = "Tabbed by C. Tabber".to_string();
    meta.instructions = "Play softly, then loud > soft".to_string();
    meta.notices = "Line one\nLine two & three".to_string();
    sketch.master_bars(1, 4, 4)?;
    let track = sketch.track(TrackSetup::guitar("Guitarra ção <1> & 🎸", "Gtr ç"))?;
    let quarter = duration(NoteValue::Quarter)?;
    sketch.fill(
        track,
        0,
        vec![voice(
            0,
            vec![
                beat(quarter, vec![fretted(5, 3)]),
                beat(quarter, vec![fretted(4, 2)]),
                beat(quarter, vec![fretted(3, 0)]),
                beat(quarter, vec![fretted(1, 1), fretted(0, 0)]),
            ],
        )],
    )?;
    Ok(sketch.finish())
}

const SIGNATURES: [(u8, u8); 15] = [
    (4, 4),
    (3, 4),
    (6, 8),
    (7, 8),
    (12, 8),
    (2, 2),
    (5, 4),
    (2, 4),
    (9, 8),
    (3, 8),
    (4, 4),
    (6, 4),
    (5, 8),
    (3, 2),
    (4, 4),
];

fn value_for(denominator: u8) -> NoteValue {
    match denominator {
        2 => NoteValue::Half,
        8 => NoteValue::Eighth,
        _ => NoteValue::Quarter,
    }
}

pub fn signatures() -> Result<Score, FixtureError> {
    let mut sketch = Sketch::new("Time and key signatures");
    sketch.score.anacrusis = true;
    for (index, (numerator, denominator)) in SIGNATURES.into_iter().enumerate() {
        let bar = sketch.master_bar(numerator, denominator)?;
        let accidentals =
            i8::try_from(index).map_err(|_| FixtureError::new("bar index".to_string()))? - 7;
        sketch.score.master_bars[bar].key_signature =
            KeySignature::new(accidentals, index % 2 == 1)?;
    }
    let track = sketch.track(TrackSetup::guitar("Guitar", "Gtr"))?;
    for (index, (numerator, denominator)) in SIGNATURES.into_iter().enumerate() {
        let value = duration(value_for(denominator))?;
        let count = if index == 0 { 1 } else { numerator };
        let beats: Vec<BeatSketch> = (0..count)
            .map(|step| {
                let fret = u8::try_from((index + usize::from(step)) % 12).unwrap_or(0);
                beat(value, vec![fretted(4, fret)])
            })
            .collect();
        sketch.fill(track, index, vec![voice(0, beats)])?;
    }
    Ok(sketch.finish())
}

fn marker(letter: &str, text: &str) -> Option<Marker> {
    Some(Marker {
        text: text.to_string(),
        letter: letter.to_string(),
    })
}

pub fn repeats() -> Result<Score, FixtureError> {
    let mut sketch = Sketch::new("Repeats, endings and markers");
    sketch.master_bars(8, 4, 4)?;
    let bars = &mut sketch.score.master_bars;
    bars[0].marker = marker("A", "Intro");
    bars[0].repeat_start = true;
    bars[1].repeat_end = Some(3);
    bars[2].marker = marker("B", "Verse");
    bars[2].repeat_start = true;
    bars[3].alternate_endings = 0b011;
    bars[3].repeat_end = Some(3);
    bars[4].alternate_endings = 0b100;
    bars[4].double_bar = true;
    bars[5].marker = marker("", "Bridge");
    bars[6].marker = marker("C", "");
    bars[6].double_bar = true;
    bars[7].repeat_start = true;
    bars[7].repeat_end = Some(2);
    let track = sketch.track(TrackSetup::guitar("Guitar", "Gtr"))?;
    let whole = duration(NoteValue::Whole)?;
    for bar in 0..8u8 {
        sketch.fill(
            track,
            usize::from(bar),
            vec![voice(0, vec![beat(whole, vec![fretted(2, bar)])])],
        )?;
    }
    Ok(sketch.finish())
}

fn change(position: Fraction, bpm: f64, unit: TempoUnit) -> Result<TempoChange, FixtureError> {
    Ok(TempoChange {
        position,
        bpm: Tempo::new(bpm)?,
        unit,
    })
}

pub fn tempo() -> Result<Score, FixtureError> {
    let mut sketch = Sketch::new("Tempo changes");
    sketch.master_bars(2, 4, 4)?;
    sketch.master_bar(3, 4)?;
    sketch.master_bar(6, 8)?;
    sketch.master_bar(4, 4)?;
    let zero = Fraction::from_integer(0);
    let bars = &mut sketch.score.master_bars;
    bars[0].tempo_changes = vec![change(zero, 120.0, TempoUnit::Quarter)?];
    bars[1].tempo_changes = vec![
        change(zero, 90.5, TempoUnit::Eighth)?,
        change(Fraction::new(1, 2), 100.0, TempoUnit::DottedQuarter)?,
    ];
    bars[2].tempo_changes = vec![
        change(zero, 60.0, TempoUnit::Half)?,
        change(Fraction::new(1, 4), 72.0, TempoUnit::DottedHalf)?,
    ];
    bars[3].tempo_changes = vec![change(zero, 133.25, TempoUnit::DottedQuarter)?];
    bars[4].tempo_changes = vec![
        change(Fraction::new(1, 4), 200.0, TempoUnit::Quarter)?,
        change(Fraction::new(3, 4), 45.0, TempoUnit::Eighth)?,
    ];
    sketch.track(TrackSetup::guitar("Guitar", "Gtr"))?;
    Ok(sketch.finish())
}

fn string_count(tuning: &[u8]) -> Result<u8, FixtureError> {
    u8::try_from(tuning.len()).map_err(|_| FixtureError::new("tuning size".to_string()))
}

fn tuning_track(sketch: &mut Sketch, setup: TrackSetup) -> Result<usize, FixtureError> {
    let strings = string_count(&setup.tuning)?;
    let track = sketch.track(setup)?;
    let chord: Vec<NoteSketch> = (0..strings)
        .map(|string| fretted(string, string % 3))
        .collect();
    sketch.fill(
        track,
        0,
        vec![voice(0, vec![beat(duration(NoteValue::Whole)?, chord)])],
    )?;
    let quarter = duration(NoteValue::Quarter)?;
    let walk: Vec<BeatSketch> = (0..4u8)
        .map(|step| {
            beat(
                quarter,
                vec![fretted(strings - 1 - step % strings, step + 1)],
            )
        })
        .collect();
    sketch.fill(track, 1, vec![voice(0, walk)])?;
    Ok(track)
}

pub fn tunings() -> Result<Score, FixtureError> {
    let mut sketch = Sketch::new("Tunings, capo, transpose and clefs");
    sketch.master_bars(2, 4, 4)?;
    let mut capo = TrackSetup::guitar("Capo guitar", "Capo");
    capo.capo = 2;
    capo.program = 25;
    let capo = tuning_track(&mut sketch, capo)?;
    sketch.clef(capo, 1, Clef::Bass)?;
    let mut drop = TrackSetup::guitar("Drop D", "DropD");
    drop.tuning = vec![64, 59, 55, 50, 45, 38];
    drop.program = 29;
    tuning_track(&mut sketch, drop)?;
    let mut seven = TrackSetup::guitar("Seven string", "7str");
    seven.tuning = vec![64, 59, 55, 50, 45, 40, 35];
    seven.capo = 3;
    seven.program = 30;
    tuning_track(&mut sketch, seven)?;
    let mut bass = TrackSetup::guitar("Bass", "Bass");
    bass.tuning = vec![43, 38, 33, 28];
    bass.program = 33;
    bass.clef = Clef::Bass;
    let bass = tuning_track(&mut sketch, bass)?;
    sketch.clef(bass, 1, Clef::Treble)?;
    let mut five = TrackSetup::guitar("Five string bass", "B5");
    five.tuning = vec![43, 38, 33, 28, 23];
    five.program = 34;
    five.clef = Clef::Bass;
    tuning_track(&mut sketch, five)?;
    let mut transposed = TrackSetup::guitar("Transposed", "Trp");
    transposed.chromatic = -2;
    transposed.octave = 0;
    transposed.program = 26;
    tuning_track(&mut sketch, transposed)?;
    let mut up = TrackSetup::guitar("Transposed up", "Up");
    up.chromatic = 5;
    up.octave = 1;
    up.program = 24;
    tuning_track(&mut sketch, up)?;
    let mut neutral = TrackSetup::guitar("Neutral clef", "Neu");
    neutral.clef = Clef::Neutral;
    neutral.program = 25;
    tuning_track(&mut sketch, neutral)?;
    let mut ukulele = TrackSetup::guitar("Ukulele", "Uke");
    ukulele.tuning = vec![69, 64, 60, 67];
    ukulele.octave = 0;
    ukulele.program = 24;
    tuning_track(&mut sketch, ukulele)?;
    Ok(sketch.finish())
}

fn notes(values: &[(NoteValue, u8)]) -> Result<Vec<BeatSketch>, FixtureError> {
    let mut beats = Vec::new();
    for (index, (value, string)) in values.iter().enumerate() {
        let fret = u8::try_from(index % 10).unwrap_or(0);
        beats.push(beat(duration(*value)?, vec![fretted(*string, fret)]));
    }
    Ok(beats)
}

fn group(count: u8, value: NoteValue, num: u8, den: u8) -> Result<Vec<BeatSketch>, FixtureError> {
    let mut beats = Vec::new();
    for step in 0..count {
        beats.push(beat(tuplet(value, num, den)?, vec![fretted(2, step)]));
    }
    Ok(beats)
}

pub fn durations() -> Result<Score, FixtureError> {
    let mut sketch = Sketch::new("Durations, dots and tuplets");
    sketch.master_bars(8, 4, 4)?;
    let track = sketch.track(TrackSetup::guitar("Guitar", "Gtr"))?;
    sketch.fill(track, 0, vec![voice(0, notes(&[(NoteValue::Whole, 3)])?)])?;
    sketch.fill(
        track,
        1,
        vec![voice(
            0,
            notes(&[
                (NoteValue::Half, 3),
                (NoteValue::Quarter, 2),
                (NoteValue::Eighth, 1),
                (NoteValue::Sixteenth, 0),
                (NoteValue::ThirtySecond, 1),
                (NoteValue::SixtyFourth, 2),
                (NoteValue::OneTwentyEighth, 3),
                (NoteValue::OneTwentyEighth, 4),
            ])?,
        )],
    )?;
    sketch.fill(
        track,
        2,
        vec![voice(
            0,
            vec![
                beat(dotted(NoteValue::Half, 1)?, vec![fretted(3, 2)]),
                rest(duration(NoteValue::Quarter)?),
            ],
        )],
    )?;
    sketch.fill(
        track,
        3,
        vec![voice(
            0,
            vec![
                beat(dotted(NoteValue::Half, 2)?, vec![fretted(3, 4)]),
                beat(duration(NoteValue::Eighth)?, vec![fretted(2, 5)]),
            ],
        )],
    )?;
    sketch.fill(
        track,
        4,
        vec![voice(
            0,
            vec![
                beat(dotted(NoteValue::Half, 3)?, vec![fretted(3, 5)]),
                beat(duration(NoteValue::Sixteenth)?, vec![fretted(2, 7)]),
            ],
        )],
    )?;
    let mut triplets = group(3, NoteValue::Eighth, 3, 2)?;
    triplets.extend(group(3, NoteValue::Quarter, 3, 2)?);
    triplets.push(beat(duration(NoteValue::Quarter)?, vec![fretted(1, 0)]));
    sketch.fill(track, 5, vec![voice(0, triplets)])?;
    let mut odd = group(5, NoteValue::Sixteenth, 5, 4)?;
    odd.extend(group(6, NoteValue::Sixteenth, 6, 4)?);
    odd.extend(group(7, NoteValue::Sixteenth, 7, 4)?);
    odd.extend(group(9, NoteValue::ThirtySecond, 9, 8)?);
    sketch.fill(track, 6, vec![voice(0, odd)])?;
    sketch.fill(
        track,
        7,
        vec![voice(
            0,
            vec![
                rest(duration(NoteValue::Half)?),
                rest(duration(NoteValue::Quarter)?),
                rest(duration(NoteValue::Eighth)?),
                rest(duration(NoteValue::Sixteenth)?),
                rest(duration(NoteValue::Sixteenth)?),
            ],
        )],
    )?;
    Ok(sketch.finish())
}

pub fn multi_voice() -> Result<Score, FixtureError> {
    let mut sketch = Sketch::new("Voices");
    sketch.master_bars(6, 4, 4)?;
    let track = sketch.track(TrackSetup::guitar("Guitar", "Gtr"))?;
    let quarter = duration(NoteValue::Quarter)?;
    let half = duration(NoteValue::Half)?;
    let whole = duration(NoteValue::Whole)?;
    let melody: Vec<BeatSketch> = (0..4u8)
        .map(|step| beat(quarter, vec![fretted(0, step + 1)]))
        .collect();
    sketch.fill(
        track,
        0,
        vec![
            voice(0, melody),
            voice(
                1,
                vec![
                    beat(half, vec![fretted(5, 0)]),
                    beat(half, vec![fretted(5, 3)]),
                ],
            ),
        ],
    )?;
    sketch.fill(
        track,
        1,
        vec![
            voice(0, vec![beat(whole, vec![fretted(0, 0)])]),
            voice(1, vec![beat(half, vec![fretted(2, 2)]), rest(half)]),
            voice(2, vec![rest(half), beat(half, vec![fretted(3, 2)])]),
            voice(3, vec![beat(whole, vec![fretted(5, 3)])]),
        ],
    )?;
    sketch.fill(
        track,
        2,
        vec![voice(1, vec![beat(whole, vec![fretted(4, 2)])])],
    )?;
    sketch.fill(
        track,
        3,
        vec![
            voice(0, vec![beat(half, vec![fretted(1, 1)]), rest(half)]),
            voice(2, vec![beat(whole, vec![fretted(4, 3)])]),
        ],
    )?;
    sketch.fill(track, 4, vec![voice(0, Vec::new())])?;
    sketch.fill(track, 5, Vec::new())?;
    Ok(sketch.finish())
}

pub fn multi_track() -> Result<Score, FixtureError> {
    let mut sketch = Sketch::new("Several tracks");
    sketch.master_bars(2, 4, 4)?;
    let mut lead = TrackSetup::guitar("Lead guitar", "Lead");
    lead.program = 29;
    let lead = sketch.track(lead)?;
    let mut rhythm = TrackSetup::guitar("Rhythm guitar", "Rhy");
    rhythm.program = 25;
    rhythm.staves = 2;
    let rhythm = sketch.track(rhythm)?;
    let mut bass = TrackSetup::guitar("Bass", "Bs");
    bass.tuning = vec![43, 38, 33, 28];
    bass.program = 33;
    bass.clef = Clef::Bass;
    let bass = sketch.track(bass)?;
    let mut keys = TrackSetup::guitar("Keys", "Ky");
    keys.program = 0;
    keys.octave = 0;
    let keys = sketch.track(keys)?;
    let quarter = duration(NoteValue::Quarter)?;
    let whole = duration(NoteValue::Whole)?;
    for bar in 0..2u8 {
        let index = usize::from(bar);
        let line: Vec<BeatSketch> = (0..4u8)
            .map(|step| beat(quarter, vec![fretted(1, 5 + step + bar)]))
            .collect();
        sketch.fill(lead, index, vec![voice(0, line)])?;
        let chord: Vec<NoteSketch> = (0..6u8).map(|string| fretted(string, bar)).collect();
        sketch.fill(rhythm, index, vec![voice(0, vec![beat(whole, chord)])])?;
        sketch.fill_staff(
            rhythm,
            1,
            index,
            vec![voice(0, vec![beat(whole, vec![fretted(5, 3 + bar)])])],
        )?;
        sketch.fill(
            bass,
            index,
            vec![voice(
                0,
                vec![
                    beat(duration(NoteValue::Half)?, vec![fretted(3, 3)]),
                    beat(duration(NoteValue::Half)?, vec![fretted(2, 2 + bar)]),
                ],
            )],
        )?;
        sketch.fill(
            keys,
            index,
            vec![voice(
                0,
                vec![beat(
                    whole,
                    vec![fretted(2, 2), fretted(1, 1), fretted(0, 0)],
                )],
            )],
        )?;
    }
    Ok(sketch.finish())
}

pub fn from_scratch() -> Result<Score, FixtureError> {
    Ok(Sketch::from_score(blank_score()?).finish())
}
