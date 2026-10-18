use opentab_model::{
    Accent, Bend, BendPoint, Duration, ElementId, Extensions, Fraction, Harmonic, HarmonicKind,
    Instrument, InstrumentKind, Marker, ModelError, Note, NoteEffects, NoteKind, NoteValue,
    OpaquePayload, Pitch, Score, Slide, Staff, Stroke, StrokeDirection, Tempo, TempoChange, Track,
    Tuning, Tuplet, Vibrato, diagnose, from_json, to_json,
};
use serde_json::{Value, json};

use crate::support::{beat, four_four, master_bar, quarters};

fn rich_score() -> Score {
    let mut score = four_four(&quarters(4));
    score.meta.title = "Canção ação 🎸 ".repeat(800);
    score.meta.artist = "Björk 中文 \u{0}\t\"quoted\"\n".to_string();
    score.meta.notices = "x".repeat(10_000);
    let master = &mut score.master_bars[0];
    master.repeat_start = true;
    master.repeat_end = Some(3);
    master.alternate_endings = 0b101;
    master.marker = Some(Marker {
        text: "Refrão ♪".to_string(),
    });
    master.tempo_changes = vec![
        TempoChange {
            position: Fraction::new(0, 1),
            bpm: Tempo::new(120.0).unwrap(),
        },
        TempoChange {
            position: Fraction::new(3, 8),
            bpm: Tempo::new(97.123_456_789_012_34).unwrap(),
        },
    ];
    let first_voice = &mut score.tracks[0].staves[0].bars[0].voices[0];
    let first_beat = &mut first_voice.beats[0];
    first_beat.effects.tremolo_picking = Some(NoteValue::ThirtySecond);
    first_beat.effects.stroke = Some(Stroke {
        direction: StrokeDirection::Down,
        duration: NoteValue::SixtyFourth,
    });
    first_beat.effects.chord_name = Some("Am7♭5".to_string());
    first_beat.effects.text = Some("ritardando".to_string());
    first_beat.effects.fade_in = true;
    first_beat.effects.tap = true;
    first_beat.effects.slap = true;
    first_beat.effects.pop = true;
    first_beat.notes[0].effects = NoteEffects {
        tie_origin: true,
        tie_destination: true,
        ghost: true,
        dead: true,
        accent: Some(Accent::Heavy),
        palm_mute: true,
        let_ring: true,
        staccato: true,
        vibrato: Some(Vibrato::Wide),
        hammer_on_pull_off: true,
        slide: Some(Slide::IntoFromBelow),
        harmonic: Some(Harmonic {
            kind: HarmonicKind::Artificial,
            fret: Fraction::new(12, 5),
        }),
        bend: Some(Bend {
            points: vec![
                BendPoint {
                    offset: 0,
                    value: 0,
                },
                BendPoint {
                    offset: 100,
                    value: -48,
                },
            ],
        }),
        left_hand_finger: Some(2),
        right_hand_finger: Some(4),
    };
    first_voice.beats[1].duration =
        Duration::new(NoteValue::Eighth, 2, Some(Tuplet { num: 7, den: 8 })).unwrap();
    let drum_id = score.alloc();
    let staff_id = score.alloc();
    let drum_note = Note {
        id: score.alloc(),
        kind: NoteKind::Percussion { articulation: 42 },
        effects: NoteEffects::default(),
    };
    let drum_beat = beat(
        &mut score,
        Duration::new(NoteValue::Whole, 0, None).unwrap(),
        vec![drum_note],
    );
    let bar_id = score.alloc();
    let voice_id = score.alloc();
    score.tracks.push(Track {
        id: drum_id,
        name: "Bateria".to_string(),
        short_name: "Bat".to_string(),
        instrument: Instrument {
            kind: InstrumentKind::Percussion,
            midi_program: 0,
        },
        tuning: Tuning {
            strings: Vec::new(),
        },
        capo: 0,
        staves: vec![Staff {
            id: staff_id,
            bars: vec![opentab_model::Bar {
                id: bar_id,
                voices: vec![opentab_model::Voice {
                    id: voice_id,
                    beats: vec![drum_beat],
                }],
            }],
        }],
    });
    score.tracks[0].tuning.strings.push(Pitch(35));
    let mut extensions = Extensions::default();
    extensions.0.insert(
        ElementId(1),
        vec![OpaquePayload {
            format: "gp8:xml".to_string(),
            data: "<Bar attr=\"1\"/>".to_string(),
        }],
    );
    extensions.0.insert(ElementId(u64::MAX), Vec::new());
    score.extensions = extensions;
    score
}

fn document() -> Value {
    serde_json::from_str(&to_json(&rich_score())).unwrap()
}

fn rejects(value: &Value, needle: &str) {
    match from_json(&value.to_string()) {
        Err(ModelError::Json { message }) => {
            assert!(message.contains(needle), "{message} lacks {needle}")
        }
        other => panic!("expected a JSON error mentioning {needle}, got {other:?}"),
    }
}

#[test]
fn rich_score_round_trips_losslessly() {
    let score = rich_score();
    assert_eq!(diagnose(&score), Vec::new());
    let text = to_json(&score);
    assert_eq!(from_json(&text), Ok(score.clone()));
    assert_eq!(to_json(&from_json(&text).unwrap()), text);
}

#[test]
fn empty_score_round_trips() {
    let score = Score::new();
    let back = from_json(&to_json(&score)).unwrap();
    assert_eq!(back, score);
    assert_eq!(diagnose(&back), Vec::new());
}

#[test]
fn document_carries_version_and_score_fields_at_top_level() {
    let text = to_json(&Score::new());
    assert!(text.contains("\n  \"opentab_json\": 1,"), "{text}");
    let value: Value = serde_json::from_str(&text).unwrap();
    let keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        [
            "extensions",
            "ids",
            "master_bars",
            "meta",
            "opentab_json",
            "tracks"
        ]
    );
    assert_eq!(value["ids"], json!({"next": 1}));
}

#[test]
fn missing_version_is_rejected() {
    let mut value = document();
    value.as_object_mut().unwrap().remove("opentab_json");
    assert_eq!(
        from_json(&value.to_string()),
        Err(ModelError::MissingVersion)
    );
    value["opentab_json"] = Value::Null;
    assert_eq!(
        from_json(&value.to_string()),
        Err(ModelError::MissingVersion)
    );
}

#[test]
fn unknown_versions_are_rejected() {
    for (version, found) in [
        (json!(2), "2"),
        (json!(0), "0"),
        (json!("1"), "\"1\""),
        (json!(1.5), "1.5"),
    ] {
        let mut value = document();
        value["opentab_json"] = version;
        assert_eq!(
            from_json(&value.to_string()),
            Err(ModelError::UnsupportedVersion {
                found: found.to_string()
            })
        );
    }
}

#[test]
fn every_truncation_is_an_error() {
    let mut score = four_four(&quarters(1));
    score.meta.title = "ação 🎸".to_string();
    let text = to_json(&score);
    let mut checked = 0;
    for (index, _) in text.char_indices().skip(1) {
        assert!(
            from_json(&text[..index]).is_err(),
            "prefix {index} accepted"
        );
        checked += 1;
    }
    assert!(checked > 500);
}

#[test]
fn garbage_inputs_are_errors() {
    let inputs = [
        "",
        "\u{0}\u{1}\u{2}",
        "null",
        "42",
        "[]",
        "{",
        "{}",
        "\"opentab\"",
        "ÿÿÿ",
    ];
    for input in inputs {
        assert!(from_json(input).is_err(), "{input:?} accepted");
    }
    assert!(from_json(r#"{"opentab_json": 1}"#).is_err());
    assert!(from_json(&"[".repeat(100_000)).is_err());
    assert!(
        from_json(&format!(
            "{{\"opentab_json\": 1, \"meta\": {}",
            "[".repeat(100_000)
        ))
        .is_err()
    );
}

#[test]
fn tempo_out_of_range_is_rejected() {
    for bpm in [json!(0), json!(0.0), json!(1000.5), json!(-1)] {
        let mut value = document();
        value["master_bars"][0]["tempo_changes"][0]["bpm"] = bpm;
        rejects(&value, "tempo");
    }
}

#[test]
fn dots_four_is_rejected() {
    let mut value = document();
    value["tracks"][0]["staves"][0]["bars"][0]["voices"][0]["beats"][0]["duration"]["dots"] =
        json!(4);
    rejects(&value, "dots");
}

#[test]
fn tuplet_zero_is_rejected() {
    let mut value = document();
    value["tracks"][0]["staves"][0]["bars"][0]["voices"][0]["beats"][0]["duration"]["tuplet"] =
        json!({"num": 0, "den": 2});
    rejects(&value, "tuplet 0:2");
}

#[test]
fn zero_time_signature_is_rejected() {
    let mut value = document();
    value["master_bars"][0]["time_signature"]["denominator"] = json!(0);
    rejects(&value, "time signature");
}

#[test]
fn invalid_fractions_are_rejected() {
    for position in [json!([1, 0]), json!([-1, 4]), json!([1, -4])] {
        let mut value = document();
        value["master_bars"][0]["tempo_changes"][1]["position"] = position;
        rejects(&value, "fraction");
    }
    let mut value = document();
    let note = &mut value["tracks"][0]["staves"][0]["bars"][0]["voices"][0]["beats"][0]["notes"][0];
    note["effects"]["harmonic"]["fret"] = json!([12, 0]);
    rejects(&value, "fraction");
}

#[test]
fn unreduced_fraction_is_normalised() {
    let mut value = document();
    value["master_bars"][0]["tempo_changes"][1]["position"] = json!([6, 16]);
    let score = from_json(&value.to_string()).unwrap();
    let position = score.master_bars[0].tempo_changes[1].position;
    assert_eq!((*position.numer(), *position.denom()), (3, 8));
}

#[test]
fn out_of_range_integers_are_rejected() {
    let mut value = document();
    let note = &mut value["tracks"][0]["staves"][0]["bars"][0]["voices"][0]["beats"][0]["notes"][0];
    note["kind"]["string"] = json!(300);
    rejects(&value, "300");
    let mut value = document();
    value["tracks"][0]["capo"] = json!(-1);
    rejects(&value, "-1");
}

#[test]
fn missing_and_unknown_fields_are_rejected() {
    let mut value = document();
    value.as_object_mut().unwrap().remove("meta");
    rejects(&value, "meta");
    let mut value = document();
    value["tracks"][0].as_object_mut().unwrap().remove("tuning");
    rejects(&value, "tuning");
    let mut value = document();
    value["meta"]["titel"] = json!("typo");
    rejects(&value, "titel");
}

#[test]
fn dangling_and_duplicate_ids_load_and_are_diagnosed() {
    let mut value = document();
    value["ids"]["next"] = json!(2);
    value["tracks"][1]["id"] = value["tracks"][0]["id"].clone();
    let score = from_json(&value.to_string()).unwrap();
    let diagnostics = diagnose(&score);
    assert!(
        diagnostics.contains(&opentab_model::Diagnostic::DuplicateId {
            id: score.tracks[0].id.0
        })
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| matches!(d, opentab_model::Diagnostic::IdAboveAllocator { .. }))
    );
}

#[test]
fn mismatched_staff_from_json_is_diagnosed() {
    let mut value = document();
    value["master_bars"] = json!([]);
    let score = from_json(&value.to_string()).unwrap();
    let diagnostics = diagnose(&score);
    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics.iter().all(|d| matches!(
        d,
        opentab_model::Diagnostic::StaffBarCountMismatch { master_bars: 0, .. }
    )));
}

#[test]
fn extension_keys_survive_round_trip() {
    let score = rich_score();
    let back = from_json(&to_json(&score)).unwrap();
    assert_eq!(back.extensions, score.extensions);
    assert!(to_json(&score).contains("\"18446744073709551615\""));
    let mut extra = Score::new();
    let id = extra.alloc::<ElementId>();
    let master = master_bar(&mut extra, 3, 4);
    extra.master_bars.push(master);
    extra.extensions.0.insert(id, Vec::new());
    assert_eq!(from_json(&to_json(&extra)), Ok(extra));
}
