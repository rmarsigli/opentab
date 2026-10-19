use std::str::FromStr;

use opentab_model::Fraction;

const MAX_SCALE: u32 = 30;
const MAX_EXPONENT: i32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Decimal {
    mantissa: i128,
    scale: u32,
}

impl Decimal {
    pub fn parse(text: &str) -> Option<Decimal> {
        let text = text.trim();
        let (body, exponent) = match text.find(['e', 'E']) {
            Some(at) => (&text[..at], text[at + 1..].parse::<i32>().ok()?),
            None => (text, 0),
        };
        if !(-MAX_EXPONENT..=MAX_EXPONENT).contains(&exponent) {
            return None;
        }
        let (negative, digits) = match body.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, body.strip_prefix('+').unwrap_or(body)),
        };
        let (whole, fraction) = match digits.split_once('.') {
            Some((whole, fraction)) => (whole, fraction),
            None => (digits, ""),
        };
        if whole.is_empty() && fraction.is_empty() {
            return None;
        }
        if !whole
            .bytes()
            .chain(fraction.bytes())
            .all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let mut mantissa: i128 = 0;
        for byte in whole.bytes().chain(fraction.bytes()) {
            mantissa = mantissa
                .checked_mul(10)?
                .checked_add(i128::from(byte - b'0'))?;
        }
        let mut scale = i32::try_from(fraction.len()).ok()?.checked_sub(exponent)?;
        while scale < 0 {
            mantissa = mantissa.checked_mul(10)?;
            scale += 1;
        }
        let scale = u32::try_from(scale).ok()?;
        if scale > MAX_SCALE {
            return None;
        }
        if negative {
            mantissa = -mantissa;
        }
        Some(Decimal { mantissa, scale })
    }

    fn denominator(self) -> i128 {
        10i128.pow(self.scale)
    }

    pub fn is_negative(self) -> bool {
        self.mantissa < 0
    }

    pub fn to_fraction(self) -> Option<Fraction> {
        let (numer, denom) = reduce(self.mantissa, self.denominator())?;
        Some(Fraction::new(
            i64::try_from(numer).ok()?,
            i64::try_from(denom).ok()?,
        ))
    }

    pub fn to_integer(self) -> Option<i64> {
        let denominator = self.denominator();
        if self.mantissa % denominator != 0 {
            return None;
        }
        i64::try_from(self.mantissa / denominator).ok()
    }

    pub fn scaled_integer(self, factor: i128) -> Option<i64> {
        let scaled = self.mantissa.checked_mul(factor)?;
        let denominator = self.denominator();
        if scaled % denominator != 0 {
            return None;
        }
        i64::try_from(scaled / denominator).ok()
    }

    pub fn round_to_grid(self, numer: i64, denom: i64, grid: i64) -> Option<Fraction> {
        if self.mantissa < 0 || numer < 0 || denom <= 0 || grid <= 0 {
            return None;
        }
        let top = self
            .mantissa
            .checked_mul(i128::from(numer))?
            .checked_mul(i128::from(grid))?;
        let bottom = self.denominator().checked_mul(i128::from(denom))?;
        let ticks = top
            .checked_mul(2)?
            .checked_add(bottom)?
            .checked_div(bottom.checked_mul(2)?)?;
        Some(Fraction::new(i64::try_from(ticks).ok()?, grid))
    }
}

fn reduce(numer: i128, denom: i128) -> Option<(i128, i128)> {
    let mut a = numer.checked_abs()?;
    let mut b = denom;
    while b != 0 {
        let rest = a % b;
        a = b;
        b = rest;
    }
    if a == 0 {
        return Some((0, 1));
    }
    Some((numer / a, denom / a))
}

const MILLIONTHS: i128 = 1_000_000;

pub(crate) fn millionths(text: &str) -> Option<u32> {
    let decimal = Decimal::parse(text)?;
    if decimal.is_negative() {
        return None;
    }
    let value = u32::try_from(decimal.scaled_integer(MILLIONTHS)?).ok()?;
    (i128::from(value) <= MILLIONTHS).then_some(value)
}

pub(crate) fn integer<T: FromStr>(text: &str) -> Option<T> {
    text.trim().parse().ok()
}

pub(crate) fn boolean(text: &str) -> Option<bool> {
    match text.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}
