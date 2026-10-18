use opentab_gp::{GpError, GpifProblem};
use opentab_model::{Marker, ModelError};

use crate::support::{Doc, gpif_path, master_bar, payloads, read_gpif};

fn with_master_bar(extra: &str) -> Doc {
    Doc {
        master_bar: extra.to_string(),
        ..Doc::default()
    }
}

#[test]
fn time_and_key_signatures_are_mapped() {
    let gpif = Doc {
        time: "7/8".to_string(),
        ..Doc::default()
    }
    .render()
    .replace(
        "<Key><AccidentalCount>0</AccidentalCount><Mode>Major</Mode></Key>",
        "<Key><AccidentalCount>-3</AccidentalCount><Mode>Minor</Mode><TransposeAs>Flats</TransposeAs></Key>",
    );
    let score = read_gpif(&gpif);
    let bar = master_bar(&score);
    assert_eq!(bar.time_signature.numerator(), 7);
    assert_eq!(bar.time_signature.denominator(), 8);
    assert_eq!(bar.key_signature.accidentals(), -3);
    assert!(bar.key_signature.minor());
    assert_eq!(
        payloads(&score, bar.id.0),
        vec![
            "<Keep parent=\"Key\" index=\"2\"><TransposeAs>Flats</TransposeAs></Keep>".to_string()
        ]
    );
}

#[test]
fn an_unknown_mode_is_replayed_while_the_model_says_major() {
    let gpif = Doc::default()
        .render()
        .replace("<Mode>Major</Mode>", "<Mode>Dorian</Mode>");
    let score = read_gpif(&gpif);
    assert!(!master_bar(&score).key_signature.minor());
    assert_eq!(
        payloads(&score, master_bar(&score).id.0),
        vec![
            "<Keep parent=\"Key\" index=\"1\" guard=\"minor:false\"><Mode>Dorian</Mode></Keep>"
                .to_string()
        ]
    );
}

#[test]
fn repeats_endings_markers_and_double_bars_are_mapped() {
    let score = with_master_bar(concat!(
        "<Repeat start=\"true\" end=\"true\" count=\"3\"/>",
        "<Section><Letter><![CDATA[A]]></Letter><Text><![CDATA[Intro]]></Text></Section>",
        "<AlternateEndings>1 3</AlternateEndings><DoubleBar/>"
    ))
    .read();
    let bar = master_bar(&score);
    assert!(bar.repeat_start);
    assert_eq!(bar.repeat_end, Some(3));
    assert_eq!(bar.alternate_endings, 0b101);
    assert!(bar.double_bar);
    assert_eq!(
        bar.marker,
        Some(Marker {
            text: "Intro".to_string(),
            letter: "A".to_string(),
        })
    );
    assert!(payloads(&score, bar.id.0).is_empty());
}

#[test]
fn a_start_only_repeat_with_a_count_is_replayed() {
    let score = with_master_bar("<Repeat start=\"true\" end=\"false\" count=\"2\"/>").read();
    let bar = master_bar(&score);
    assert!(bar.repeat_start);
    assert_eq!(bar.repeat_end, None);
    assert_eq!(
        payloads(&score, bar.id.0),
        vec![
            "<Keep parent=\"\" index=\"2\" guard=\"repeat:true none\"><Repeat start=\"true\" end=\"false\" count=\"2\"/></Keep>"
                .to_string()
        ]
    );
    let plain = with_master_bar("<Repeat start=\"true\" end=\"false\" count=\"0\"/>").read();
    assert!(payloads(&plain, master_bar(&plain).id.0).is_empty());
}

#[test]
fn unknown_master_bar_children_keep_their_position() {
    let score = with_master_bar("<TripletFeel>Triplet8th</TripletFeel><Fermatas/>").read();
    assert_eq!(
        payloads(&score, master_bar(&score).id.0),
        vec![
            "<Keep parent=\"\" index=\"2\"><TripletFeel>Triplet8th</TripletFeel></Keep>"
                .to_string(),
            "<Keep parent=\"\" index=\"3\"><Fermatas/></Keep>".to_string(),
        ]
    );
}

#[test]
fn anacrusis_is_mapped() {
    let doc = Doc {
        master_track: "<Anacrusis/>".to_string(),
        ..Doc::default()
    };
    assert!(doc.read().anacrusis);
    assert!(!Doc::default().read().anacrusis);
}

#[test]
fn an_invalid_time_signature_names_its_path() {
    let error = Doc {
        time: "4/3".to_string(),
        ..Doc::default()
    }
    .error();
    assert_eq!(
        error,
        GpError::Gpif {
            entry: "Content/score.gpif".to_string(),
            path: "/GPIF/MasterBars/MasterBar/Time".to_string(),
            problem: GpifProblem::Model(ModelError::InvalidTimeSignature {
                numerator: 4,
                denominator: 3,
            }),
        }
    );
    assert_eq!(
        error.to_string(),
        "Content/score.gpif: /GPIF/MasterBars/MasterBar/Time: time signature 4/3 needs a non-zero numerator and a power-of-two denominator within 1..=64"
    );
}

#[test]
fn an_alternate_ending_outside_one_to_eight_is_rejected() {
    let error = with_master_bar("<AlternateEndings>9</AlternateEndings>").error();
    assert_eq!(
        gpif_path(&error),
        "/GPIF/MasterBars/MasterBar/AlternateEndings"
    );
}

#[test]
fn a_master_bar_without_time_is_rejected() {
    let gpif = Doc::default().render().replace("<Time>4/4</Time>", "");
    let error = crate::support::read_gpif_error(&gpif);
    assert_eq!(gpif_path(&error), "/GPIF/MasterBars/MasterBar");
}
