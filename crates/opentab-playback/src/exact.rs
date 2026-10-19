use std::cmp::Ordering;

use opentab_model::Fraction;

use crate::PlaybackError;

const GCD_STEP_LIMIT: u32 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Exact {
    numer: i128,
    denom: i128,
}

fn gcd(mut a: i128, mut b: i128) -> Result<i128, PlaybackError> {
    for _ in 0..GCD_STEP_LIMIT {
        if b == 0 {
            return a.checked_abs().ok_or(PlaybackError::TimeOverflow);
        }
        let rest = a.checked_rem(b).ok_or(PlaybackError::TimeOverflow)?;
        a = b;
        b = rest;
    }
    Err(PlaybackError::TimeOverflow)
}

fn overflow<T>(value: Option<T>) -> Result<T, PlaybackError> {
    value.ok_or(PlaybackError::TimeOverflow)
}

impl Exact {
    pub(crate) const ZERO: Exact = Exact { numer: 0, denom: 1 };

    pub(crate) fn new(numer: i128, denom: i128) -> Result<Exact, PlaybackError> {
        if denom == 0 {
            return Err(PlaybackError::TimeOverflow);
        }
        let divisor = gcd(numer, denom)?;
        let sign = if denom < 0 { -1 } else { 1 };
        Ok(Exact {
            numer: overflow(overflow(numer.checked_div(divisor))?.checked_mul(sign))?,
            denom: overflow(overflow(denom.checked_div(divisor))?.checked_mul(sign))?,
        })
    }

    pub(crate) fn integer(value: i128) -> Exact {
        Exact {
            numer: value,
            denom: 1,
        }
    }

    pub(crate) fn of(value: Fraction) -> Exact {
        Exact {
            numer: i128::from(*value.numer()),
            denom: i128::from(*value.denom()),
        }
    }

    pub(crate) fn numer(self) -> i128 {
        self.numer
    }

    pub(crate) fn denom(self) -> i128 {
        self.denom
    }

    pub(crate) fn fraction(self) -> Result<Fraction, PlaybackError> {
        let numer = overflow(i64::try_from(self.numer).ok())?;
        let denom = overflow(i64::try_from(self.denom).ok())?;
        Ok(Fraction::new_raw(numer, denom))
    }

    pub(crate) fn add(self, other: Exact) -> Result<Exact, PlaybackError> {
        let common = gcd(self.denom, other.denom)?;
        let left_scale = overflow(other.denom.checked_div(common))?;
        let right_scale = overflow(self.denom.checked_div(common))?;
        let left = overflow(self.numer.checked_mul(left_scale))?;
        let right = overflow(other.numer.checked_mul(right_scale))?;
        let numer = overflow(left.checked_add(right))?;
        let denom = overflow(self.denom.checked_mul(left_scale))?;
        Exact::new(numer, denom)
    }

    pub(crate) fn sub(self, other: Exact) -> Result<Exact, PlaybackError> {
        self.add(Exact {
            numer: overflow(other.numer.checked_neg())?,
            denom: other.denom,
        })
    }

    pub(crate) fn mul(self, other: Exact) -> Result<Exact, PlaybackError> {
        let first = gcd(self.numer, other.denom)?.max(1);
        let second = gcd(other.numer, self.denom)?.max(1);
        let numer = overflow(
            overflow(self.numer.checked_div(first))?
                .checked_mul(overflow(other.numer.checked_div(second))?),
        )?;
        let denom = overflow(
            overflow(self.denom.checked_div(second))?
                .checked_mul(overflow(other.denom.checked_div(first))?),
        )?;
        Exact::new(numer, denom)
    }

    pub(crate) fn div(self, other: Exact) -> Result<Exact, PlaybackError> {
        if other.numer == 0 {
            return Err(PlaybackError::TimeOverflow);
        }
        self.mul(Exact {
            numer: other.denom,
            denom: other.numer,
        })
    }

    pub(crate) fn floor(self) -> i128 {
        self.numer.div_euclid(self.denom)
    }

    pub(crate) fn is_negative(self) -> bool {
        self.numer < 0
    }
}

impl PartialOrd for Exact {
    fn partial_cmp(&self, other: &Exact) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Exact {
    fn cmp(&self, other: &Exact) -> Ordering {
        let (mut a, mut b, mut c, mut d) = (self.numer, self.denom, other.numer, other.denom);
        let mut flipped = false;
        loop {
            let left = a.div_euclid(b);
            let right = c.div_euclid(d);
            let order = left.cmp(&right);
            if order != Ordering::Equal {
                return if flipped { order.reverse() } else { order };
            }
            let left_rest = a.rem_euclid(b);
            let right_rest = c.rem_euclid(d);
            let order = match (left_rest == 0, right_rest == 0) {
                (true, true) => return Ordering::Equal,
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                (false, false) => {
                    (a, b, c, d) = (b, left_rest, d, right_rest);
                    flipped = !flipped;
                    continue;
                }
            };
            return if flipped { order.reverse() } else { order };
        }
    }
}
