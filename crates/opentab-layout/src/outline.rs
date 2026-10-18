use std::num::{NonZeroI64, NonZeroI128};

use ttf_parser::{Face, GlyphId, OutlineBuilder};

use crate::display::PathSegment;
use crate::units::{Point, REFERENCE_EM, REFERENCE_EM_DIVISOR, Rect, Units, narrow, round_div};

const FLOAT_MANTISSA_BITS: u32 = 23;
const FLOAT_EXPONENT_MASK: u32 = 0xff;
const FLOAT_EXPONENT_BIAS: i32 = 150;
const FLOAT_SUBNORMAL_POWER: i32 = -149;
const MAX_SHIFT: u32 = 64;
const NOTDEF: GlyphId = GlyphId(0);
const TWO: NonZeroI128 = NonZeroI128::new(2).unwrap();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontError(pub String);

impl std::fmt::Display for FontError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for FontError {}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Outline {
    pub segments: Vec<PathSegment>,
}

impl Outline {
    pub fn bounds(&self) -> Option<Rect> {
        let mut bounds: Option<Rect> = None;
        for point in self.segments.iter().flat_map(segment_points) {
            bounds = Some(match bounds {
                Some(rect) => rect.including(point),
                None => Rect::new(point, point),
            });
        }
        bounds
    }

    pub fn placed(&self, origin: Point, em: Units) -> Vec<PathSegment> {
        let place = |point: Point| Point {
            x: origin.x + point.x.scaled(em.0, REFERENCE_EM_DIVISOR),
            y: origin.y + point.y.scaled(em.0, REFERENCE_EM_DIVISOR),
        };
        self.segments
            .iter()
            .map(|segment| match *segment {
                PathSegment::MoveTo(to) => PathSegment::MoveTo(place(to)),
                PathSegment::LineTo(to) => PathSegment::LineTo(place(to)),
                PathSegment::QuadTo(control, to) => PathSegment::QuadTo(place(control), place(to)),
                PathSegment::CubicTo(first, second, to) => {
                    PathSegment::CubicTo(place(first), place(second), place(to))
                }
                PathSegment::Close => PathSegment::Close,
            })
            .collect()
    }
}

fn segment_points(segment: &PathSegment) -> Vec<Point> {
    match *segment {
        PathSegment::MoveTo(to) | PathSegment::LineTo(to) => vec![to],
        PathSegment::QuadTo(control, to) => vec![control, to],
        PathSegment::CubicTo(first, second, to) => vec![first, second, to],
        PathSegment::Close => Vec::new(),
    }
}

pub struct OutlineFont {
    face: Face<'static>,
    units_per_em: NonZeroI64,
}

impl OutlineFont {
    pub fn parse(bytes: &'static [u8]) -> Result<OutlineFont, FontError> {
        let face = Face::parse(bytes, 0).map_err(|error| FontError(error.to_string()))?;
        let units_per_em = NonZeroI64::new(i64::from(face.units_per_em()))
            .filter(|value| value.get() > 0)
            .ok_or_else(|| FontError("font has no units per em".to_string()))?;
        Ok(OutlineFont { face, units_per_em })
    }

    pub fn units_per_em(&self) -> i64 {
        self.units_per_em.get()
    }

    pub fn has(&self, character: char) -> bool {
        self.face.glyph_index(character).is_some()
    }

    fn glyph(&self, character: char) -> GlyphId {
        self.face.glyph_index(character).unwrap_or(NOTDEF)
    }

    pub fn outline(&self, character: char) -> Outline {
        let mut builder = Converter {
            units_per_em: self.units_per_em,
            segments: Vec::new(),
        };
        let _ = self.face.outline_glyph(self.glyph(character), &mut builder);
        Outline {
            segments: builder.segments,
        }
    }

    pub fn codepoints(&self) -> Vec<char> {
        let mut found = std::collections::BTreeSet::new();
        if let Some(cmap) = self.face.tables().cmap {
            for subtable in cmap
                .subtables
                .into_iter()
                .filter(|table| table.is_unicode())
            {
                subtable.codepoints(|codepoint| {
                    if let Some(character) = char::from_u32(codepoint) {
                        found.insert(character);
                    }
                });
            }
        }
        found.into_iter().collect()
    }

    pub fn advance_font_units(&self, character: char) -> i64 {
        self.face
            .glyph_hor_advance(self.glyph(character))
            .map_or(0, i64::from)
    }

    pub fn advance(&self, character: char, em: Units) -> Units {
        self.to_em(self.advance_font_units(character), em)
    }

    pub fn pen_positions(&self, text: &str, em: Units) -> Vec<Units> {
        let mut total: i64 = 0;
        let mut positions = vec![Units::ZERO];
        for character in text.chars() {
            total = total.saturating_add(self.advance_font_units(character));
            positions.push(self.to_em(total, em));
        }
        positions
    }

    pub fn measure(&self, text: &str, em: Units) -> Units {
        self.pen_positions(text, em)
            .last()
            .copied()
            .unwrap_or(Units::ZERO)
    }

    fn to_em(&self, font_units: i64, em: Units) -> Units {
        Units(narrow(round_div(
            i128::from(font_units) * i128::from(em.0),
            NonZeroI128::from(self.units_per_em),
        )))
    }
}

struct Converter {
    units_per_em: NonZeroI64,
    segments: Vec<PathSegment>,
}

impl Converter {
    fn point(&self, x: f32, y: f32) -> Point {
        Point {
            x: Units(reference_units(x.to_bits(), self.units_per_em)),
            y: Units(-reference_units(y.to_bits(), self.units_per_em)),
        }
    }
}

impl OutlineBuilder for Converter {
    fn move_to(&mut self, x: f32, y: f32) {
        let to = self.point(x, y);
        self.segments.push(PathSegment::MoveTo(to));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let to = self.point(x, y);
        self.segments.push(PathSegment::LineTo(to));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let control = self.point(x1, y1);
        let to = self.point(x, y);
        self.segments.push(PathSegment::QuadTo(control, to));
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let first = self.point(x1, y1);
        let second = self.point(x2, y2);
        let to = self.point(x, y);
        self.segments.push(PathSegment::CubicTo(first, second, to));
    }

    fn close(&mut self) {
        self.segments.push(PathSegment::Close);
    }
}

pub fn reference_units(bits: u32, units_per_em: NonZeroI64) -> i64 {
    let negative = bits >> 31 == 1;
    let exponent = (bits >> FLOAT_MANTISSA_BITS) & FLOAT_EXPONENT_MASK;
    let fraction = i128::from(bits & ((1 << FLOAT_MANTISSA_BITS) - 1));
    if exponent == FLOAT_EXPONENT_MASK {
        return 0;
    }
    let (mantissa, power) = if exponent == 0 {
        (fraction, FLOAT_SUBNORMAL_POWER)
    } else {
        (
            fraction | (1 << FLOAT_MANTISSA_BITS),
            i32::try_from(exponent).unwrap_or(0) - FLOAT_EXPONENT_BIAS,
        )
    };
    let numerator = mantissa * i128::from(REFERENCE_EM.0);
    let denominator = NonZeroI128::from(units_per_em);
    let shift = power.unsigned_abs();
    let magnitude = if power >= 0 {
        if shift >= MAX_SHIFT {
            i128::from(i64::MAX)
        } else {
            round_div(numerator << shift, denominator)
        }
    } else if shift >= MAX_SHIFT {
        0
    } else {
        round_div(numerator, shifted(denominator, shift))
    };
    let value = narrow(magnitude);
    if negative { -value } else { value }
}

fn shifted(value: NonZeroI128, shift: u32) -> NonZeroI128 {
    (0..shift).fold(value, |acc, _| acc.saturating_mul(TWO))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upm(value: i64) -> NonZeroI64 {
        NonZeroI64::new(value).unwrap()
    }

    fn by(value: i128) -> NonZeroI128 {
        NonZeroI128::new(value).unwrap()
    }

    fn bits_of(value: i64, scale_power: i32) -> u32 {
        let mut bits = 0_u32;
        let negative = value < 0;
        let magnitude = value.unsigned_abs();
        if magnitude == 0 {
            return if negative { 1 << 31 } else { 0 };
        }
        let width = 64 - magnitude.leading_zeros();
        let top = i32::try_from(width).unwrap() - 1;
        let exponent = u32::try_from(top + scale_power + 127).unwrap();
        let mantissa = if top >= 23 {
            u32::try_from(magnitude >> (top - 23)).unwrap()
        } else {
            u32::try_from(magnitude << (23 - top)).unwrap()
        };
        bits |= exponent << 23;
        bits |= mantissa & 0x7f_ffff;
        if negative {
            bits |= 1 << 31;
        }
        bits
    }

    #[test]
    fn integer_font_units_scale_exactly_for_every_value_in_range() {
        for value in -3000_i64..=3000 {
            let bits = bits_of(value, 0);
            let expected = round_div(i128::from(value) * 4096, by(1000));
            assert_eq!(
                i128::from(reference_units(bits, upm(1000))),
                expected,
                "{value}"
            );
            let expected_2048 = round_div(i128::from(value) * 4096, by(2048));
            assert_eq!(
                i128::from(reference_units(bits, upm(2048))),
                expected_2048,
                "{value}"
            );
        }
    }

    #[test]
    fn fractional_font_units_scale_exactly() {
        for value in -400_i64..=400 {
            let bits = bits_of(value, -2);
            let expected = round_div(i128::from(value) * 4096, by(4000));
            assert_eq!(
                i128::from(reference_units(bits, upm(1000))),
                expected,
                "{value}/4"
            );
        }
    }

    #[test]
    fn special_and_extreme_bit_patterns_do_not_overflow() {
        assert_eq!(reference_units(0x7f80_0000, upm(1000)), 0);
        assert_eq!(reference_units(0x7fc0_0000, upm(1000)), 0);
        assert_eq!(reference_units(0x0000_0001, upm(1000)), 0);
        assert_eq!(reference_units(0x8000_0000, upm(1000)), 0);
        assert_eq!(reference_units(0x7f7f_ffff, upm(1000)), i64::MAX);
        assert_eq!(reference_units(0xff7f_ffff, upm(1000)), -i64::MAX);
    }
}
