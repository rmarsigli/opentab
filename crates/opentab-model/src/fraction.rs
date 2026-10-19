use serde::de::Error;
use serde::{Deserialize, Deserializer};

pub use num_rational::Rational64 as Fraction;

pub type WholeNotes = Fraction;

pub(crate) fn deserialize_non_negative<'de, D>(deserializer: D) -> Result<Fraction, D::Error>
where
    D: Deserializer<'de>,
{
    let (numer, denom): (i64, i64) = Deserialize::deserialize(deserializer)?;
    if numer < 0 || denom <= 0 {
        return Err(D::Error::custom(format!(
            "fraction {numer}/{denom} must be non-negative with a positive denominator"
        )));
    }
    Ok(Fraction::new(numer, denom))
}

#[derive(Debug, Clone, Copy)]
struct Exact {
    numer: i128,
    denom: i128,
}

impl Exact {
    fn checked_add(self, other: Fraction) -> Option<Exact> {
        let numer = i128::from(*other.numer());
        let denom = i128::from(*other.denom());
        let common = gcd(self.denom, denom)?;
        let left = self.numer.checked_mul(denom.checked_div(common)?)?;
        let right = numer.checked_mul(self.denom.checked_div(common)?)?;
        let sum = left.checked_add(right)?;
        let scaled = self.denom.checked_div(common)?.checked_mul(denom)?;
        let divisor = gcd(sum.checked_abs()?, scaled)?;
        Some(Exact {
            numer: sum.checked_div(divisor)?,
            denom: scaled.checked_div(divisor)?,
        })
    }

    fn to_fraction(self) -> Option<Fraction> {
        let numer = i64::try_from(self.numer).ok()?;
        let denom = i64::try_from(self.denom).ok()?;
        if denom <= 0 {
            return None;
        }
        Some(Fraction::new(numer, denom))
    }
}

const GCD_STEP_LIMIT: u32 = 256;

fn gcd(mut a: i128, mut b: i128) -> Option<i128> {
    for _ in 0..GCD_STEP_LIMIT {
        if b == 0 {
            return a.checked_abs();
        }
        let rest = a.checked_rem(b)?;
        a = b;
        b = rest;
    }
    None
}

pub(crate) fn checked_sum<I>(items: I) -> Option<Fraction>
where
    I: IntoIterator<Item = Fraction>,
{
    let mut total = Exact { numer: 0, denom: 1 };
    for item in items {
        total = total.checked_add(item)?;
    }
    total.to_fraction()
}
