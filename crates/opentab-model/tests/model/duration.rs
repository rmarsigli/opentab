use opentab_model::{Duration, Fraction, ModelError, NoteValue, Tuplet};

const VALUES: [(NoteValue, i64); 8] = [
    (NoteValue::Whole, 1),
    (NoteValue::Half, 2),
    (NoteValue::Quarter, 4),
    (NoteValue::Eighth, 8),
    (NoteValue::Sixteenth, 16),
    (NoteValue::ThirtySecond, 32),
    (NoteValue::SixtyFourth, 64),
    (NoteValue::OneTwentyEighth, 128),
];

const DOT_FACTORS: [(u8, i64, i64); 4] = [(0, 1, 1), (1, 3, 2), (2, 7, 4), (3, 15, 8)];

const TUPLETS: [(u8, u8, i64, i64); 3] = [(3, 2, 2, 3), (5, 4, 4, 5), (7, 8, 8, 7)];

#[test]
fn length_of_every_value_and_dot_count() {
    for (value, denominator) in VALUES {
        for (dots, numer, denom) in DOT_FACTORS {
            let duration = Duration::new(value, dots, None).unwrap();
            let expected = Fraction::new(numer, denom * denominator);
            assert_eq!(duration.length(), expected, "{value:?} with {dots} dots");
        }
    }
}

#[test]
fn length_with_tuplets() {
    for (value, denominator) in VALUES {
        for (dots, dot_numer, dot_denom) in DOT_FACTORS {
            for (num, den, ratio_numer, ratio_denom) in TUPLETS {
                let tuplet = Tuplet { num, den };
                let duration = Duration::new(value, dots, Some(tuplet)).unwrap();
                let expected = Fraction::new(
                    dot_numer * ratio_numer,
                    dot_denom * denominator * ratio_denom,
                );
                assert_eq!(duration.length(), expected, "{value:?} {dots} {num}:{den}");
            }
        }
    }
}

#[test]
fn length_spot_checks() {
    let dotted_quarter = Duration::new(NoteValue::Quarter, 1, None).unwrap();
    assert_eq!(dotted_quarter.length(), Fraction::new(3, 8));
    let triplet_eighth = Duration::new(NoteValue::Eighth, 0, Some(Tuplet { num: 3, den: 2 }));
    assert_eq!(triplet_eighth.unwrap().length(), Fraction::new(1, 12));
    let septuplet = Duration::new(NoteValue::ThirtySecond, 0, Some(Tuplet { num: 7, den: 8 }));
    assert_eq!(septuplet.unwrap().length(), Fraction::new(1, 28));
    let whole = Duration::new(NoteValue::Whole, 3, None).unwrap();
    assert_eq!(whole.length(), Fraction::new(15, 8));
}

#[test]
fn new_rejects_more_than_three_dots() {
    for dots in [4, 5, 255] {
        assert_eq!(
            Duration::new(NoteValue::Quarter, dots, None),
            Err(ModelError::InvalidDots { dots })
        );
    }
}

#[test]
fn new_rejects_tuplet_terms_outside_range() {
    for (num, den) in [(0, 2), (2, 0), (0, 0), (65, 64), (64, 65), (255, 1)] {
        assert_eq!(
            Duration::new(NoteValue::Quarter, 0, Some(Tuplet { num, den })),
            Err(ModelError::InvalidTuplet { num, den })
        );
    }
}

#[test]
fn new_accepts_boundaries() {
    for (num, den) in [(1, 1), (64, 64), (1, 64), (64, 1)] {
        let duration = Duration::new(NoteValue::OneTwentyEighth, 3, Some(Tuplet { num, den }));
        let duration = duration.unwrap();
        assert_eq!(duration.value(), NoteValue::OneTwentyEighth);
        assert_eq!(duration.dots(), 3);
        assert_eq!(duration.tuplet(), Some(Tuplet { num, den }));
    }
}
