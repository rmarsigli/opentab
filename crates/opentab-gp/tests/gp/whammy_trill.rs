use opentab_gp::{GpError, Refusal, read_gp, write_gp};
use opentab_model::{
    BendPoint, Diagnostic, NoteValue, Score, TremoloBar, Trill, TrillSpeed, diagnose,
};

use crate::anchors::gpif_text;
use crate::support::{Doc, beat, beat_payloads, note, note_payloads};

const DIVE: &str = "<Whammy originValue=\"0.000000\" middleValue=\"-50.000000\" destinationValue=\"-100.000000\" originOffset=\"0.000000\" middleOffset1=\"35.000000\" middleOffset2=\"35.000000\" destinationOffset=\"99.000000\"/>";
const SPEED_ID: &str = "688062467";

fn point(offset: u8, value: i16) -> BendPoint {
    BendPoint::new(offset, value).unwrap()
}

fn with_beat(beat: &str) -> Score {
    Doc {
        beat: beat.to_string(),
        ..Doc::default()
    }
    .read()
}

fn with_note(note: &str, staff: &str) -> Score {
    Doc {
        note: note.to_string(),
        staff: staff.to_string(),
        ..Doc::default()
    }
    .read()
}

fn written(score: &Score) -> String {
    gpif_text(&write_gp(score).expect("write")).replace('\n', "")
}

fn round_trip(score: &Score) -> Score {
    let again = read_gp(&write_gp(score).expect("write")).expect("read");
    assert_eq!(&again, score);
    again
}

fn speed(ticks: &str) -> String {
    format!(
        "<XProperties><XProperty id=\"{SPEED_ID}\"><Int>{ticks}</Int></XProperty></XProperties>"
    )
}

fn trill(fret: u8, value: NoteValue) -> Trill {
    Trill {
        fret,
        speed: TrillSpeed::new(value).unwrap(),
    }
}

#[test]
fn a_whammy_is_a_typed_tremolo_bar_in_cents() {
    let score = with_beat(DIVE);
    assert_eq!(
        beat(&score).effects.tremolo_bar,
        Some(
            TremoloBar::new(
                point(0, 0),
                point(35, -100),
                point(35, -100),
                point(99, -200)
            )
            .unwrap()
        )
    );
    assert_eq!(beat_payloads(&score), Vec::<String>::new());
    assert!(written(&score).contains(DIVE), "{}", written(&score));
    round_trip(&score);
}

#[test]
fn a_rising_whammy_keeps_its_sign_and_half_cents_stay_whole() {
    let rise = DIVE
        .replace("-50.000000", "12.500000")
        .replace("-100.000000", "37.500000");
    let bar = beat(&with_beat(&rise)).effects.tremolo_bar.expect("typed");
    assert_eq!(bar.middle_value(), 25);
    assert_eq!(bar.destination().value(), 75);
    assert!(written(&with_beat(&rise)).contains(&rise));
}

#[test]
fn a_whammy_in_another_spelling_is_typed_and_kept_until_it_changes() {
    let loose = DIVE.replace("originValue=\"0.000000\"", "originValue=\"0\"");
    let score = with_beat(&loose);
    let bar = beat(&score).effects.tremolo_bar.expect("typed");
    assert_eq!(bar.origin(), point(0, 0));
    assert_eq!(beat_payloads(&score).len(), 1);
    assert!(beat_payloads(&score)[0].contains("guard=\"tremolo-bar:0 0;35 -100;35 -100;99 -200\""));
    assert!(written(&score).contains(&loose));
    let mut edited = score.clone();
    edited.tracks[0].staves[0].bars[0].voices[0].beats[0]
        .effects
        .tremolo_bar =
        Some(TremoloBar::new(point(0, 0), point(50, 100), point(50, 100), point(100, 0)).unwrap());
    let text = written(&edited);
    assert!(!text.contains(&loose), "{text}");
    assert_eq!(text.matches("<Whammy ").count(), 1, "{text}");
    assert!(text.contains("middleValue=\"50.000000\""), "{text}");
}

#[test]
fn a_whammy_the_model_cannot_hold_stays_opaque_until_replaced() {
    for odd in [
        DIVE.replace("-50.000000", "-50.250000"),
        DIVE.replace("middleOffset1=\"35.000000\"", "middleOffset1=\"35.5\""),
        DIVE.replace(
            "middleOffset2=\"35.000000\"",
            "middleOffset2=\"101.000000\"",
        ),
        DIVE.replace(" destinationOffset=\"99.000000\"", ""),
    ] {
        let score = with_beat(&odd);
        assert_eq!(beat(&score).effects.tremolo_bar, None, "{odd}");
        assert!(written(&score).contains(&odd), "{odd}");
        round_trip(&score);
        let mut edited = score.clone();
        edited.tracks[0].staves[0].bars[0].voices[0].beats[0]
            .effects
            .tremolo_bar =
            Some(TremoloBar::new(point(0, 0), point(10, 0), point(20, 0), point(30, 0)).unwrap());
        let text = written(&edited);
        assert!(!text.contains(&odd), "{text}");
        assert_eq!(text.matches("<Whammy ").count(), 1, "{text}");
    }
}

#[test]
fn a_trill_names_its_auxiliary_fret_and_defaults_to_thirty_seconds() {
    let score = with_note("<Trill>45</Trill>", "");
    assert_eq!(
        note(&score).effects.trill,
        Some(trill(5, NoteValue::ThirtySecond))
    );
    assert_eq!(note_payloads(&score), Vec::<String>::new());
    let text = written(&score);
    assert!(text.contains("<Note id=\"0\"><Trill>45</Trill>"), "{text}");
    assert!(!text.contains(SPEED_ID), "{text}");
    round_trip(&score);
}

#[test]
fn a_trill_counts_the_capo_and_reads_every_speed() {
    let capo = "<Property name=\"CapoFret\"><Fret>2</Fret></Property>";
    for (ticks, value) in [
        ("120", NoteValue::Sixteenth),
        ("240", NoteValue::Eighth),
        ("480", NoteValue::Quarter),
    ] {
        let score = with_note(&format!("<Trill>47</Trill>{}", ""), capo);
        assert_eq!(
            note(&score).effects.trill,
            Some(trill(5, NoteValue::ThirtySecond))
        );
        let doc = Doc {
            note: "<Trill>47</Trill>".to_string(),
            staff: capo.to_string(),
            note_properties: String::new(),
            ..Doc::default()
        };
        let text = doc.render().replace(
            "</Properties></Note>",
            &format!("</Properties>{}</Note>", speed(ticks)),
        );
        let score = crate::support::read_gpif(&text);
        assert_eq!(note(&score).effects.trill, Some(trill(5, value)), "{ticks}");
        assert_eq!(note_payloads(&score), Vec::<String>::new(), "{ticks}");
        assert!(written(&score).contains(&speed(ticks)), "{ticks}");
        round_trip(&score);
    }
}

fn with_speed(trill_text: &str, xproperties: &str) -> Score {
    let doc = Doc {
        note: trill_text.to_string(),
        ..Doc::default()
    };
    let text = doc.render().replace(
        "</Properties></Note>",
        &format!("</Properties>{xproperties}</Note>"),
    );
    crate::support::read_gpif(&text)
}

#[test]
fn an_explicit_default_speed_is_kept_until_the_speed_changes() {
    let score = with_speed("<Trill>45</Trill>", &speed("60"));
    assert_eq!(
        note(&score).effects.trill,
        Some(trill(5, NoteValue::ThirtySecond))
    );
    assert!(written(&score).contains(&speed("60")));
    round_trip(&score);
    let mut faster = score.clone();
    faster.tracks[0].staves[0].bars[0].voices[0].beats[0].notes[0]
        .effects
        .trill = Some(trill(5, NoteValue::Sixteenth));
    let text = written(&faster);
    assert!(text.contains(&speed("120")), "{text}");
    assert!(!text.contains("<Int>60</Int>"), "{text}");
}

#[test]
fn a_speed_guitar_pro_cannot_play_leaves_the_trill_opaque() {
    for (trill_text, ticks) in [
        ("<Trill>45</Trill>", "960"),
        ("<Trill>45</Trill>", "100"),
        ("<Trill>39</Trill>", "120"),
        ("", "120"),
    ] {
        let score = with_speed(trill_text, &speed(ticks));
        assert_eq!(note(&score).effects.trill, None, "{trill_text} {ticks}");
        assert_eq!(diagnose(&score), Vec::new());
        let text = written(&score);
        assert!(text.contains(&speed(ticks)), "{text}");
        assert!(text.contains(trill_text), "{text}");
        round_trip(&score);
    }
}

#[test]
fn other_note_xproperties_stay_beside_the_speed() {
    let other = "<XProperty id=\"12345\"><Int>7</Int></XProperty>";
    let both = format!(
        "<XProperties>{other}<XProperty id=\"{SPEED_ID}\"><Int>240</Int></XProperty></XProperties>"
    );
    let score = with_speed("<Trill>45</Trill>", &both);
    assert_eq!(
        note(&score).effects.trill,
        Some(trill(5, NoteValue::Eighth))
    );
    let text = written(&score);
    assert!(text.contains(&both), "{text}");
    round_trip(&score);
    let alone = format!("<XProperties>{other}</XProperties>");
    let plain = with_speed("", &alone);
    assert_eq!(note(&plain).effects.trill, None);
    assert!(written(&plain).contains(&alone));
}

#[test]
fn the_writer_refuses_an_auxiliary_note_above_midi_127_and_diagnose_flags_it() {
    let mut score = with_note("<Trill>45</Trill>", "");
    let high = Trill {
        fret: 88,
        speed: TrillSpeed::new(NoteValue::Eighth).unwrap(),
    };
    score.tracks[0].staves[0].bars[0].voices[0].beats[0].notes[0]
        .effects
        .trill = Some(high);
    let id = note(&score).id;
    assert_eq!(
        diagnose(&score),
        vec![Diagnostic::TrillOutOfRange { note: id }]
    );
    match write_gp(&score) {
        Err(GpError::Unsupported { reason, path }) => {
            assert_eq!(reason, Refusal::TrillPitch { fret: 88 });
            assert!(path.ends_with("effects.trill"), "{path}");
        }
        other => panic!("{:?}", other.map(|bytes| bytes.len())),
    }
    score.tracks[0].staves[0].bars[0].voices[0].beats[0].notes[0]
        .effects
        .trill = Some(Trill { fret: 87, ..high });
    assert_eq!(diagnose(&score), Vec::new());
    round_trip(&score);
}
