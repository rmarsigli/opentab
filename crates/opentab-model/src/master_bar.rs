use serde::{Deserialize, Serialize};

use crate::fraction::deserialize_non_negative;
use crate::{Directions, Fraction, MasterBarId, ModelError, WholeNotes};

const MIN_BPM: f64 = 1.0;
const MAX_BPM: f64 = 1000.0;
const MAX_ACCIDENTALS: i8 = 7;
const MAX_DENOMINATOR: u8 = 64;
const MAX_TEMPO_DECIMALS: usize = 6;
const DECIMAL_BASE: i128 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "TimeSignatureRepr")]
pub struct TimeSignature {
    numerator: u8,
    denominator: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TimeSignatureRepr {
    numerator: u8,
    denominator: u8,
}

impl TimeSignature {
    pub fn new(numerator: u8, denominator: u8) -> Result<TimeSignature, ModelError> {
        if numerator == 0
            || !(1..=MAX_DENOMINATOR).contains(&denominator)
            || !denominator.is_power_of_two()
        {
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

    pub fn numerator(&self) -> u8 {
        self.numerator
    }

    pub fn denominator(&self) -> u8 {
        self.denominator
    }

    pub fn bar_length(&self) -> Fraction {
        Fraction::new(i64::from(self.numerator), i64::from(self.denominator))
    }
}

impl TryFrom<TimeSignatureRepr> for TimeSignature {
    type Error = ModelError;

    fn try_from(repr: TimeSignatureRepr) -> Result<Self, Self::Error> {
        TimeSignature::new(repr.numerator, repr.denominator)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "KeySignatureRepr")]
pub struct KeySignature {
    accidentals: i8,
    minor: bool,
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

    pub fn accidentals(&self) -> i8 {
        self.accidentals
    }

    pub fn minor(&self) -> bool {
        self.minor
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
    pub letter: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TempoUnit {
    Eighth,
    #[default]
    Quarter,
    DottedQuarter,
    Half,
    DottedHalf,
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

    pub fn exact(self) -> Option<Fraction> {
        let text = self.0.to_string();
        let (whole, decimals) = text.split_once('.').unwrap_or((text.as_str(), ""));
        if decimals.len() > MAX_TEMPO_DECIMALS || whole.is_empty() {
            return None;
        }
        let mut numer: i128 = 0;
        for byte in whole.bytes().chain(decimals.bytes()) {
            if !byte.is_ascii_digit() {
                return None;
            }
            numer = numer
                .checked_mul(DECIMAL_BASE)?
                .checked_add(i128::from(byte - b'0'))?;
        }
        let places = u32::try_from(decimals.len()).ok()?;
        let denom = DECIMAL_BASE.checked_pow(places)?;
        let exact = Fraction::new(i64::try_from(numer).ok()?, i64::try_from(denom).ok()?);
        (exact > Fraction::from_integer(0)).then_some(exact)
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
    pub position: WholeNotes,
    pub bpm: Tempo,
    pub unit: TempoUnit,
    #[serde(default)]
    pub linear: bool,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum TripletFeel {
    #[default]
    None,
    TripletEighth,
    TripletSixteenth,
    DottedEighth,
    DottedSixteenth,
    ScottishEighth,
    ScottishSixteenth,
}

pub const TRIPLET_FEELS: [TripletFeel; 7] = [
    TripletFeel::None,
    TripletFeel::TripletEighth,
    TripletFeel::TripletSixteenth,
    TripletFeel::DottedEighth,
    TripletFeel::DottedSixteenth,
    TripletFeel::ScottishEighth,
    TripletFeel::ScottishSixteenth,
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MasterBar {
    pub id: MasterBarId,
    pub time_signature: TimeSignature,
    pub key_signature: KeySignature,
    pub repeat_start: bool,
    pub repeat_end: Option<u8>,
    pub alternate_endings: u8,
    #[serde(default)]
    pub triplet_feel: TripletFeel,
    pub double_bar: bool,
    pub marker: Option<Marker>,
    #[serde(default)]
    pub directions: Directions,
    pub tempo_changes: Vec<TempoChange>,
}
