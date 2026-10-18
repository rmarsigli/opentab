use opentab_model::{
    Bar, Beat, BeatEffects, Clef, Duration, ElementId, Instrument, InstrumentKind, KeySignature,
    MasterBar, Note, NoteEffects, NoteKind, NoteValue, Pitch, Score, Staff, TimeSignature, Track,
    Transpose, Tuning, Tuplet, Voice, VoiceSlot,
};

use crate::fixtures::FixtureError;

pub const STANDARD: [u8; 6] = [64, 59, 55, 50, 45, 40];

pub struct TrackSetup {
    pub name: &'static str,
    pub short_name: &'static str,
    pub kind: InstrumentKind,
    pub program: u8,
    pub tuning: Vec<u8>,
    pub capo: u8,
    pub chromatic: i8,
    pub octave: i8,
    pub staves: usize,
    pub clef: Clef,
}

impl TrackSetup {
    pub fn guitar(name: &'static str, short_name: &'static str) -> TrackSetup {
        TrackSetup {
            name,
            short_name,
            kind: InstrumentKind::Fretted,
            program: 27,
            tuning: STANDARD.to_vec(),
            capo: 0,
            chromatic: 0,
            octave: -1,
            staves: 1,
            clef: Clef::Treble,
        }
    }
}

pub struct NoteSketch {
    pub kind: NoteKind,
    pub effects: NoteEffects,
}

pub struct BeatSketch {
    pub duration: Duration,
    pub notes: Vec<NoteSketch>,
    pub effects: BeatEffects,
}

pub struct VoiceSketch {
    pub slot: u8,
    pub beats: Vec<BeatSketch>,
}

pub fn duration(value: NoteValue) -> Result<Duration, FixtureError> {
    Ok(Duration::new(value, 0, None)?)
}

pub fn dotted(value: NoteValue, dots: u8) -> Result<Duration, FixtureError> {
    Ok(Duration::new(value, dots, None)?)
}

pub fn tuplet(value: NoteValue, num: u8, den: u8) -> Result<Duration, FixtureError> {
    Ok(Duration::new(value, 0, Some(Tuplet::new(num, den)?))?)
}

pub fn fretted(string: u8, fret: u8) -> NoteSketch {
    NoteSketch {
        kind: NoteKind::Fretted { string, fret },
        effects: NoteEffects::default(),
    }
}

pub fn affected(string: u8, fret: u8, effects: NoteEffects) -> NoteSketch {
    NoteSketch {
        kind: NoteKind::Fretted { string, fret },
        effects,
    }
}

pub fn beat(duration: Duration, notes: Vec<NoteSketch>) -> BeatSketch {
    BeatSketch {
        duration,
        notes,
        effects: BeatEffects::default(),
    }
}

pub fn rest(duration: Duration) -> BeatSketch {
    beat(duration, Vec::new())
}

pub fn with_effects(
    duration: Duration,
    notes: Vec<NoteSketch>,
    effects: BeatEffects,
) -> BeatSketch {
    BeatSketch {
        duration,
        notes,
        effects,
    }
}

pub fn voice(slot: u8, beats: Vec<BeatSketch>) -> VoiceSketch {
    VoiceSketch { slot, beats }
}

fn value_of(denominator: u8) -> Result<NoteValue, FixtureError> {
    match denominator {
        1 => Ok(NoteValue::Whole),
        2 => Ok(NoteValue::Half),
        4 => Ok(NoteValue::Quarter),
        8 => Ok(NoteValue::Eighth),
        16 => Ok(NoteValue::Sixteenth),
        32 => Ok(NoteValue::ThirtySecond),
        64 => Ok(NoteValue::SixtyFourth),
        _ => Err(FixtureError::new(format!(
            "no rest fill for denominator {denominator}"
        ))),
    }
}

pub fn rests_for(time: TimeSignature) -> Result<Vec<BeatSketch>, FixtureError> {
    if time.numerator() == time.denominator() {
        return Ok(vec![rest(duration(NoteValue::Whole)?)]);
    }
    let value = duration(value_of(time.denominator())?)?;
    Ok((0..time.numerator()).map(|_| rest(value)).collect())
}

pub struct Sketch {
    pub score: Score,
}

impl Sketch {
    pub fn new(title: &str) -> Sketch {
        let mut score = Score::new();
        score.meta.title = title.to_string();
        Sketch { score }
    }

    pub fn id<T: From<ElementId>>(&mut self) -> Result<T, FixtureError> {
        self.score
            .alloc()
            .ok_or_else(|| FixtureError::new("id space exhausted".to_string()))
    }

    pub fn master_bar(&mut self, numerator: u8, denominator: u8) -> Result<usize, FixtureError> {
        let id = self.id()?;
        self.score.master_bars.push(MasterBar {
            id,
            time_signature: TimeSignature::new(numerator, denominator)?,
            key_signature: KeySignature::default(),
            repeat_start: false,
            repeat_end: None,
            alternate_endings: 0,
            double_bar: false,
            marker: None,
            tempo_changes: Vec::new(),
        });
        Ok(self.score.master_bars.len() - 1)
    }

    pub fn master_bars(
        &mut self,
        count: usize,
        numerator: u8,
        denominator: u8,
    ) -> Result<(), FixtureError> {
        for _ in 0..count {
            self.master_bar(numerator, denominator)?;
        }
        Ok(())
    }

    pub fn track(&mut self, setup: TrackSetup) -> Result<usize, FixtureError> {
        let id = self.id()?;
        let times: Vec<TimeSignature> = self
            .score
            .master_bars
            .iter()
            .map(|master_bar| master_bar.time_signature)
            .collect();
        let mut staves = Vec::new();
        for _ in 0..setup.staves {
            let staff_id = self.id()?;
            let mut bars = Vec::new();
            for time in &times {
                let bar_id = self.id()?;
                let voices = vec![self.voice(voice(0, rests_for(*time)?))?];
                bars.push(Bar {
                    id: bar_id,
                    clef: setup.clef,
                    voices,
                });
            }
            staves.push(Staff { id: staff_id, bars });
        }
        self.score.tracks.push(Track {
            id,
            name: setup.name.to_string(),
            short_name: setup.short_name.to_string(),
            instrument: Instrument {
                kind: setup.kind,
                midi_program: setup.program,
            },
            tuning: Tuning {
                strings: setup.tuning.into_iter().map(Pitch).collect(),
            },
            capo: setup.capo,
            transpose: Transpose::new(setup.chromatic, setup.octave)?,
            staves,
        });
        Ok(self.score.tracks.len() - 1)
    }

    fn note(&mut self, sketch: NoteSketch) -> Result<Note, FixtureError> {
        Ok(Note {
            id: self.id()?,
            kind: sketch.kind,
            effects: sketch.effects,
        })
    }

    fn beat(&mut self, sketch: BeatSketch) -> Result<Beat, FixtureError> {
        let id = self.id()?;
        let mut notes = Vec::new();
        for note in sketch.notes {
            notes.push(self.note(note)?);
        }
        Ok(Beat {
            id,
            duration: sketch.duration,
            notes,
            effects: sketch.effects,
        })
    }

    fn voice(&mut self, sketch: VoiceSketch) -> Result<Voice, FixtureError> {
        let id = self.id()?;
        let mut beats = Vec::new();
        for beat in sketch.beats {
            beats.push(self.beat(beat)?);
        }
        Ok(Voice {
            id,
            slot: VoiceSlot::new(sketch.slot)?,
            beats,
        })
    }

    pub fn fill(
        &mut self,
        track: usize,
        bar: usize,
        voices: Vec<VoiceSketch>,
    ) -> Result<(), FixtureError> {
        self.fill_staff(track, 0, bar, voices)
    }

    pub fn fill_staff(
        &mut self,
        track: usize,
        staff: usize,
        bar: usize,
        voices: Vec<VoiceSketch>,
    ) -> Result<(), FixtureError> {
        let mut built = Vec::new();
        for sketch in voices {
            built.push(self.voice(sketch)?);
        }
        let target = self
            .score
            .tracks
            .get_mut(track)
            .and_then(|track| track.staves.get_mut(staff))
            .and_then(|staff| staff.bars.get_mut(bar))
            .ok_or_else(|| {
                FixtureError::new(format!("no bar {bar} in staff {staff} of track {track}"))
            })?;
        target.voices = built;
        Ok(())
    }

    pub fn clef(&mut self, track: usize, bar: usize, clef: Clef) -> Result<(), FixtureError> {
        let target = self
            .score
            .tracks
            .get_mut(track)
            .and_then(|track| track.staves.get_mut(0))
            .and_then(|staff| staff.bars.get_mut(bar))
            .ok_or_else(|| FixtureError::new(format!("no bar {bar} in track {track}")))?;
        target.clef = clef;
        Ok(())
    }

    pub fn finish(self) -> Score {
        self.score
    }
}
