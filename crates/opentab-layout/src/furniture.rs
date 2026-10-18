use std::num::NonZeroI64;

use opentab_model::{KeySignature, TimeSignature};

use crate::display::{PlacedGlyph, Primitive};
use crate::glyphs::{
    ACCIDENTAL_FLAT, ACCIDENTAL_NATURAL, ACCIDENTAL_SHARP, F_CLEF, G_CLEF, G_CLEF_8VB, Glyph,
    Metrics, TAB_CLEF, TAB_CLEF_4, TIME_SIG_DIGITS, UNPITCHED_PERCUSSION_CLEF_1,
};
use crate::structure::{ClefSign, StaffLayout};
use crate::units::{Point, Rect, STAFF_SPACE, Units};

pub const HEAD_PAD: Units = Units(512);
pub const HEAD_GAP: Units = Units(1024);
pub const KEY_GAP: Units = Units(128);
pub const TAB_CLEF_SHARE: i64 = 4;
pub const TAB_CLEF_PARTS: i64 = 5;
pub const MIN_TAB_CLEF_SIZE: Units = Units(512);
pub const MAX_TAB_CLEF_SIZE: Units = Units(2048);
pub const SHARP_STEPS: [i64; 7] = [0, 3, -1, 2, 5, 1, 4];
pub const FLAT_STEPS: [i64; 7] = [4, 1, 5, 2, 6, 3, 7];
pub const BASS_STEP_OFFSET: i64 = 2;
const HALF_SPACE: Units = Units(STAFF_SPACE.0 / 2);

pub fn bbox(glyph: Glyph) -> Rect {
    Metrics::bravura().bbox(glyph.name).unwrap_or_default()
}

pub fn advance(glyph: Glyph) -> Units {
    Metrics::bravura().advance(glyph.name).unwrap_or_default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaffShape {
    pub lines: usize,
    pub spacing: Units,
}

impl StaffShape {
    pub fn height(self) -> Units {
        let gaps = i64::try_from(self.lines.saturating_sub(1)).unwrap_or(i64::MAX);
        Units(self.spacing.0.saturating_mul(gaps))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizedGlyph {
    pub glyph: Glyph,
    pub size: Units,
}

impl SizedGlyph {
    pub fn width(self) -> Units {
        bbox(self.glyph)
            .max
            .x
            .scaled(self.size.0, STAFF_SPACE_DIVISOR)
    }
}

const STAFF_SPACE_DIVISOR: NonZeroI64 = NonZeroI64::new(STAFF_SPACE.0).unwrap();

pub fn clef_glyph(sign: ClefSign, shape: StaffShape) -> SizedGlyph {
    let normal = |glyph| SizedGlyph {
        glyph,
        size: STAFF_SPACE,
    };
    match sign {
        ClefSign::Treble => normal(G_CLEF),
        ClefSign::Treble8vb => normal(G_CLEF_8VB),
        ClefSign::Bass => normal(F_CLEF),
        ClefSign::Neutral => normal(UNPITCHED_PERCUSSION_CLEF_1),
        ClefSign::Tab => tab_clef(shape),
    }
}

fn tab_clef(shape: StaffShape) -> SizedGlyph {
    let glyph = if shape.lines >= 5 {
        TAB_CLEF
    } else {
        TAB_CLEF_4
    };
    let target = shape.height().0.saturating_mul(TAB_CLEF_SHARE) / TAB_CLEF_PARTS;
    let size = match NonZeroI64::new(bbox(glyph).height().0) {
        Some(height) => STAFF_SPACE.scaled(target, height),
        None => STAFF_SPACE,
    };
    SizedGlyph {
        glyph,
        size: size.clamp(MIN_TAB_CLEF_SIZE, MAX_TAB_CLEF_SIZE),
    }
}

pub fn clef_origin_y(sign: ClefSign, staff: &StaffLayout) -> Units {
    match sign {
        ClefSign::Treble | ClefSign::Treble8vb => staff.line_y(3),
        ClefSign::Bass => staff.line_y(1),
        ClefSign::Neutral | ClefSign::Tab => staff.middle(),
    }
}

pub fn draw_clef(sign: ClefSign, x: Units, staff: &StaffLayout, items: &mut Vec<Primitive>) {
    let shape = StaffShape {
        lines: staff.lines,
        spacing: staff.line_spacing,
    };
    let sized = clef_glyph(sign, shape);
    items.push(Primitive::Glyph(PlacedGlyph {
        origin: Point {
            x,
            y: clef_origin_y(sign, staff),
        },
        codepoint: sized.glyph.codepoint,
        staff_space: sized.size,
    }));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    Sharps,
    Flats,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyDraw {
    pub glyph: Glyph,
    pub kind: KeyKind,
    pub count: usize,
}

impl KeyDraw {
    pub fn of(key: KeySignature) -> Option<KeyDraw> {
        let count = usize::from(key.accidentals().unsigned_abs());
        match key.accidentals() {
            0 => None,
            value if value > 0 => Some(KeyDraw {
                glyph: ACCIDENTAL_SHARP,
                kind: KeyKind::Sharps,
                count,
            }),
            _ => Some(KeyDraw {
                glyph: ACCIDENTAL_FLAT,
                kind: KeyKind::Flats,
                count,
            }),
        }
    }

    pub fn change(old: KeySignature, new: KeySignature) -> Option<KeyDraw> {
        if old.accidentals() == new.accidentals() {
            return None;
        }
        match (KeyDraw::of(new), KeyDraw::of(old)) {
            (Some(draw), _) => Some(draw),
            (None, Some(cancelled)) => Some(KeyDraw {
                glyph: ACCIDENTAL_NATURAL,
                ..cancelled
            }),
            (None, None) => None,
        }
    }

    pub fn advance(self) -> Units {
        bbox(self.glyph).max.x + KEY_GAP
    }

    pub fn width(self) -> Units {
        let count = i64::try_from(self.count).unwrap_or(i64::MAX);
        Units(self.advance().0.saturating_mul(count))
    }

    pub fn steps(self) -> &'static [i64; 7] {
        match self.kind {
            KeyKind::Sharps => &SHARP_STEPS,
            KeyKind::Flats => &FLAT_STEPS,
        }
    }

    pub fn draw(self, sign: ClefSign, x: Units, staff: &StaffLayout, items: &mut Vec<Primitive>) {
        let offset = match sign {
            ClefSign::Treble | ClefSign::Treble8vb => 0,
            ClefSign::Bass => BASS_STEP_OFFSET,
            ClefSign::Neutral | ClefSign::Tab => return,
        };
        let advance = self.advance();
        let mut pen = x;
        for step in self.steps().iter().take(self.count) {
            items.push(Primitive::Glyph(PlacedGlyph {
                origin: Point {
                    x: pen,
                    y: staff.top + Units(HALF_SPACE.0.saturating_mul(step + offset)),
                },
                codepoint: self.glyph.codepoint,
                staff_space: STAFF_SPACE,
            }));
            pen += advance;
        }
    }
}

pub fn digits(number: u8) -> Vec<Glyph> {
    number
        .to_string()
        .bytes()
        .filter_map(|byte| TIME_SIG_DIGITS.get(usize::from(byte.wrapping_sub(b'0'))))
        .copied()
        .collect()
}

pub fn number_width(number: u8) -> Units {
    digits(number)
        .into_iter()
        .fold(Units::ZERO, |total, glyph| total + advance(glyph))
}

pub fn time_width(time: TimeSignature) -> Units {
    number_width(time.numerator()).max(number_width(time.denominator()))
}

pub fn draw_time(time: TimeSignature, x: Units, staff: &StaffLayout, items: &mut Vec<Primitive>) {
    let width = time_width(time);
    let middle = staff.middle();
    for (number, y) in [
        (time.numerator(), middle - STAFF_SPACE),
        (time.denominator(), middle + STAFF_SPACE),
    ] {
        let mut pen = x + (width - number_width(number)).half();
        for glyph in digits(number) {
            items.push(Primitive::Glyph(PlacedGlyph {
                origin: Point { x: pen, y },
                codepoint: glyph.codepoint,
                staff_space: STAFF_SPACE,
            }));
            pen += advance(glyph);
        }
    }
}
