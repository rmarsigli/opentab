use serde::{Deserialize, Serialize};

use crate::{
    BarId, BeatEffects, BeatId, Duration, ModelError, NoteEffects, NoteId, StaffId, TrackId,
    VoiceId,
};

const MAX_MIDI_NOTE: u8 = 127;
const MAX_VOICE_SLOT: u8 = 3;
const MAX_TRANSPOSE_CHROMATIC: i8 = 12;
const MAX_TRANSPOSE_OCTAVE: i8 = 3;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Clef {
    #[default]
    Treble,
    Bass,
    Neutral,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(try_from = "u8", into = "u8")]
pub struct VoiceSlot(u8);

impl VoiceSlot {
    pub fn new(slot: u8) -> Result<VoiceSlot, ModelError> {
        if slot > MAX_VOICE_SLOT {
            return Err(ModelError::InvalidVoiceSlot { slot });
        }
        Ok(VoiceSlot(slot))
    }

    pub fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for VoiceSlot {
    type Error = ModelError;

    fn try_from(slot: u8) -> Result<Self, Self::Error> {
        VoiceSlot::new(slot)
    }
}

impl From<VoiceSlot> for u8 {
    fn from(slot: VoiceSlot) -> Self {
        slot.0
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "TransposeRepr")]
pub struct Transpose {
    chromatic: i8,
    octave: i8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TransposeRepr {
    chromatic: i8,
    octave: i8,
}

impl Transpose {
    pub fn new(chromatic: i8, octave: i8) -> Result<Transpose, ModelError> {
        if !(-MAX_TRANSPOSE_CHROMATIC..=MAX_TRANSPOSE_CHROMATIC).contains(&chromatic)
            || !(-MAX_TRANSPOSE_OCTAVE..=MAX_TRANSPOSE_OCTAVE).contains(&octave)
        {
            return Err(ModelError::InvalidTranspose { chromatic, octave });
        }
        Ok(Transpose { chromatic, octave })
    }

    pub fn chromatic(&self) -> i8 {
        self.chromatic
    }

    pub fn octave(&self) -> i8 {
        self.octave
    }
}

impl TryFrom<TransposeRepr> for Transpose {
    type Error = ModelError;

    fn try_from(repr: TransposeRepr) -> Result<Self, Self::Error> {
        Transpose::new(repr.chromatic, repr.octave)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Pitch(pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstrumentKind {
    Fretted,
    Percussion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instrument {
    pub kind: InstrumentKind,
    pub midi_program: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tuning {
    pub strings: Vec<Pitch>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum NoteKind {
    Fretted { string: u8, fret: u8 },
    Percussion { articulation: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Note {
    pub id: NoteId,
    pub kind: NoteKind,
    pub effects: NoteEffects,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Beat {
    pub id: BeatId,
    pub duration: Duration,
    pub notes: Vec<Note>,
    pub effects: BeatEffects,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Voice {
    pub id: VoiceId,
    pub slot: VoiceSlot,
    pub beats: Vec<Beat>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bar {
    pub id: BarId,
    pub clef: Clef,
    pub voices: Vec<Voice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Staff {
    pub id: StaffId,
    pub bars: Vec<Bar>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Track {
    pub id: TrackId,
    pub name: String,
    pub short_name: String,
    pub instrument: Instrument,
    pub tuning: Tuning,
    pub capo: u8,
    pub transpose: Transpose,
    pub staves: Vec<Staff>,
}

impl Track {
    pub fn pitch_of(&self, note: &Note) -> Option<Pitch> {
        let NoteKind::Fretted { string, fret } = note.kind else {
            return None;
        };
        let open = self.tuning.strings.get(usize::from(string))?;
        let midi = u16::from(open.0) + u16::from(self.capo) + u16::from(fret);
        u8::try_from(midi)
            .ok()
            .filter(|value| *value <= MAX_MIDI_NOTE)
            .map(Pitch)
    }
}
