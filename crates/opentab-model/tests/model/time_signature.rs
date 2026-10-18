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

#[test]
fn only_powers_of_two_up_to_sixty_four_are_denominators() {
    let allowed = [1u8, 2, 4, 8, 16, 32, 64];
    for denominator in 0..=u8::MAX {
        let result = TimeSignature::new(4, denominator);
        if allowed.contains(&denominator) {
            let signature = result.unwrap();
            assert_eq!(signature.denominator, denominator);
        } else {
            assert_eq!(
                result,
                Err(ModelError::InvalidTimeSignature {
                    numerator: 4,
                    denominator
                }),
                "denominator {denominator}"
            );
        }
    }
}

#[test]
fn non_power_denominators_are_rejected_from_json() {
    for denominator in [3, 5, 6, 12, 48, 96, 128, 255] {
        let text = format!(r#"{{"numerator":4,"denominator":{denominator}}}"#);
        let error = serde_json::from_str::<TimeSignature>(&text).unwrap_err();
        assert!(error.to_string().contains("time signature"), "{text}");
    }
    let good: TimeSignature = serde_json::from_str(r#"{"numerator":3,"denominator":64}"#).unwrap();
    assert_eq!(good, TimeSignature::new(3, 64).unwrap());
}
