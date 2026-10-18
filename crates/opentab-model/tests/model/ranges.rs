use opentab_model::{BendPoint, KeySignature, ModelError};

#[test]
fn bend_point_accepts_inclusive_bounds() {
    for (offset, value) in [(0, 0), (100, 48), (100, -48), (0, -48), (50, 1)] {
        assert_eq!(
            BendPoint::new(offset, value),
            Ok(BendPoint { offset, value })
        );
    }
}

#[test]
fn bend_point_rejects_values_just_outside() {
    for (offset, value) in [
        (101, 0),
        (255, 0),
        (0, 49),
        (0, -49),
        (0, i16::MIN),
        (0, i16::MAX),
    ] {
        assert_eq!(
            BendPoint::new(offset, value),
            Err(ModelError::InvalidBendPoint { offset, value })
        );
    }
}

#[test]
fn bend_point_json_applies_the_same_bounds() {
    let good: BendPoint = serde_json::from_str(r#"{"offset":100,"value":-48}"#).unwrap();
    assert_eq!(
        good,
        BendPoint {
            offset: 100,
            value: -48
        }
    );
    for text in [
        r#"{"offset":101,"value":0}"#,
        r#"{"offset":250,"value":0}"#,
        r#"{"offset":0,"value":49}"#,
        r#"{"offset":0,"value":-32000}"#,
    ] {
        let error = serde_json::from_str::<BendPoint>(text).unwrap_err();
        assert!(error.to_string().contains("bend point"), "{text}: {error}");
    }
}

#[test]
fn key_signature_accepts_inclusive_bounds() {
    for accidentals in [-7, -1, 0, 1, 7] {
        for minor in [false, true] {
            assert_eq!(
                KeySignature::new(accidentals, minor),
                Ok(KeySignature { accidentals, minor })
            );
        }
    }
}

#[test]
fn key_signature_rejects_values_just_outside() {
    for accidentals in [8, -8, i8::MAX, i8::MIN] {
        assert_eq!(
            KeySignature::new(accidentals, false),
            Err(ModelError::InvalidKeySignature { accidentals })
        );
    }
}

#[test]
fn key_signature_json_applies_the_same_bounds() {
    let good: KeySignature = serde_json::from_str(r#"{"accidentals":-7,"minor":true}"#).unwrap();
    assert_eq!(
        good,
        KeySignature {
            accidentals: -7,
            minor: true
        }
    );
    for text in [
        r#"{"accidentals":8,"minor":false}"#,
        r#"{"accidentals":-128,"minor":false}"#,
    ] {
        let error = serde_json::from_str::<KeySignature>(text).unwrap_err();
        assert!(
            error.to_string().contains("key signature"),
            "{text}: {error}"
        );
    }
}
