use opentab_model::Fraction;

use crate::PlaybackError;
use crate::exact::Exact;

pub const MIN_SPEED: u16 = 25;
pub const MAX_SPEED: u16 = 200;
pub(crate) const PERCENT: u16 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScoreTime(Fraction);

impl ScoreTime {
    pub const ZERO: ScoreTime = ScoreTime(Fraction::new_raw(0, 1));

    pub fn new(whole_notes: Fraction) -> Result<ScoreTime, PlaybackError> {
        if whole_notes < Fraction::new_raw(0, 1) {
            return Err(PlaybackError::NegativeTime);
        }
        Ok(ScoreTime(whole_notes))
    }

    pub fn whole_notes(self) -> Fraction {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Speed(u16);

impl Speed {
    pub const NORMAL: Speed = Speed(PERCENT);

    pub fn new(percent: u16) -> Result<Speed, PlaybackError> {
        if (MIN_SPEED..=MAX_SPEED).contains(&percent) {
            Ok(Speed(percent))
        } else {
            Err(PlaybackError::InvalidSpeed { percent })
        }
    }

    pub fn percent(self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Seconds(Exact);

impl Seconds {
    pub const ZERO: Seconds = Seconds(Exact::ZERO);

    pub fn new(numer: i128, denom: i128) -> Result<Seconds, PlaybackError> {
        Exact::new(numer, denom).map(Seconds)
    }

    pub(crate) fn exact(self) -> Exact {
        self.0
    }

    pub(crate) fn of(exact: Exact) -> Seconds {
        Seconds(exact)
    }

    pub fn numer(self) -> i128 {
        self.0.numer()
    }

    pub fn denom(self) -> i128 {
        self.0.denom()
    }

    pub fn fraction(self) -> Result<Fraction, PlaybackError> {
        self.0.fraction()
    }

    pub fn floor_scaled(self, scale: i128) -> Result<i128, PlaybackError> {
        Ok(self.0.mul(Exact::integer(scale))?.floor())
    }
}

impl From<Fraction> for Seconds {
    fn from(value: Fraction) -> Seconds {
        Seconds(Exact::of(value))
    }
}
