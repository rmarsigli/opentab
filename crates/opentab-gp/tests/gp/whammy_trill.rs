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

fn committed(name: &str) -> Score {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/gp")
        .join(name);
    read_gp(&std::fs::read(path).expect("fixture")).expect("fixture reads")
}

#[test]
fn the_playing_fixtures_read_back_typed_and_clean() {
    let ramps = committed("tempo-ramp.gp");
    let bars: Vec<usize> = ramps
        .master_bars
        .iter()
        .enumerate()
        .flat_map(|(index, master)| master.tempo_changes.iter().map(move |_| index))
        .collect();
    let linear: Vec<bool> = ramps
        .master_bars
        .iter()
        .flat_map(|master| master.tempo_changes.iter().map(|change| change.linear))
        .collect();
    assert_eq!(bars, [0, 2, 3, 4, 5]);
    assert_eq!(linear, [true, true, false, true, true]);
    let whammy = committed("tremolo-bar.gp");
    let bars: Vec<TremoloBar> = whammy.tracks[0].staves[0]
        .bars
        .iter()
        .flat_map(|bar| &bar.voices[0].beats)
        .filter_map(|beat| beat.effects.tremolo_bar)
        .collect();
    assert_eq!(bars.len(), 8);
    assert_eq!(bars.iter().filter(|bar| bar.moves()).count(), 7);
    assert!(bars.iter().any(|bar| bar.origin().value() < 0));
    assert!(bars.iter().any(|bar| bar.middle_value() == 25));
    let trills = committed("trill.gp");
    let speeds: Vec<NoteValue> = trills.tracks[0].staves[0]
        .bars
        .iter()
        .filter_map(|bar| bar.voices[0].beats[0].notes[0].effects.trill)
        .map(|trill| trill.speed.value())
        .collect();
    assert_eq!(
        speeds,
        vec![
            NoteValue::ThirtySecond,
            NoteValue::Sixteenth,
            NoteValue::Eighth,
            NoteValue::Quarter
        ]
    );
    for score in [&ramps, &whammy, &trills] {
        assert_eq!(diagnose(score), Vec::new());
        assert!(score.extensions.0.is_empty());
    }
}

fn first_note_mut(score: &mut Score) -> &mut opentab_model::Note {
    &mut score.tracks[0].staves[0].bars[0].voices[0].beats[0].notes[0]
}

fn note_text(score: &Score, id: &str) -> String {
    let text = written(score);
    let start = text
        .find(&format!("<Note id=\"{id}\">"))
        .expect("the note is written");
    let end = text[start..].find("</Note>").expect("the note closes") + start;
    text[start..end].to_string()
}

#[test]
fn a_trill_speed_set_on_a_note_with_other_xproperties_joins_their_container() {
    let fixture = committed("trill.gp");
    let gpif = written(&fixture);
    let start = gpif.find("<Note id=\"0\">").expect("note 0");
    let end = gpif[start..].find("</Note>").expect("end") + start;
    let source = gpif[start..end].replace("<Trill>73</Trill>", "").replace(
        "</Properties>",
        "</Properties><XProperties><XProperty id=\"12345\"><Int>7</Int></XProperty></XProperties>",
    );
    let edited_text = format!("{}{source}{}", &gpif[..start], &gpif[end..]);
    let mut score = crate::support::read_gpif(&edited_text);
    assert_eq!(note(&score).effects.trill, None);
    let id = note(&score).id;
    opentab_command::apply(
        &mut score,
        &opentab_command::Command::SetTrill {
            note: id,
            trill: Some(trill(2, NoteValue::Sixteenth)),
        },
    )
    .expect("set_trill applies");
    let text = note_text(&score, "0");
    assert_eq!(text.matches("<XProperties>").count(), 1, "{text}");
    assert!(
        text.contains("<XProperty id=\"12345\"><Int>7</Int></XProperty>")
            && text.contains(&format!(
                "<XProperty id=\"{SPEED_ID}\"><Int>120</Int></XProperty>"
            )),
        "{text}"
    );
    assert!(
        text.starts_with("<Note id=\"0\"><Trill>68</Trill>"),
        "{text}"
    );
    let again = round_trip(&score);
    assert_eq!(
        note(&again).effects.trill,
        Some(trill(2, NoteValue::Sixteenth))
    );
}

#[test]
fn every_xproperties_container_of_a_note_is_read_and_written_as_one() {
    let other = "<XProperty id=\"12345\"><Int>7</Int></XProperty>";
    let two = format!(
        "<XProperties>{other}</XProperties><XProperties><XProperty id=\"{SPEED_ID}\"><Int>240</Int></XProperty></XProperties>"
    );
    let score = with_speed("<Trill>45</Trill>", &two);
    assert_eq!(
        note(&score).effects.trill,
        Some(trill(5, NoteValue::Eighth))
    );
    let text = note_text(&score, "0");
    assert_eq!(text.matches("<XProperties>").count(), 1, "{text}");
    assert!(
        text.contains(other) && text.contains("<Int>240</Int>"),
        "{text}"
    );
    let again = read_gp(&write_gp(&score).unwrap()).unwrap();
    assert_eq!(
        note(&again).effects.trill,
        Some(trill(5, NoteValue::Eighth))
    );
    assert_eq!(read_gp(&write_gp(&again).unwrap()).unwrap(), again);
    let twice = format!(
        "<XProperties>{}</XProperties><XProperties>{}</XProperties>",
        speed("120")
            .replace("<XProperties>", "")
            .replace("</XProperties>", ""),
        speed("240")
            .replace("<XProperties>", "")
            .replace("</XProperties>", "")
    );
    let ambiguous = with_speed("<Trill>45</Trill>", &twice);
    assert_eq!(note(&ambiguous).effects.trill, None);
    let empty = with_speed("", "<XProperties/>");
    let mut trilled = empty.clone();
    first_note_mut(&mut trilled).effects.trill = Some(trill(5, NoteValue::Quarter));
    let text = note_text(&trilled, "0");
    assert_eq!(text.matches("<XProperties").count(), 1, "{text}");
    assert!(text.contains("<Int>480</Int>"), "{text}");
}

#[test]
fn a_trill_added_to_a_guitar_pro_note_is_its_first_child() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../goldens/gp/durations.gp/resave.gpif");
    let gpif = std::fs::read_to_string(path).expect("golden resave");
    let mut score = crate::support::read_gpif(&gpif);
    let mut edited = 0;
    for track in &mut score.tracks {
        for staff in &mut track.staves {
            for bar in &mut staff.bars {
                for voice in &mut bar.voices {
                    for beat in &mut voice.beats {
                        for note in &mut beat.notes {
                            note.effects.trill = Some(trill(1, NoteValue::Sixteenth));
                            edited += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(edited > 0);
    let text = gpif_text(&write_gp(&score).expect("write"));
    let notes = crate::inventory::note_children(&text);
    assert_eq!(notes.len(), edited);
    for children in &notes {
        assert_eq!(
            children.first().map(String::as_str),
            Some("Trill"),
            "{children:?}"
        );
        assert_eq!(crate::inventory::out_of_note_order(children), None);
    }
    assert!(notes.iter().any(|children| {
        children
            == &[
                "Trill",
                "InstrumentArticulation",
                "Properties",
                "XProperties",
            ]
            .map(str::to_string)
            .to_vec()
    }));
    let again = read_gp(&write_gp(&score).unwrap()).unwrap();
    assert_eq!(again, read_gp(&write_gp(&again).unwrap()).unwrap());
}

#[test]
fn a_trill_on_a_staff_with_its_own_tuning_counts_from_the_track_tuning() {
    let gpif = crate::track::two_staff_document(concat!(
        "<Property name=\"Tuning\"><Pitches>28 33 38 43</Pitches></Property>",
        "<Property name=\"CapoFret\"><Fret>3</Fret></Property>"
    ))
    .replace(
        "<Note id=\"1\"><Properties>",
        "<Note id=\"1\"><Trill>55</Trill><Properties>",
    );
    let score = crate::support::read_gpif(&gpif);
    let track = &score.tracks[0];
    let second = &track.staves[1].bars[0].voices[0].beats[0].notes[0];
    assert_eq!(
        second.effects.trill,
        Some(trill(5, NoteValue::ThirtySecond))
    );
    assert_eq!(track.trill_pitch_of(second).map(|pitch| pitch.0), Some(55));
    assert!(written(&score).contains("<Note id=\"1\"><Trill>55</Trill>"));
    round_trip(&score);
    let set = |fret: u8| {
        let mut changed = score.clone();
        changed.tracks[0].staves[1].bars[0].voices[0].beats[0].notes[0]
            .effects
            .trill = Some(trill(fret, NoteValue::Eighth));
        changed
    };
    assert_eq!(diagnose(&set(77)), Vec::new());
    assert!(write_gp(&set(77)).is_ok());
    let high = set(78);
    assert_eq!(
        diagnose(&high),
        vec![Diagnostic::TrillOutOfRange { note: second.id }]
    );
    assert!(matches!(
        write_gp(&high),
        Err(GpError::Unsupported {
            reason: Refusal::TrillPitch { fret: 78 },
            ..
        })
    ));
}
