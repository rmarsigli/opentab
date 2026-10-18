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

#[test]
fn tricky_floats_survive_a_json_round_trip_bit_for_bit() {
    for text in [
        "128.48806208011501",
        "1.6506591711229999",
        "2.3204331470510002",
        "1.1637998953470001",
        "2.5703550371409998",
        "1.2690320673639999",
    ] {
        let bpm: f64 = text.parse().unwrap();
        let tempo = Tempo::new(bpm).unwrap();
        let written = serde_json::to_string(&tempo).unwrap();
        let back: Tempo = serde_json::from_str(&written).unwrap();
        assert_eq!(back.bpm().to_bits(), bpm.to_bits(), "{written}");
        let direct: Tempo = serde_json::from_str(text).unwrap();
        assert_eq!(direct.bpm().to_bits(), bpm.to_bits(), "{text}");
    }
}

#[test]
fn tricky_float_literal_in_a_document_parses_to_the_nearest_double() {
    let text = "128.48806208011501";
    let back: Tempo = serde_json::from_str(text).unwrap();
    assert_eq!(back.bpm().to_bits(), text.parse::<f64>().unwrap().to_bits());
}
