use opentab_model::{Clef, Dynamic, Grace, NoteValue, StrokeDirection};

use crate::support::{Doc, beat, beat_payloads, payloads, read_gpif};

fn with_beat(extra: &str) -> Doc {
    Doc {
        beat: extra.to_string(),
        ..Doc::default()
    }
}

#[test]
fn dynamic_grace_tremolo_fade_and_text_are_mapped() {
    let score = with_beat(concat!(
        "<GraceNotes>OnBeat</GraceNotes><Dynamic>FFF</Dynamic><Tremolo>1/4</Tremolo>",
        "<Fadding>FadeIn</Fadding><FreeText><![CDATA[let ring]]></FreeText>"
    ))
    .read();
    let effects = &beat(&score).effects;
    assert_eq!(effects.grace, Some(Grace::OnBeat));
    assert_eq!(effects.dynamic, Dynamic::Fff);
    assert_eq!(
        effects.tremolo_picking.map(|tremolo| tremolo.value()),
        Some(NoteValue::Sixteenth)
    );
    assert!(effects.fade_in);
    assert_eq!(effects.text.as_deref(), Some("let ring"));
    assert!(beat_payloads(&score).is_empty());
}

#[test]
fn every_dynamic_and_tremolo_code_is_mapped() {
    let dynamics = [
        ("PPP", Dynamic::Ppp),
        ("PP", Dynamic::Pp),
        ("P", Dynamic::P),
        ("MP", Dynamic::Mp),
        ("MF", Dynamic::Mf),
        ("F", Dynamic::F),
        ("FF", Dynamic::Ff),
        ("FFF", Dynamic::Fff),
    ];
    for (code, dynamic) in dynamics {
        let score = with_beat(&format!("<Dynamic>{code}</Dynamic>")).read();
        assert_eq!(beat(&score).effects.dynamic, dynamic);
    }
    let tremolos = [
        ("1/2", NoteValue::Eighth),
        ("1/4", NoteValue::Sixteenth),
        ("1/8", NoteValue::ThirtySecond),
    ];
    for (code, value) in tremolos {
        let score = with_beat(&format!("<Tremolo>{code}</Tremolo>")).read();
        assert_eq!(
            beat(&score)
                .effects
                .tremolo_picking
                .map(|tremolo| tremolo.value()),
            Some(value)
        );
    }
    let score = with_beat("<GraceNotes>BeforeBeat</GraceNotes>").read();
    assert_eq!(beat(&score).effects.grace, Some(Grace::BeforeBeat));
}

#[test]
fn values_the_model_cannot_hold_are_replayed_with_a_guard() {
    let score = with_beat(concat!(
        "<Dynamic>SFZ</Dynamic><Tremolo>1/16</Tremolo><Fadding>FadeOut</Fadding>",
        "<GraceNotes>Later</GraceNotes><TransposedPitchStemOrientation>Upward</TransposedPitchStemOrientation>"
    ))
    .read();
    let effects = &beat(&score).effects;
    assert_eq!(effects.dynamic, Dynamic::Mf);
    assert_eq!(effects.tremolo_picking, None);
    assert!(!effects.fade_in);
    assert_eq!(effects.grace, None);
    assert_eq!(
        beat_payloads(&score),
        vec![
            "<Keep parent=\"\" index=\"1\" guard=\"dynamic:MF\"><Dynamic>SFZ</Dynamic></Keep>".to_string(),
            "<Keep parent=\"\" index=\"2\" guard=\"tremolo:none\"><Tremolo>1/16</Tremolo></Keep>".to_string(),
            "<Keep parent=\"\" index=\"3\" guard=\"fade-in:false\"><Fadding>FadeOut</Fadding></Keep>".to_string(),
            "<Keep parent=\"\" index=\"4\" guard=\"grace:none\"><GraceNotes>Later</GraceNotes></Keep>".to_string(),
            "<Keep parent=\"\" index=\"5\"><TransposedPitchStemOrientation>Upward</TransposedPitchStemOrientation></Keep>".to_string(),
        ]
    );
}

#[test]
fn slap_pop_and_brush_strokes_are_mapped() {
    let score = with_beat(concat!(
        "<Properties><Property name=\"PrimaryPickupVolume\"><Float>0.5</Float></Property>",
        "<Property name=\"Slapped\"><Enable/></Property><Property name=\"Popped\"><Enable/></Property>",
        "<Property name=\"Brush\"><Direction>Up</Direction></Property></Properties>",
        "<XProperties><XProperty id=\"687935489\"><Int>57</Int></XProperty>",
        "<XProperty id=\"687935490\"><Float>1</Float></XProperty></XProperties>"
    ))
    .read();
    let effects = &beat(&score).effects;
    assert!(effects.slap);
    assert!(effects.pop);
    let stroke = effects.stroke.expect("stroke");
    assert_eq!(stroke.direction, StrokeDirection::Up);
    assert_eq!(stroke.duration.ticks(), 57);
    assert_eq!(
        beat_payloads(&score),
        vec![
            "<Keep parent=\"Properties\" index=\"0\"><Property name=\"PrimaryPickupVolume\"><Float>0.5</Float></Property></Keep>".to_string(),
            "<Keep parent=\"XProperties\" index=\"1\" guard=\"stroke:Up 57\"><XProperty id=\"687935490\"><Float>1</Float></XProperty></Keep>".to_string(),
        ]
    );
}

#[test]
fn a_brush_without_duration_takes_the_default_and_bad_ones_are_replayed() {
    let score = with_beat(
        "<Properties><Property name=\"Brush\"><Direction>Down</Direction></Property></Properties>",
    )
    .read();
    let stroke = beat(&score).effects.stroke.expect("stroke");
    assert_eq!(stroke.direction, StrokeDirection::Down);
    assert_eq!(stroke.duration.ticks(), 480);
    let score = with_beat(concat!(
        "<Properties><Property name=\"Brush\"><Direction>Down</Direction></Property></Properties>",
        "<XProperties><XProperty id=\"687935489\"><Int>960</Int></XProperty></XProperties>"
    ))
    .read();
    assert_eq!(beat(&score).effects.stroke, None);
    assert_eq!(
        beat_payloads(&score),
        vec![
            "<Keep parent=\"Properties\" index=\"0\" guard=\"stroke:none\"><Property name=\"Brush\"><Direction>Down</Direction></Property></Keep>".to_string(),
            "<Keep parent=\"XProperties\" index=\"0\" guard=\"stroke:none\"><XProperty id=\"687935489\"><Int>960</Int></XProperty></Keep>".to_string(),
        ]
    );
}

#[test]
fn chord_names_come_from_the_staff_diagram_collection() {
    let doc = Doc {
        beat: "<Chord>1</Chord>".to_string(),
        staff: concat!(
            "<Property name=\"DiagramCollection\"><Items><Item id=\"0\" name=\"E5\"/>",
            "<Item id=\"1\" name=\"A7(#9)\"/></Items></Property>"
        )
        .to_string(),
        ..Doc::default()
    };
    let score = doc.read();
    assert_eq!(beat(&score).effects.chord_name.as_deref(), Some("A7(#9)"));
    assert_eq!(
        beat_payloads(&score),
        vec![
            "<Keep parent=\"\" index=\"1\" guard=\"chord:=A7(#9)\"><Chord>1</Chord></Keep>"
                .to_string()
        ]
    );
    let missing = with_beat("<Chord>4</Chord>").read();
    assert_eq!(beat(&missing).effects.chord_name, None);
    assert_eq!(
        beat_payloads(&missing),
        vec![
            "<Keep parent=\"\" index=\"1\" guard=\"chord:none\"><Chord>4</Chord></Keep>"
                .to_string()
        ]
    );
}

#[test]
fn clefs_are_mapped_and_unknown_clefs_replayed() {
    for (code, clef) in [
        ("G2", Clef::Treble),
        ("F4", Clef::Bass),
        ("Neutral", Clef::Neutral),
    ] {
        let gpif = Doc::default()
            .render()
            .replace("<Clef>G2</Clef>", &format!("<Clef>{code}</Clef>"));
        let score = read_gpif(&gpif);
        assert_eq!(score.tracks[0].staves[0].bars[0].clef, clef);
    }
    let gpif = Doc::default()
        .render()
        .replace("<Clef>G2</Clef>", "<Clef>C3</Clef>");
    let score = read_gpif(&gpif);
    let bar = &score.tracks[0].staves[0].bars[0];
    assert_eq!(bar.clef, Clef::Treble);
    assert_eq!(
        payloads(&score, bar.id.0),
        vec!["<Keep parent=\"\" index=\"0\" guard=\"clef:G2\"><Clef>C3</Clef></Keep>".to_string()]
    );
}

#[test]
fn voices_keep_their_slots() {
    let gpif = Doc::default()
        .render()
        .replace("<Voices>0 -1 -1 -1</Voices>", "<Voices>-1 0 -1 1</Voices>")
        .replace(
            "<Voice id=\"0\"><Beats>0</Beats></Voice>",
            "<Voice id=\"0\"><Beats>0</Beats></Voice><Voice id=\"1\"><Beats/></Voice>",
        );
    let score = read_gpif(&gpif);
    let bar = &score.tracks[0].staves[0].bars[0];
    let slots: Vec<u8> = bar.voices.iter().map(|voice| voice.slot.get()).collect();
    assert_eq!(slots, vec![1, 3]);
    assert!(bar.voices[1].beats.is_empty());
    let empty = read_gpif(
        &Doc::default()
            .render()
            .replace("<Voices>0 -1 -1 -1</Voices>", "<Voices>-1 -1 -1 -1</Voices>")
            .replace("<Voices><Voice id=\"0\"><Beats>0</Beats></Voice></Voices>", "<Voices/>")
            .replace("<Beats><Beat id=\"0\"><Rhythm ref=\"0\"/><Notes>0</Notes></Beat></Beats>", "<Beats/>")
            .replace(
                "<Notes><Note id=\"0\"><Properties><Property name=\"String\"><String>0</String></Property><Property name=\"Fret\"><Fret>3</Fret></Property></Properties></Note></Notes>",
                "<Notes/>",
            ),
    );
    assert!(empty.tracks[0].staves[0].bars[0].voices.is_empty());
}
