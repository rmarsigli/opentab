use std::num::NonZeroI128;

use crate::units::{STAFF_SPACE, Units, round_div};

const TEN: NonZeroI128 = NonZeroI128::new(10).unwrap();

const MAX_DIGITS: usize = 30;
const MAX_EXPONENT_DIGITS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecimalError(pub String);

pub fn staff_spaces_to_units(text: &str) -> Result<Units, DecimalError> {
    let invalid = || DecimalError(format!("not a decimal number: {text}"));
    let bytes = text.trim().as_bytes();
    let (negative, rest) = match bytes.split_first() {
        Some((b'-', rest)) => (true, rest),
        _ => (false, bytes),
    };
    let mut digits: i128 = 0;
    let mut digit_count = 0_usize;
    let mut fraction_length: i32 = 0;
    let mut index = 0;
    let mut seen_point = false;
    while let Some(&byte) = rest.get(index) {
        match byte {
            b'0'..=b'9' => {
                digit_count += 1;
                if digit_count > MAX_DIGITS {
                    return Err(invalid());
                }
                digits = digits * 10 + i128::from(byte - b'0');
                if seen_point {
                    fraction_length += 1;
                }
            }
            b'.' if !seen_point && digit_count > 0 => seen_point = true,
            b'e' | b'E' => break,
            _ => return Err(invalid()),
        }
        index += 1;
    }
    if digit_count == 0 || rest.get(index.wrapping_sub(1)) == Some(&b'.') {
        return Err(invalid());
    }
    let exponent = match rest.get(index) {
        Some(_) => parse_exponent(rest.get(index + 1..).unwrap_or_default()).ok_or_else(invalid)?,
        None => 0,
    };
    let power = exponent - fraction_length;
    let scaled = digits * i128::from(STAFF_SPACE.0);
    let magnitude = if power >= 0 {
        10_i128
            .checked_pow(power.unsigned_abs())
            .and_then(|factor| scaled.checked_mul(factor))
            .ok_or_else(invalid)?
    } else {
        match TEN.checked_pow(power.unsigned_abs()) {
            Some(divisor) => round_div(scaled, divisor),
            None => 0,
        }
    };
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map(Units).map_err(|_| invalid())
}

fn parse_exponent(bytes: &[u8]) -> Option<i32> {
    let (negative, digits) = match bytes.split_first() {
        Some((b'-', rest)) => (true, rest),
        Some((b'+', rest)) => (false, rest),
        _ => (false, bytes),
    };
    if digits.is_empty() || digits.len() > MAX_EXPONENT_DIGITS {
        return None;
    }
    let mut value: i32 = 0;
    for &byte in digits {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value * 10 + i32::from(byte - b'0');
    }
    Some(if negative { -value } else { value })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units(text: &str) -> i64 {
        staff_spaces_to_units(text).unwrap().0
    }

    #[test]
    fn decimals_become_units_rounded_half_away_from_zero() {
        assert_eq!(units("1"), 1024);
        assert_eq!(units("0"), 0);
        assert_eq!(units("-0.0"), 0);
        assert_eq!(units("1.18"), 1208);
        assert_eq!(units("0.13"), 133);
        assert_eq!(units("-0.168"), -172);
        assert_eq!(units("4.392"), 4497);
        assert_eq!(units("0.5"), 512);
        assert_eq!(units("0.001953125"), 2);
        assert_eq!(units("-0.001953125"), -2);
        assert_eq!(units("0.0019531"), 2);
        assert_eq!(units("0.00048828125"), 1);
        assert_eq!(units("0.00048828124"), 0);
        assert_eq!(units("1.5e1"), 15360);
        assert_eq!(units("25E-1"), 2560);
        assert_eq!(units("1e+0"), 1024);
    }

    #[test]
    fn malformed_numbers_are_refused() {
        for text in [
            "", "-", ".5", "1.", "1..2", "1.2.3", "abc", "1e", "1e+", "1e1000", "0x10", "--1",
            "1e40", "true", "\"1\"",
        ] {
            assert!(staff_spaces_to_units(text).is_err(), "{text}");
        }
        assert!(staff_spaces_to_units(&"9".repeat(31)).is_err());
    }

    #[test]
    fn every_three_decimal_value_matches_exact_rational_rounding() {
        for thousandths in -5000_i64..=5000 {
            let sign = if thousandths < 0 { "-" } else { "" };
            let magnitude = thousandths.abs();
            let text = format!("{sign}{}.{:03}", magnitude / 1000, magnitude % 1000);
            let expected = round_div(
                i128::from(thousandths) * 1024,
                NonZeroI128::new(1000).unwrap(),
            );
            assert_eq!(i128::from(units(&text)), expected, "{text}");
        }
    }
}
