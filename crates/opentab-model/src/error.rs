use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelError {
    #[error("tempo {bpm} is not a finite value within 1..=1000 bpm")]
    InvalidTempo { bpm: f64 },
    #[error("{dots} dots exceed the maximum of 3")]
    InvalidDots { dots: u8 },
    #[error("tuplet {num}:{den} has a term outside 1..=64")]
    InvalidTuplet { num: u8, den: u8 },
    #[error("time signature {numerator}/{denominator} has a zero term")]
    InvalidTimeSignature { numerator: u8, denominator: u8 },
    #[error(
        "bend point offset {offset} value {value} is outside offset 0..=100 and value -48..=48"
    )]
    InvalidBendPoint { offset: u8, value: i16 },
    #[error("key signature with {accidentals} accidentals is outside -7..=7")]
    InvalidKeySignature { accidentals: i8 },
    #[error("missing \"opentab_json\" version field")]
    MissingVersion,
    #[error("unsupported opentab_json version {found}, expected 1")]
    UnsupportedVersion { found: String },
    #[error("invalid opentab JSON: {message}")]
    Json { message: String },
}
