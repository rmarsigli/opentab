use serde::{Deserialize, Serialize};

use crate::fraction::deserialize_non_negative;
use crate::{Fraction, ModelError, NoteValue};

const MAX_BEND_OFFSET: u8 = 100;
const MAX_BEND_VALUE: i16 = 1200;
const MIN_STROKE_TICKS: u16 = 30;
const MAX_STROKE_TICKS: u16 = 480;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrokeDirection {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub struct StrokeTicks(u16);

impl StrokeTicks {
    pub fn new(ticks: u16) -> Result<StrokeTicks, ModelError> {
        if (MIN_STROKE_TICKS..=MAX_STROKE_TICKS).contains(&ticks) {
            Ok(StrokeTicks(ticks))
        } else {
            Err(ModelError::InvalidStrokeTicks { ticks })
        }
    }

    pub fn ticks(self) -> u16 {
        self.0
    }
}

impl Default for StrokeTicks {
    fn default() -> Self {
        StrokeTicks(MAX_STROKE_TICKS)
    }
}

impl TryFrom<u16> for StrokeTicks {
    type Error = ModelError;

    fn try_from(ticks: u16) -> Result<Self, Self::Error> {
        StrokeTicks::new(ticks)
    }
}

impl From<StrokeTicks> for u16 {
    fn from(ticks: StrokeTicks) -> Self {
        ticks.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stroke {
    pub direction: StrokeDirection,
    pub duration: StrokeTicks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "NoteValue", into = "NoteValue")]
pub struct TremoloPicking(NoteValue);

impl TremoloPicking {
    pub fn new(value: NoteValue) -> Result<TremoloPicking, ModelError> {
        match value {
            NoteValue::Eighth | NoteValue::Sixteenth | NoteValue::ThirtySecond => {
                Ok(TremoloPicking(value))
            }
            _ => Err(ModelError::InvalidTremolo { value }),
        }
    }

    pub fn value(self) -> NoteValue {
        self.0
    }
}

impl TryFrom<NoteValue> for TremoloPicking {
    type Error = ModelError;

    fn try_from(value: NoteValue) -> Result<Self, Self::Error> {
        TremoloPicking::new(value)
    }
}

impl From<TremoloPicking> for NoteValue {
    fn from(tremolo: TremoloPicking) -> Self {
        tremolo.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Grace {
    BeforeBeat,
    OnBeat,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dynamic {
    Ppp,
    Pp,
    P,
    Mp,
    #[default]
    Mf,
    F,
    Ff,
    Fff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "TremoloBarRepr")]
pub struct TremoloBar {
    origin: BendPoint,
    middle_start: BendPoint,
    middle_end: BendPoint,
    destination: BendPoint,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TremoloBarRepr {
    origin: BendPoint,
    middle_start: BendPoint,
    middle_end: BendPoint,
    destination: BendPoint,
}

impl TremoloBar {
    pub fn new(
        origin: BendPoint,
        middle_start: BendPoint,
        middle_end: BendPoint,
        destination: BendPoint,
    ) -> Result<TremoloBar, ModelError> {
        if middle_start.value() != middle_end.value() {
            return Err(ModelError::InvalidTremoloBar {
                start: middle_start.value(),
                end: middle_end.value(),
            });
        }
        Ok(TremoloBar {
            origin,
            middle_start,
            middle_end,
            destination,
        })
    }

    pub fn origin(&self) -> BendPoint {
        self.origin
    }

    pub fn middle_start(&self) -> BendPoint {
        self.middle_start
    }

    pub fn middle_end(&self) -> BendPoint {
        self.middle_end
    }

    pub fn destination(&self) -> BendPoint {
        self.destination
    }

    pub fn middle_value(&self) -> i16 {
        self.middle_start.value()
    }

    pub fn moves(&self) -> bool {
        [self.origin, self.middle_start, self.destination]
            .iter()
            .any(|point| point.value() != 0)
    }
}

impl TryFrom<TremoloBarRepr> for TremoloBar {
    type Error = ModelError;

    fn try_from(repr: TremoloBarRepr) -> Result<Self, Self::Error> {
        TremoloBar::new(
            repr.origin,
            repr.middle_start,
            repr.middle_end,
            repr.destination,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "NoteValue", into = "NoteValue")]
pub struct TrillSpeed(NoteValue);

impl TrillSpeed {
    pub const THIRTY_SECOND: TrillSpeed = TrillSpeed(NoteValue::ThirtySecond);

    pub fn new(value: NoteValue) -> Result<TrillSpeed, ModelError> {
        match value {
            NoteValue::ThirtySecond
            | NoteValue::Sixteenth
            | NoteValue::Eighth
            | NoteValue::Quarter => Ok(TrillSpeed(value)),
            _ => Err(ModelError::InvalidTrillSpeed { value }),
        }
    }

    pub fn value(self) -> NoteValue {
        self.0
    }
}

impl Default for TrillSpeed {
    fn default() -> Self {
        TrillSpeed::THIRTY_SECOND
    }
}

impl TryFrom<NoteValue> for TrillSpeed {
    type Error = ModelError;

    fn try_from(value: NoteValue) -> Result<Self, Self::Error> {
        TrillSpeed::new(value)
    }
}

impl From<TrillSpeed> for NoteValue {
    fn from(speed: TrillSpeed) -> Self {
        speed.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trill {
    pub fret: u8,
    pub speed: TrillSpeed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BeatEffects {
    pub tremolo_picking: Option<TremoloPicking>,
    pub tremolo_bar: Option<TremoloBar>,
    pub stroke: Option<Stroke>,
    pub chord_name: Option<String>,
    pub text: Option<String>,
    pub fade_in: bool,
    pub slap: bool,
    pub pop: bool,
    pub grace: Option<Grace>,
    pub dynamic: Dynamic,
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
pub enum Tap {
    Right,
    Left,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Finger {
    Open,
    Thumb,
    Index,
    Middle,
    Ring,
    Little,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "BendRepr")]
pub struct Bend {
    origin: BendPoint,
    middle_start: BendPoint,
    middle_end: BendPoint,
    destination: BendPoint,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BendRepr {
    origin: BendPoint,
    middle_start: BendPoint,
    middle_end: BendPoint,
    destination: BendPoint,
}

impl Bend {
    pub fn new(
        origin: BendPoint,
        middle_start: BendPoint,
        middle_end: BendPoint,
        destination: BendPoint,
    ) -> Result<Bend, ModelError> {
        if middle_start.value() != middle_end.value() {
            return Err(ModelError::InvalidBend {
                start: middle_start.value(),
                end: middle_end.value(),
            });
        }
        Ok(Bend {
            origin,
            middle_start,
            middle_end,
            destination,
        })
    }

    pub fn origin(&self) -> BendPoint {
        self.origin
    }

    pub fn middle_start(&self) -> BendPoint {
        self.middle_start
    }

    pub fn middle_end(&self) -> BendPoint {
        self.middle_end
    }

    pub fn destination(&self) -> BendPoint {
        self.destination
    }

    pub fn middle_value(&self) -> i16 {
        self.middle_start.value()
    }
}

impl TryFrom<BendRepr> for Bend {
    type Error = ModelError;

    fn try_from(repr: BendRepr) -> Result<Self, Self::Error> {
        Bend::new(
            repr.origin,
            repr.middle_start,
            repr.middle_end,
            repr.destination,
        )
    }
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
    pub tap: Option<Tap>,
    pub left_hand_finger: Option<Finger>,
    pub right_hand_finger: Option<Finger>,
    pub trill: Option<Trill>,
}
