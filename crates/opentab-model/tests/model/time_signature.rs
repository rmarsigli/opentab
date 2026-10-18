use opentab_model::{Fraction, ModelError, TimeSignature};

#[test]
fn bar_length_of_common_signatures() {
    let cases = [
        (4, 4, Fraction::new(1, 1)),
        (3, 4, Fraction::new(3, 4)),
        (6, 8, Fraction::new(3, 4)),
        (7, 8, Fraction::new(7, 8)),
        (12, 16, Fraction::new(3, 4)),
    ];
    for (numerator, denominator, expected) in cases {
        let signature = TimeSignature::new(numerator, denominator).unwrap();
        assert_eq!(
            signature.bar_length(),
            expected,
            "{numerator}/{denominator}"
        );
    }
}

#[test]
fn zero_terms_are_rejected() {
    for (numerator, denominator) in [(0, 4), (4, 0), (0, 0)] {
        assert_eq!(
            TimeSignature::new(numerator, denominator),
            Err(ModelError::InvalidTimeSignature {
                numerator,
                denominator
            })
        );
    }
}

#[test]
fn zero_denominator_literal_does_not_panic() {
    let signature = TimeSignature {
        numerator: 4,
        denominator: 0,
    };
    assert_eq!(signature.bar_length(), Fraction::from_integer(0));
}
