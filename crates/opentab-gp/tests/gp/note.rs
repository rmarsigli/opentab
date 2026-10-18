use opentab_gp::{GpError, GpifProblem};
use opentab_model::{Accent, Finger, Fraction, HarmonicKind, NoteKind, Slide, Tap, Vibrato};

use crate::support::{Doc, gpif_path, note, note_payloads};

fn with_note(children: &str, properties: &str) -> Doc {
    Doc {
        note: children.to_string(),
        note_properties: properties.to_string(),
        ..Doc::default()
    }
}

fn flag(name: &str) -> String {
    format!("<Property name=\"{name}\"><Enable/></Property>")
}

fn float(name: &str, value: &str) -> String {
    format!("<Property name=\"{name}\"><Float>{value}</Float></Property>")
}

#[test]
fn strings_count_from_the_highest_in_the_model() {
    for (gpif, model) in [("0", 5), ("1", 4), ("5", 0)] {
        let doc = Doc {
            string: gpif.to_string(),
            ..Doc::default()
        };
        let score = doc.read();
        assert_eq!(
            note(&score).kind,
            NoteKind::Fretted {
                string: model,
                fret: 3
            }
        );
    }
}

#[test]
fn a_string_outside_the_tuning_is_rejected() {
    let error = Doc {
        string: "6".to_string(),
        ..Doc::default()
    }
    .error();
    assert_eq!(
        gpif_path(&error),
        "/GPIF/Notes/Note/Properties/Property[@name=\"String\"]"
    );
    assert!(matches!(
        error,
        GpError::Gpif {
            problem: GpifProblem::StringOutOfRange {
                string: 6,
                strings: 6
            },
            ..
        }
    ));
}

#[test]
fn element_effects_are_mapped() {
    let score = with_note(
        concat!(
            "<Vibrato>Wide</Vibrato><AntiAccent>Normal</AntiAccent><LeftFingering>C</LeftFingering>",
            "<LetRing/><Tie origin=\"true\" destination=\"false\"/><Accent>9</Accent>",
            "<RightFingering>P</RightFingering>"
        ),
        "",
    )
    .read();
    let effects = &note(&score).effects;
    assert_eq!(effects.vibrato, Some(Vibrato::Wide));
    assert!(effects.ghost);
    assert_eq!(effects.left_hand_finger, Some(Finger::Little));
    assert_eq!(effects.right_hand_finger, Some(Finger::Thumb));
    assert!(effects.let_ring);
    assert!(effects.tie_origin);
    assert!(!effects.tie_destination);
    assert_eq!(effects.accent, Some(Accent::Normal));
    assert!(effects.staccato);
}

#[test]
fn every_finger_code_is_mapped() {
    let codes = [
        ("Open", Finger::Open),
        ("P", Finger::Thumb),
        ("I", Finger::Index),
        ("M", Finger::Middle),
        ("A", Finger::Ring),
        ("C", Finger::Little),
    ];
    for (code, finger) in codes {
        let score = with_note(&format!("<LeftFingering>{code}</LeftFingering>"), "").read();
        assert_eq!(note(&score).effects.left_hand_finger, Some(finger));
    }
}

#[test]
fn accent_bits_the_model_cannot_hold_are_replayed() {
    for (bits, accent, staccato, replayed) in [
        ("1", None, true, false),
        ("4", Some(Accent::Heavy), false, false),
        ("8", Some(Accent::Normal), false, false),
        ("5", Some(Accent::Heavy), true, false),
        ("2", None, false, true),
        ("17", None, true, true),
        ("12", None, false, true),
    ] {
        let score = with_note(&format!("<Accent>{bits}</Accent>"), "").read();
        let effects = &note(&score).effects;
        assert_eq!(effects.accent, accent, "{bits}");
        assert_eq!(effects.staccato, staccato, "{bits}");
        let payloads = note_payloads(&score);
        let kept = payloads.iter().any(|payload| payload.contains("<Accent>"));
        assert_eq!(kept, replayed, "{bits}");
    }
    let score = with_note("<Accent>17</Accent>", "").read();
    assert!(
        note_payloads(&score).contains(
            &"<Keep parent=\"\" index=\"0\" guard=\"accent:none true\"><Accent>17</Accent></Keep>"
                .to_string()
        )
    );
}

#[test]
fn property_flags_are_mapped() {
    let properties = [flag("HopoOrigin"), flag("Muted"), flag("PalmMuted")].concat();
    let score = with_note("", &properties).read();
    let effects = &note(&score).effects;
    assert!(effects.hammer_on_pull_off);
    assert!(effects.dead);
    assert!(effects.palm_mute);
    assert!(note_payloads(&score).is_empty());
}

#[test]
fn hopo_destination_stays_opaque_beside_mapped_effects() {
    let properties = [flag("HopoOrigin"), flag("HopoDestination")].concat();
    let score = with_note("<Vibrato>Slight</Vibrato>", &properties).read();
    let effects = &note(&score).effects;
    assert!(effects.hammer_on_pull_off);
    assert_eq!(effects.vibrato, Some(Vibrato::Slight));
    assert_eq!(
        note_payloads(&score),
        vec![
            "<Keep parent=\"Properties\" index=\"3\"><Property name=\"HopoDestination\"><Enable/></Property></Keep>"
                .to_string()
        ]
    );
}

#[test]
fn single_slides_are_mapped_and_combinations_replayed() {
    let slides = [
        ("1", Slide::Shift),
        ("2", Slide::Legato),
        ("4", Slide::OutDownwards),
        ("8", Slide::OutUpwards),
        ("16", Slide::IntoFromBelow),
        ("32", Slide::IntoFromAbove),
    ];
    for (bits, slide) in slides {
        let property = format!("<Property name=\"Slide\"><Flags>{bits}</Flags></Property>");
        let score = with_note("", &property).read();
        assert_eq!(note(&score).effects.slide, Some(slide));
    }
    let score = with_note("", "<Property name=\"Slide\"><Flags>18</Flags></Property>").read();
    assert_eq!(note(&score).effects.slide, None);
    assert_eq!(
        note_payloads(&score),
        vec![
            "<Keep parent=\"Properties\" index=\"2\" guard=\"slide:none\"><Property name=\"Slide\"><Flags>18</Flags></Property></Keep>"
                .to_string()
        ]
    );
}

#[test]
fn harmonics_are_mapped() {
    let properties = [
        flag("Harmonic"),
        "<Property name=\"HarmonicType\"><HType>Artificial</HType></Property>".to_string(),
        "<Property name=\"HarmonicFret\"><HFret>5.800000</HFret></Property>".to_string(),
    ]
    .concat();
    let score = with_note("", &properties).read();
    let harmonic = note(&score).effects.harmonic.expect("harmonic");
    assert_eq!(harmonic.kind, HarmonicKind::Artificial);
    assert_eq!(harmonic.fret, Fraction::new(29, 5));
    let broken = [
        flag("Harmonic"),
        "<Property name=\"HarmonicType\"><HType>Laser</HType></Property>".to_string(),
    ]
    .concat();
    let score = with_note("", &broken).read();
    assert_eq!(note(&score).effects.harmonic, None);
    assert_eq!(note_payloads(&score).len(), 2);
    assert!(
        note_payloads(&score)
            .iter()
            .all(|payload| payload.contains("guard=\"harmonic:none\""))
    );
}

fn bend_properties(middle_value: &str, origin_offset: &str) -> String {
    [
        flag("Bended"),
        float("BendOriginOffset", origin_offset),
        float("BendOriginValue", "0.000000"),
        float("BendMiddleOffset1", "12.000000"),
        float("BendMiddleOffset2", "12.000000"),
        float("BendMiddleValue", middle_value),
        float("BendDestinationOffset", "100.000000"),
        float("BendDestinationValue", "100.000000"),
    ]
    .concat()
}

#[test]
fn bends_are_mapped_in_cents() {
    let score = with_note("", &bend_properties("37.500000", "0.000000")).read();
    let bend = note(&score).effects.bend.expect("bend");
    assert_eq!((bend.origin().offset(), bend.origin().value()), (0, 0));
    assert_eq!(
        (bend.middle_start().offset(), bend.middle_value()),
        (12, 75)
    );
    assert_eq!(bend.middle_end().offset(), 12);
    assert_eq!(
        (bend.destination().offset(), bend.destination().value()),
        (100, 200)
    );
    assert!(note_payloads(&score).is_empty());
}

#[test]
fn bends_the_model_cannot_hold_are_replayed() {
    for (middle, origin) in [
        ("37.250000", "0.000000"),
        ("50.000000", "12.500000"),
        ("700.000000", "0.000000"),
    ] {
        let score = with_note("", &bend_properties(middle, origin)).read();
        assert_eq!(note(&score).effects.bend, None);
        let payloads = note_payloads(&score);
        assert_eq!(payloads.len(), 8);
        assert!(
            payloads
                .iter()
                .all(|payload| payload.contains("guard=\"bend:none\""))
        );
    }
}

#[test]
fn taps_are_mapped() {
    let score = with_note("", &flag("Tapped")).read();
    assert_eq!(note(&score).effects.tap, Some(Tap::Right));
    let score = with_note("", &flag("LeftHandTapped")).read();
    assert_eq!(note(&score).effects.tap, Some(Tap::Left));
    let both = [flag("Tapped"), flag("LeftHandTapped")].concat();
    let score = with_note("", &both).read();
    assert_eq!(note(&score).effects.tap, None);
    assert!(
        note_payloads(&score)
            .iter()
            .all(|payload| payload.contains("guard=\"tap:none\""))
    );
}

#[test]
fn derived_pitch_is_replayed_while_its_inputs_are_unchanged() {
    let doc = Doc {
        track: "<Transpose><Chromatic>0</Chromatic><Octave>-1</Octave></Transpose>".to_string(),
        staff: "<Property name=\"CapoFret\"><Fret>1</Fret></Property>".to_string(),
        note_properties: "<Property name=\"Midi\"><Number>45</Number></Property>".to_string(),
        ..Doc::default()
    };
    let score = doc.read();
    assert_eq!(
        note_payloads(&score),
        vec![
            "<Keep parent=\"Properties\" index=\"2\" guard=\"pitch:fretted 5 3;tuning 64 59 55 50 45 40;capo 1;transpose 0 -1\"><Property name=\"Midi\"><Number>45</Number></Property></Keep>"
                .to_string()
        ]
    );
}

#[test]
fn derived_pitch_equal_to_the_computed_one_is_mapped() {
    let doc = Doc {
        track: "<Transpose><Chromatic>0</Chromatic><Octave>-1</Octave></Transpose>".to_string(),
        staff: "<Property name=\"CapoFret\"><Fret>1</Fret></Property>".to_string(),
        note_properties: concat!(
            "<Property name=\"Midi\"><Number>44</Number></Property>",
            "<Property name=\"ConcertPitch\"><Pitch><Step>G</Step><Accidental>#</Accidental><Octave>3</Octave></Pitch></Property>",
            "<Property name=\"TransposedPitch\"><Pitch><Step>G</Step><Accidental>#</Accidental><Octave>4</Octave></Pitch></Property>"
        )
        .to_string(),
        ..Doc::default()
    };
    assert_eq!(note_payloads(&doc.read()), Vec::<String>::new());
}

#[test]
fn pitched_notes_keep_their_instrument_articulation() {
    let score = with_note("<InstrumentArticulation>0</InstrumentArticulation>", "").read();
    assert_eq!(
        note_payloads(&score),
        vec!["<Keep parent=\"\" index=\"0\"><InstrumentArticulation>0</InstrumentArticulation></Keep>".to_string()]
    );
}
