use serde::{Deserialize, Serialize};

use crate::fraction::deserialize_non_negative;
use crate::{Fraction, ModelError, NoteValue};

const MAX_BEND_OFFSET: u8 = 100;
const MAX_BEND_VALUE: i16 = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrokeDirection {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stroke {
    pub direction: StrokeDirection,
    pub duration: NoteValue,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BeatEffects {
    pub tremolo_picking: Option<NoteValue>,
    pub stroke: Option<Stroke>,
    pub chord_name: Option<String>,
    pub text: Option<String>,
    pub fade_in: bool,
    pub tap: bool,
    pub slap: bool,
    pub pop: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Accent {
    Normal,
    Heavy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Vibrato {
    Slight,
    Wide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Slide {
    Shift,
    Legato,
    IntoFromBelow,
    IntoFromAbove,
    OutDownwards,
    OutUpwards,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarmonicKind {
    Natural,
    Artificial,
    Pinch,
    Tap,
    Semi,
    Feedback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Harmonic {
    pub kind: HarmonicKind,
    #[serde(deserialize_with = "deserialize_non_negative")]
    pub fret: Fraction,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "BendPointRepr")]
pub struct BendPoint {
    offset: u8,
    value: i16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BendPointRepr {
    offset: u8,
    value: i16,
}

impl BendPoint {
    pub fn new(offset: u8, value: i16) -> Result<BendPoint, ModelError> {
        if offset > MAX_BEND_OFFSET || !(-MAX_BEND_VALUE..=MAX_BEND_VALUE).contains(&value) {
            return Err(ModelError::InvalidBendPoint { offset, value });
        }
        Ok(BendPoint { offset, value })
    }

    pub fn offset(&self) -> u8 {
        self.offset
    }

    pub fn value(&self) -> i16 {
        self.value
    }
}

impl TryFrom<BendPointRepr> for BendPoint {
    type Error = ModelError;

    fn try_from(repr: BendPointRepr) -> Result<Self, Self::Error> {
        BendPoint::new(repr.offset, repr.value)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bend {
    pub points: Vec<BendPoint>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoteEffects {
    pub tie_origin: bool,
    pub tie_destination: bool,
    pub ghost: bool,
    pub dead: bool,
    pub accent: Option<Accent>,
    pub palm_mute: bool,
    pub let_ring: bool,
    pub staccato: bool,
    pub vibrato: Option<Vibrato>,
    pub hammer_on_pull_off: bool,
    pub slide: Option<Slide>,
    pub harmonic: Option<Harmonic>,
    pub bend: Option<Bend>,
    pub left_hand_finger: Option<u8>,
    pub right_hand_finger: Option<u8>,
}
