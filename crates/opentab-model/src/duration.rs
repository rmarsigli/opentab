use serde::{Deserialize, Serialize};

use crate::{Fraction, ModelError};

const MAX_DOTS: u8 = 3;
const MAX_TUPLET_TERM: u8 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteValue {
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
    SixtyFourth,
    OneTwentyEighth,
}

impl NoteValue {
    pub fn denominator(self) -> i64 {
        match self {
            NoteValue::Whole => 1,
            NoteValue::Half => 2,
            NoteValue::Quarter => 4,
            NoteValue::Eighth => 8,
            NoteValue::Sixteenth => 16,
            NoteValue::ThirtySecond => 32,
            NoteValue::SixtyFourth => 64,
            NoteValue::OneTwentyEighth => 128,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tuplet {
    pub num: u8,
    pub den: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "DurationRepr")]
pub struct Duration {
    value: NoteValue,
    dots: u8,
    tuplet: Option<Tuplet>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DurationRepr {
    value: NoteValue,
    dots: u8,
    tuplet: Option<Tuplet>,
}

impl Duration {
    pub fn new(value: NoteValue, dots: u8, tuplet: Option<Tuplet>) -> Result<Duration, ModelError> {
        if dots > MAX_DOTS {
            return Err(ModelError::InvalidDots { dots });
        }
        if let Some(Tuplet { num, den }) = tuplet {
            let valid = 1..=MAX_TUPLET_TERM;
            if !valid.contains(&num) || !valid.contains(&den) {
                return Err(ModelError::InvalidTuplet { num, den });
            }
        }
        Ok(Duration {
            value,
            dots,
            tuplet,
        })
    }

    pub fn value(&self) -> NoteValue {
        self.value
    }

    pub fn dots(&self) -> u8 {
        self.dots
    }

    pub fn tuplet(&self) -> Option<Tuplet> {
        self.tuplet
    }

    pub fn length(&self) -> Fraction {
        let base = Fraction::new(1, self.value.denominator());
        let mut total = base;
        let mut dot = base;
        for _ in 0..self.dots {
            dot /= 2;
            total += dot;
        }
        match self.tuplet {
            Some(tuplet) => total * Fraction::new(i64::from(tuplet.den), i64::from(tuplet.num)),
            None => total,
        }
    }
}

impl TryFrom<DurationRepr> for Duration {
    type Error = ModelError;

    fn try_from(repr: DurationRepr) -> Result<Self, Self::Error> {
        Duration::new(repr.value, repr.dots, repr.tuplet)
    }
}
