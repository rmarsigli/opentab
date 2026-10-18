use serde::{Deserialize, Serialize};

use crate::{BarId, BeatEffects, BeatId, Duration, NoteEffects, NoteId, StaffId, TrackId, VoiceId};

const MAX_MIDI_NOTE: u8 = 127;

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
    pub beats: Vec<Beat>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bar {
    pub id: BarId,
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
