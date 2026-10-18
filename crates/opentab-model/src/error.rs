use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::NoteValue;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelError {
    #[error("tempo {bpm} is not a finite value within 1..=1000 bpm")]
    InvalidTempo { bpm: f64 },
    #[error("{dots} dots exceed the maximum of 3")]
    InvalidDots { dots: u8 },
    #[error("tuplet {num}:{den} has a term outside 1..=64")]
    InvalidTuplet { num: u8, den: u8 },
    #[error(
        "time signature {numerator}/{denominator} needs a non-zero numerator and a power-of-two denominator within 1..=64"
    )]
    InvalidTimeSignature { numerator: u8, denominator: u8 },
    #[error(
        "bend point offset {offset} value {value} is outside offset 0..=100 and value -1200..=1200"
    )]
    InvalidBendPoint { offset: u8, value: i16 },
    #[error("bend middle values {start} and {end} differ; both middle points share one value")]
    InvalidBend { start: i16, end: i16 },
    #[error("stroke duration {ticks} ticks is outside 30..=480")]
    InvalidStrokeTicks { ticks: u16 },
    #[error("tremolo picking {value:?} is not an eighth, sixteenth or thirty-second")]
    InvalidTremolo { value: NoteValue },
    #[error("transpose chromatic {chromatic} octave {octave} is outside -12..=12 and -3..=3")]
    InvalidTranspose { chromatic: i8, octave: i8 },
    #[error("voice slot {slot} is outside 0..=3")]
    InvalidVoiceSlot { slot: u8 },
    #[error("key signature with {accidentals} accidentals is outside -7..=7")]
    InvalidKeySignature { accidentals: i8 },
    #[error("container has {scores} score entries; at most one is allowed")]
    InvalidContainer { scores: usize },
    #[error("the score's id space is exhausted")]
    IdsExhausted,
    #[error("missing \"opentab_json\" version field")]
    MissingVersion,
    #[error("unsupported opentab_json version {found}, expected 2")]
    UnsupportedVersion { found: String },
    #[error("invalid opentab JSON: {message}")]
    Json { message: String },
}
