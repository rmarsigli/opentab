use std::num::NonZeroI64;
use std::sync::OnceLock;

use opentab_model::{Finger, Fraction, HarmonicKind, Note, NoteKind, Tap};

use crate::glyphs::{Metrics, NOTEHEAD_BLACK};
use crate::text::text_font;
use crate::units::{REFERENCE_EM, Rect, STAFF_SPACE, Units};

pub const FRET_EM: Units = Units(STAFF_SPACE.0 * 3 / 2);
pub const MARK_EM: Units = Units(STAFF_SPACE.0 * 11 / 10);
pub const FINGER_EM: Units = Units(STAFF_SPACE.0);
pub const LABEL_PAD: Units = Units(STAFF_SPACE.0 * 3 / 20);
pub const CENTS_PER_QUARTER: i64 = 50;
pub const QUARTERS_PER_STEP: i64 = 4;
const EM_DIVISOR: NonZeroI64 = NonZeroI64::new(REFERENCE_EM.0).unwrap();
const SPACE_DIVISOR: NonZeroI64 = NonZeroI64::new(STAFF_SPACE.0).unwrap();
const TENTHS: i128 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DigitBox {
    pub top: Units,
    pub bottom: Units,
}

static DIGIT: OnceLock<DigitBox> = OnceLock::new();

fn digit_reference() -> DigitBox {
    *DIGIT.get_or_init(|| {
        let bounds = text_font().outline('0').bounds().unwrap_or_default();
        DigitBox {
            top: bounds.min.y,
            bottom: bounds.max.y,
        }
    })
}

pub fn digit_box(em: Units) -> DigitBox {
    let reference = digit_reference();
    DigitBox {
        top: reference.top.scaled(em.0, EM_DIVISOR),
        bottom: reference.bottom.scaled(em.0, EM_DIVISOR),
    }
}

pub fn digit_height(em: Units) -> Units {
    let digit = digit_box(em);
    digit.bottom - digit.top
}

pub fn baseline_below_centre(em: Units) -> Units {
    let digit = digit_box(em);
    -(digit.top + (digit.bottom - digit.top).half())
}

pub fn measure(text: &str, em: Units) -> Units {
    text_font().measure(text, em)
}

pub fn label_box(width: Units, em: Units) -> Rect {
    let half_height = digit_height(em).half() + LABEL_PAD;
    let half_width = width.half() + LABEL_PAD;
    Rect::new(
        crate::units::Point {
            x: -half_width,
            y: -half_height,
        },
        crate::units::Point {
            x: half_width,
            y: half_height,
        },
    )
}

pub fn half_head(size: Units) -> Units {
    Metrics::bravura()
        .bbox(NOTEHEAD_BLACK.name)
        .map_or(STAFF_SPACE, |rect| rect.max.x)
        .scaled(size.0, SPACE_DIVISOR)
        .half()
}

pub fn decimal(value: Fraction) -> String {
    ratio_text(i128::from(*value.numer()), i128::from(*value.denom()))
}

pub fn harmonic_above(fret: u8, harmonic: Fraction) -> String {
    let denom = i128::from(*harmonic.denom());
    let numer = i128::from(*harmonic.numer()) + i128::from(fret) * denom;
    ratio_text(numer, denom)
}

fn ratio_text(numer: i128, denom: i128) -> String {
    if denom <= 0 {
        return String::new();
    }
    let tenths = (numer * TENTHS * 2 + denom) / (denom * 2);
    let whole = tenths / TENTHS;
    let rest = tenths % TENTHS;
    if rest == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{rest}")
    }
}

pub fn fret_text(note: &Note) -> Option<String> {
    let NoteKind::Fretted { fret, .. } = note.kind else {
        return None;
    };
    let effects = &note.effects;
    let inner = if effects.dead {
        "x".to_string()
    } else {
        match effects.harmonic {
            Some(harmonic) if harmonic.kind == HarmonicKind::Natural => {
                format!("<{}>", decimal(harmonic.fret))
            }
            Some(harmonic) => format!("{fret} <{}>", harmonic_above(fret, harmonic.fret)),
            None => fret.to_string(),
        }
    };
    Some(if effects.ghost || effects.tie_destination {
        format!("({inner})")
    } else {
        inner
    })
}

pub fn bend_text(cents: i64) -> String {
    let magnitude = cents.unsigned_abs();
    let quarter = CENTS_PER_QUARTER.unsigned_abs();
    let quarters = ((magnitude * 2 + quarter) / (quarter * 2)).max(1);
    let steps = QUARTERS_PER_STEP.unsigned_abs();
    let whole = quarters / steps;
    let fraction = match quarters % steps {
        1 => "\u{bc}",
        2 => "\u{bd}",
        3 => "\u{be}",
        _ => "",
    };
    let amount = match (whole, fraction) {
        (1, "") => "full".to_string(),
        (0, _) => fraction.to_string(),
        (_, _) => format!("{whole}{fraction}"),
    };
    if cents < 0 {
        format!("\u{2212}{amount}")
    } else {
        amount
    }
}

pub fn finger_text(finger: Finger) -> &'static str {
    match finger {
        Finger::Open => "0",
        Finger::Thumb => "T",
        Finger::Index => "1",
        Finger::Middle => "2",
        Finger::Ring => "3",
        Finger::Little => "4",
    }
}

pub fn harmonic_mark(kind: HarmonicKind) -> Option<&'static str> {
    match kind {
        HarmonicKind::Natural => None,
        HarmonicKind::Artificial => Some("A.H."),
        HarmonicKind::Pinch => Some("P.H."),
        HarmonicKind::Tap => Some("T.H."),
        HarmonicKind::Semi => Some("S.H."),
        HarmonicKind::Feedback => Some("F.H."),
    }
}

pub fn tap_mark(tap: Tap) -> &'static str {
    match tap {
        Tap::Right => "T",
        Tap::Left => "LT",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentab_model::{Harmonic, NoteEffects, NoteId};

    fn note(fret: u8, effects: NoteEffects) -> Note {
        Note {
            id: NoteId::from(opentab_model::ElementId(1)),
            kind: NoteKind::Fretted { string: 0, fret },
            effects,
        }
    }

    #[test]
    fn bend_amounts_are_labelled_in_whole_steps_from_cents() {
        let cases = [
            (50, "\u{bc}"),
            (100, "\u{bd}"),
            (150, "\u{be}"),
            (200, "full"),
            (250, "1\u{bc}"),
            (300, "1\u{bd}"),
            (400, "2"),
            (600, "3"),
            (1200, "6"),
            (75, "\u{bd}"),
            (10, "\u{bc}"),
            (-100, "\u{2212}\u{bd}"),
            (-200, "\u{2212}full"),
        ];
        for (cents, label) in cases {
            assert_eq!(bend_text(cents), label, "{cents} cents");
        }
    }

    #[test]
    fn harmonic_frets_print_with_at_most_one_decimal() {
        assert_eq!(decimal(Fraction::new(12, 1)), "12");
        assert_eq!(decimal(Fraction::new(16, 5)), "3.2");
        assert_eq!(decimal(Fraction::new(29, 10)), "2.9");
        assert_eq!(decimal(Fraction::new(8, 3)), "2.7");
        assert_eq!(decimal(Fraction::new(1, 20)), "0.1");
        assert_eq!(decimal(Fraction::new(0, 1)), "0");
    }

    #[test]
    fn fret_texts_follow_the_note_effects() {
        assert_eq!(
            fret_text(&note(5, NoteEffects::default())),
            Some("5".into())
        );
        let dead = NoteEffects {
            dead: true,
            ..NoteEffects::default()
        };
        assert_eq!(fret_text(&note(5, dead)), Some("x".into()));
        let ghost = NoteEffects {
            ghost: true,
            ..NoteEffects::default()
        };
        assert_eq!(fret_text(&note(7, ghost)), Some("(7)".into()));
        let tied = NoteEffects {
            tie_destination: true,
            ..NoteEffects::default()
        };
        assert_eq!(fret_text(&note(12, tied)), Some("(12)".into()));
        let natural = NoteEffects {
            harmonic: Some(Harmonic {
                kind: HarmonicKind::Natural,
                fret: Fraction::new(16, 5),
            }),
            ..NoteEffects::default()
        };
        assert_eq!(fret_text(&note(3, natural)), Some("<3.2>".into()));
        let pinch = NoteEffects {
            harmonic: Some(Harmonic {
                kind: HarmonicKind::Pinch,
                fret: Fraction::new(12, 1),
            }),
            ..NoteEffects::default()
        };
        assert_eq!(fret_text(&note(5, pinch)), Some("5 <17>".into()));
        let tapped = NoteEffects {
            harmonic: Some(Harmonic {
                kind: HarmonicKind::Tap,
                fret: Fraction::new(5, 2),
            }),
            ghost: true,
            ..NoteEffects::default()
        };
        assert_eq!(fret_text(&note(7, tapped)), Some("(7 <9.5>)".into()));
        assert_eq!(
            harmonic_above(u8::MAX, Fraction::new(i64::MAX, 1)),
            format!("{}", i128::from(i64::MAX) + 255)
        );
    }

    #[test]
    fn the_text_font_holds_every_label_character() {
        for character in "0123456789x()<>[].\u{bc}\u{bd}\u{be}\u{2212}fulTLAHPSFMeting".chars() {
            assert!(text_font().has(character), "{character}");
        }
        assert!(digit_height(FRET_EM) > Units(900));
        assert!(digit_height(FRET_EM) < Units(1100));
    }
}
