use serde::{Deserialize, Serialize};

use crate::fraction::deserialize_non_negative;
use crate::{Fraction, MasterBarId, ModelError};

const MIN_BPM: f64 = 1.0;
const MAX_BPM: f64 = 1000.0;
const MAX_ACCIDENTALS: i8 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "TimeSignatureRepr")]
pub struct TimeSignature {
    pub numerator: u8,
    pub denominator: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TimeSignatureRepr {
    numerator: u8,
    denominator: u8,
}

impl TimeSignature {
    pub fn new(numerator: u8, denominator: u8) -> Result<TimeSignature, ModelError> {
        if numerator == 0 || denominator == 0 {
            return Err(ModelError::InvalidTimeSignature {
                numerator,
                denominator,
            });
        }
        Ok(TimeSignature {
            numerator,
            denominator,
        })
    }

    pub fn bar_length(&self) -> Fraction {
        if self.denominator == 0 {
            return Fraction::from_integer(0);
        }
        Fraction::new(i64::from(self.numerator), i64::from(self.denominator))
    }
}

impl TryFrom<TimeSignatureRepr> for TimeSignature {
    type Error = ModelError;

    fn try_from(repr: TimeSignatureRepr) -> Result<Self, Self::Error> {
        TimeSignature::new(repr.numerator, repr.denominator)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "KeySignatureRepr")]
pub struct KeySignature {
    pub accidentals: i8,
    pub minor: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeySignatureRepr {
    accidentals: i8,
    minor: bool,
}

impl KeySignature {
    pub fn new(accidentals: i8, minor: bool) -> Result<KeySignature, ModelError> {
        if !(-MAX_ACCIDENTALS..=MAX_ACCIDENTALS).contains(&accidentals) {
            return Err(ModelError::InvalidKeySignature { accidentals });
        }
        Ok(KeySignature { accidentals, minor })
    }
}

impl TryFrom<KeySignatureRepr> for KeySignature {
    type Error = ModelError;

    fn try_from(repr: KeySignatureRepr) -> Result<Self, Self::Error> {
        KeySignature::new(repr.accidentals, repr.minor)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Marker {
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Tempo(f64);

impl Tempo {
    pub fn new(bpm: f64) -> Result<Tempo, ModelError> {
        if (MIN_BPM..=MAX_BPM).contains(&bpm) {
            Ok(Tempo(bpm))
        } else {
            Err(ModelError::InvalidTempo { bpm })
        }
    }

    pub fn bpm(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Tempo {
    type Error = ModelError;

    fn try_from(bpm: f64) -> Result<Self, Self::Error> {
        Tempo::new(bpm)
    }
}

impl From<Tempo> for f64 {
    fn from(tempo: Tempo) -> Self {
        tempo.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TempoChange {
    #[serde(deserialize_with = "deserialize_non_negative")]
    pub position: Fraction,
    pub bpm: Tempo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MasterBar {
    pub id: MasterBarId,
    pub time_signature: TimeSignature,
    pub key_signature: KeySignature,
    pub repeat_start: bool,
    pub repeat_end: Option<u8>,
    pub alternate_endings: u8,
    pub marker: Option<Marker>,
    pub tempo_changes: Vec<TempoChange>,
}
