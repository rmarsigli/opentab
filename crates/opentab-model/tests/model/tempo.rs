use opentab_model::{ModelError, Tempo};

#[test]
fn accepts_range_bounds() {
    for bpm in [1.0, 120.0, 999.999, 1000.0] {
        assert_eq!(Tempo::new(bpm).unwrap().bpm(), bpm);
    }
}

#[test]
fn rejects_out_of_range_and_non_finite() {
    for bpm in [
        0.0,
        -0.0,
        0.999,
        -120.0,
        1000.001,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        assert_eq!(Tempo::new(bpm), Err(ModelError::InvalidTempo { bpm }));
    }
    assert!(Tempo::new(f64::NAN).is_err());
}
