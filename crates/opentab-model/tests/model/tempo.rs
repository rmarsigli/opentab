use opentab_model::{Fraction, ModelError, Tempo};

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

const HUNDREDTHS: u32 = 100;
const TEN_THOUSANDTHS: u32 = 10_000;
const MILLIONTHS: u32 = 1_000_000;
const TEN_MILLIONTHS: u64 = 10_000_000;

fn decimal_text(units: u64, scale: u64, places: usize) -> String {
    format!("{}.{:0places$}", units / scale, units % scale)
}

fn exact_of(text: &str) -> Option<Fraction> {
    Tempo::new(text.parse::<f64>().unwrap()).unwrap().exact()
}

fn fraction(units: u64, scale: u64) -> Fraction {
    Fraction::new(i64::try_from(units).unwrap(), i64::try_from(scale).unwrap())
}

#[test]
fn every_hundredth_from_one_to_a_thousand_bpm_is_exactly_its_decimal_text() {
    let scale = u64::from(HUNDREDTHS);
    for units in scale..=1000 * scale {
        let text = decimal_text(units, scale, 2);
        assert_eq!(exact_of(&text), Some(fraction(units, scale)), "{text}");
    }
}

#[test]
fn every_ten_thousandth_in_sample_ranges_is_exactly_its_decimal_text() {
    let scale = u64::from(TEN_THOUSANDTHS);
    let ranges = [(1, 2), (130, 140), (999, 1000)];
    for (low, high) in ranges {
        for units in low * scale..=high * scale {
            let text = decimal_text(units, scale, 4);
            assert_eq!(exact_of(&text), Some(fraction(units, scale)), "{text}");
        }
    }
}

#[test]
fn six_decimals_are_exact_and_seven_are_not() {
    let scale = u64::from(MILLIONTHS);
    for start in [scale, 120 * scale, 999 * scale] {
        for units in start..=start + scale / 10 {
            let text = decimal_text(units, scale, 6);
            assert_eq!(exact_of(&text), Some(fraction(units, scale)), "{text}");
        }
    }
    for start in [TEN_MILLIONTHS, 120 * TEN_MILLIONTHS, 998 * TEN_MILLIONTHS] {
        for units in (start + 1..=start + TEN_MILLIONTHS / 10).filter(|units| units % 10 != 0) {
            let text = decimal_text(units, TEN_MILLIONTHS, 7);
            assert_eq!(exact_of(&text), None, "{text}");
        }
    }
}

#[test]
fn a_seventeen_digit_tempo_has_no_exact_value_and_is_diagnosed() {
    for text in ["128.48806208011501", "1.6506591711229999", "120.0000001"] {
        assert_eq!(exact_of(text), None, "{text}");
    }
    assert_eq!(exact_of("1000"), Some(Fraction::from_integer(1000)));
    assert_eq!(exact_of("1"), Some(Fraction::from_integer(1)));
    assert_eq!(exact_of("133.1234"), Some(Fraction::new(1_331_234, 10_000)));
}

const MILLION: i64 = 1_000_000;

#[test]
fn rounding_keeps_every_millionth_tempo_exact() {
    for millionths in (MILLION..=1000 * MILLION).step_by(9_973) {
        let tempo = Tempo::new(millionths as f64 / MILLION as f64).unwrap();
        let expected = Fraction::new(millionths, MILLION);
        assert_eq!(tempo.rounded(6), Some(expected), "{millionths}");
        assert_eq!(tempo.exact(), Some(expected), "{millionths}");
    }
}

#[test]
fn rounding_a_seventh_decimal_goes_half_up() {
    for tenths in (10 * MILLION..=10_000 * MILLION).step_by(99_991) {
        let tempo = Tempo::new(tenths as f64 / (10 * MILLION) as f64).unwrap();
        let expected = Fraction::new((tenths + 5) / 10, MILLION);
        assert_eq!(tempo.rounded(6), Some(expected), "{tenths}");
        if tenths % 10 != 0 {
            assert_eq!(tempo.exact(), None, "{tenths}");
        }
    }
    let noise = Tempo::new(100.000_000_000_000_01).unwrap();
    assert_eq!(noise.rounded(6), Some(Fraction::from_integer(100)));
    assert_eq!(noise.rounded(0), Some(Fraction::from_integer(100)));
    let half = Tempo::new(120.5).unwrap();
    assert_eq!(half.rounded(0), Some(Fraction::from_integer(121)));
    assert_eq!(half.rounded(1), Some(Fraction::new(241, 2)));
}
