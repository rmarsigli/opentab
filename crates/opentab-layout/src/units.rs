use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Units(pub i64);

pub const STAFF_SPACE: Units = Units(1024);

pub const STAVES_PER_EM: i64 = 4;

pub const REFERENCE_EM: Units = Units(STAFF_SPACE.0 * STAVES_PER_EM);

impl Units {
    pub const ZERO: Units = Units(0);

    pub fn scaled(self, numerator: i64, denominator: i64) -> Units {
        Units(narrow(round_div(
            i128::from(self.0) * i128::from(numerator),
            i128::from(denominator),
        )))
    }

    pub fn half(self) -> Units {
        self.scaled(1, 2)
    }
}

pub fn round_div(numerator: i128, denominator: i128) -> i128 {
    if denominator == 0 {
        return 0;
    }
    let (numerator, denominator) = if denominator < 0 {
        (-numerator, -denominator)
    } else {
        (numerator, denominator)
    };
    let magnitude = (numerator.abs() + denominator / 2) / denominator;
    if numerator < 0 { -magnitude } else { magnitude }
}

pub fn narrow(value: i128) -> i64 {
    i64::try_from(value).unwrap_or(if value < 0 { i64::MIN } else { i64::MAX })
}

impl Add for Units {
    type Output = Units;

    fn add(self, other: Units) -> Units {
        Units(self.0.saturating_add(other.0))
    }
}

impl Sub for Units {
    type Output = Units;

    fn sub(self, other: Units) -> Units {
        Units(self.0.saturating_sub(other.0))
    }
}

impl Neg for Units {
    type Output = Units;

    fn neg(self) -> Units {
        Units(self.0.saturating_neg())
    }
}

impl AddAssign for Units {
    fn add_assign(&mut self, other: Units) {
        *self = *self + other;
    }
}

impl SubAssign for Units {
    fn sub_assign(&mut self, other: Units) {
        *self = *self - other;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Point {
    pub x: Units,
    pub y: Units,
}

impl Point {
    pub const ORIGIN: Point = Point {
        x: Units::ZERO,
        y: Units::ZERO,
    };

    pub fn new(x: i64, y: i64) -> Point {
        Point {
            x: Units(x),
            y: Units(y),
        }
    }

    pub fn offset(self, by: Point) -> Point {
        Point {
            x: self.x + by.x,
            y: self.y + by.y,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rect {
    pub min: Point,
    pub max: Point,
}

impl Rect {
    pub fn new(min: Point, max: Point) -> Rect {
        Rect { min, max }
    }

    pub fn width(&self) -> Units {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> Units {
        self.max.y - self.min.y
    }

    pub fn including(self, point: Point) -> Rect {
        Rect {
            min: Point {
                x: self.min.x.min(point.x),
                y: self.min.y.min(point.y),
            },
            max: Point {
                x: self.max.x.max(point.x),
                y: self.max.y.max(point.y),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Micrometres(pub i64);

impl Micrometres {
    pub fn from_mm(mm: i64) -> Micrometres {
        Micrometres(mm.saturating_mul(1000))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_staff_space_is_1024_units_and_the_reference_em_four_of_them() {
        assert_eq!(STAFF_SPACE, Units(1024));
        assert_eq!(REFERENCE_EM, Units(4096));
    }

    #[test]
    fn rounding_is_half_away_from_zero() {
        assert_eq!(round_div(5, 2), 3);
        assert_eq!(round_div(-5, 2), -3);
        assert_eq!(round_div(4, 3), 1);
        assert_eq!(round_div(-4, 3), -1);
        assert_eq!(round_div(7, -2), -4);
        assert_eq!(round_div(1, 0), 0);
        assert_eq!(Units(3).half(), Units(2));
        assert_eq!(Units(-3).half(), Units(-2));
    }

    #[test]
    fn round_div_matches_exact_rational_rounding_for_every_small_pair() {
        for numerator in -300_i128..=300 {
            for denominator in 1_i128..=40 {
                let rounded = round_div(numerator, denominator);
                let twice = 2 * numerator;
                let low = (2 * rounded - 1) * denominator;
                let high = (2 * rounded + 1) * denominator;
                assert!(low <= twice && twice <= high, "{numerator}/{denominator}");
                if twice == low {
                    assert!(numerator > 0, "{numerator}/{denominator}");
                }
                if twice == high {
                    assert!(numerator < 0, "{numerator}/{denominator}");
                }
            }
        }
    }

    #[test]
    fn arithmetic_saturates_instead_of_overflowing() {
        assert_eq!(Units(i64::MAX) + Units(1), Units(i64::MAX));
        assert_eq!(Units(i64::MIN) - Units(1), Units(i64::MIN));
        assert_eq!(-Units(i64::MIN), Units(i64::MAX));
        assert_eq!(Units(i64::MAX).scaled(4, 1), Units(i64::MAX));
        let mut value = Units(2);
        value += Units(3);
        value -= Units(1);
        assert_eq!(value, Units(4));
    }

    #[test]
    fn rect_grows_to_include_points() {
        let rect = Rect::new(Point::new(0, 0), Point::new(2, 3)).including(Point::new(-1, 5));
        assert_eq!(rect, Rect::new(Point::new(-1, 0), Point::new(2, 5)));
        assert_eq!(rect.width(), Units(3));
        assert_eq!(rect.height(), Units(5));
        assert_eq!(Point::new(1, 2).offset(Point::new(3, 4)), Point::new(4, 6));
    }
}
