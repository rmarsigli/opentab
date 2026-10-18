use opentab_model::{
    Accent, Base64Bytes, Bend, BendPoint, Clef, ContainerEntry, Diagnostic, Duration, Dynamic,
    ElementId, Extensions, Finger, Fraction, Grace, Harmonic, HarmonicKind, Instrument,
    InstrumentKind, Marker, ModelError, Note, NoteEffects, NoteKind, NoteValue, OpaquePayload,
    Pitch, Score, Slide, Staff, Stroke, StrokeDirection, StrokeTicks, Tap, Tempo, TempoChange,
    TempoUnit, Track, Transpose, TremoloPicking, Tuning, Tuplet, Vibrato, VoiceSlot, diagnose,
    from_json, to_json,
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
        letter: "A".to_string(),
    });
    master.double_bar = true;
    score.anacrusis = true;
    master.tempo_changes = vec![
        TempoChange {
            position: Fraction::new(0, 1),
            bpm: Tempo::new(120.0).unwrap(),
            unit: TempoUnit::DottedQuarter,
        },
        TempoChange {
            position: Fraction::new(3, 8),
            bpm: Tempo::new(97.123_456_789_012_34).unwrap(),
            unit: TempoUnit::Quarter,
        },
    ];
    let first_voice = &mut score.tracks[0].staves[0].bars[0].voices[0];
    let first_beat = &mut first_voice.beats[0];
    first_beat.effects.tremolo_picking =
        Some(TremoloPicking::new(NoteValue::ThirtySecond).unwrap());
    first_beat.effects.stroke = Some(Stroke {
        direction: StrokeDirection::Down,
        duration: StrokeTicks::new(147).unwrap(),
    });
    first_beat.effects.chord_name = Some("Am7♭5".to_string());
    first_beat.effects.text = Some("ritardando".to_string());
    first_beat.effects.fade_in = true;
    first_beat.effects.grace = Some(Grace::OnBeat);
    first_beat.effects.dynamic = Dynamic::Ff;
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
        bend: Some(
            Bend::new(
                BendPoint::new(0, 0).unwrap(),
                BendPoint::new(25, 150).unwrap(),
                BendPoint::new(75, 150).unwrap(),
                BendPoint::new(100, -1200).unwrap(),
            )
            .unwrap(),
        ),
        tap: Some(Tap::Left),
        left_hand_finger: Some(Finger::Open),
        right_hand_finger: Some(Finger::Little),
    };
    first_voice.beats[1].duration =
        Duration::new(NoteValue::Eighth, 2, Some(Tuplet::new(7, 8).unwrap())).unwrap();
    let drum_id = score.alloc().unwrap();
    let staff_id = score.alloc().unwrap();
    let drum_note = Note {
        id: score.alloc().unwrap(),
        kind: NoteKind::Percussion { articulation: 42 },
        effects: NoteEffects::default(),
    };
    let drum_beat = beat(
        &mut score,
        Duration::new(NoteValue::Whole, 0, None).unwrap(),
        vec![drum_note],
    );
    let bar_id = score.alloc().unwrap();
    let voice_id = score.alloc().unwrap();
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
        transpose: Transpose::new(-2, -1).unwrap(),
        staves: vec![Staff {
            id: staff_id,
            bars: vec![opentab_model::Bar {
                id: bar_id,
                clef: Clef::Neutral,
                voices: vec![opentab_model::Voice {
                    id: voice_id,
                    slot: VoiceSlot::new(2).unwrap(),
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
    score.document_extensions = vec![OpaquePayload {
        format: "gp8:root".to_string(),
        data: "<GPRevision/>".to_string(),
    }];
    score.container = vec![
        ContainerEntry::File {
            name: "Content/PartConfiguration".to_string(),
            data: Base64Bytes(Vec::new()),
        },
        ContainerEntry::Score {
            name: "Content/score.gpif".to_string(),
        },
        ContainerEntry::File {
            name: "Content/Assets/\u{e9}.bin".to_string(),
            data: Base64Bytes((0..=255).collect()),
        },
    ];
    score
}

fn document() -> Value {
    serde_json::from_str(&to_json(&rich_score()).unwrap()).unwrap()
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
    assert_eq!(
        diagnose(&score),
        vec![Diagnostic::OrphanExtension {
            id: ElementId(u64::MAX)
        }]
    );
    let text = to_json(&score).unwrap();
    assert_eq!(from_json(&text), Ok(score.clone()));
    assert_eq!(to_json(&from_json(&text).unwrap()).unwrap(), text);
}

#[test]
fn empty_score_round_trips() {
    let score = Score::new();
    let back = from_json(&to_json(&score).unwrap()).unwrap();
    assert_eq!(back, score);
    assert_eq!(diagnose(&back), Vec::new());
}

#[test]
fn document_carries_version_and_score_fields_at_top_level() {
    let text = to_json(&Score::new()).unwrap();
    assert!(text.contains("\n  \"opentab_json\": 2,"), "{text}");
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
            "anacrusis",
            "container",
            "document_extensions",
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
        (json!(1), "1"),
        (json!(3), "3"),
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
    let text = to_json(&score).unwrap();
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
    assert!(from_json(r#"{"opentab_json": 2}"#).is_err());
    assert!(from_json(&"[".repeat(100_000)).is_err());
    assert!(
        from_json(&format!(
            "{{\"opentab_json\": 2, \"meta\": {}",
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
    assert_eq!(diagnostics.len(), 4);
    let mismatches = diagnostics
        .iter()
        .filter(|d| matches!(d, Diagnostic::StaffBarCountMismatch { master_bars: 0, .. }))
        .count();
    assert_eq!(mismatches, 2);
    assert_eq!(
        &diagnostics[2..],
        &[
            Diagnostic::OrphanExtension { id: ElementId(1) },
            Diagnostic::OrphanExtension {
                id: ElementId(u64::MAX)
            },
        ]
    );
}

#[test]
fn extension_keys_survive_round_trip() {
    let score = rich_score();
    let back = from_json(&to_json(&score).unwrap()).unwrap();
    assert_eq!(back.extensions, score.extensions);
    assert!(
        to_json(&score)
            .unwrap()
            .contains("\"18446744073709551615\"")
    );
    let mut extra = Score::new();
    let id = extra.alloc::<ElementId>().unwrap();
    let master = master_bar(&mut extra, 3, 4);
    extra.master_bars.push(master);
    extra.extensions.0.insert(id, Vec::new());
    assert_eq!(from_json(&to_json(&extra).unwrap()), Ok(extra));
}

#[test]
fn container_serialises_as_base64_text() {
    let value = document();
    assert_eq!(value["container"][0]["data"], json!(""));
    assert_eq!(value["container"][2]["data"].as_str().unwrap().len(), 344);
    assert!(
        value["container"][2]["data"]
            .as_str()
            .unwrap()
            .starts_with("AAECAwQF")
    );
}

#[test]
fn invalid_base64_is_rejected() {
    for data in ["!!!!", "AAE", "AAF=", "AA==AA==", " AAAA"] {
        let mut value = document();
        value["container"][2]["data"] = json!(data);
        rejects(&value, "base64");
    }
    let mut value = document();
    value["container"][2]["data"] = json!(7);
    assert!(from_json(&value.to_string()).is_err());
}

#[test]
fn new_fields_are_required_and_closed() {
    for pointer in [
        "/container",
        "/document_extensions",
        "/anacrusis",
        "/master_bars/0/double_bar",
        "/master_bars/0/marker/letter",
        "/master_bars/0/tempo_changes/0/unit",
        "/tracks/0/transpose",
        "/tracks/0/staves/0/bars/0/clef",
        "/tracks/0/staves/0/bars/0/voices/0/slot",
        "/tracks/0/staves/0/bars/0/voices/0/beats/0/effects/dynamic",
    ] {
        let mut value = document();
        let (parent, key) = pointer.rsplit_once('/').unwrap();
        let target = value.pointer_mut(parent).unwrap().as_object_mut().unwrap();
        assert!(target.remove(key).is_some(), "{pointer}");
        assert!(from_json(&value.to_string()).is_err(), "{pointer}");
    }
    let mut value = document();
    value["container"][0]["extra"] = json!(1);
    rejects(&value, "extra");
    let mut value = document();
    value["container"][1]["data"] = json!("AA==");
    rejects(&value, "data");
}

#[test]
fn new_values_are_validated_on_load() {
    let beat = "/tracks/0/staves/0/bars/0/voices/0/beats/0";
    let cases = [
        (
            "/tracks/0/staves/0/bars/0/voices/0/slot",
            json!(4),
            "voice slot",
        ),
        (
            "/tracks/0/transpose",
            json!({"chromatic": 13, "octave": 0}),
            "transpose",
        ),
        (
            "/tracks/0/transpose",
            json!({"chromatic": 0, "octave": -4}),
            "transpose",
        ),
        ("/tracks/0/staves/0/bars/0/clef", json!("alto"), "alto"),
        (
            "/master_bars/0/tempo_changes/0/unit",
            json!("whole"),
            "whole",
        ),
    ];
    for (pointer, replacement, needle) in cases {
        let mut value = document();
        *value.pointer_mut(pointer).unwrap() = replacement;
        rejects(&value, needle);
    }
    for (suffix, replacement, needle) in [
        ("/effects/stroke/duration", json!(29), "stroke duration"),
        ("/effects/stroke/duration", json!(481), "stroke duration"),
        ("/effects/tremolo_picking", json!("sixty_fourth"), "tremolo"),
        ("/effects/tremolo_picking", json!("quarter"), "tremolo"),
        ("/effects/dynamic", json!("sfz"), "sfz"),
        ("/effects/grace", json!("after_beat"), "after_beat"),
    ] {
        let mut value = document();
        *value.pointer_mut(&format!("{beat}{suffix}")).unwrap() = replacement;
        rejects(&value, needle);
    }
    let note = format!("{beat}/notes/0/effects");
    for (suffix, replacement, needle) in [
        ("/left_hand_finger", json!(2), "expected"),
        ("/right_hand_finger", json!("pinky"), "pinky"),
        ("/tap", json!("both"), "both"),
        ("/bend/origin/value", json!(1201), "bend point"),
        ("/bend/middle_end/value", json!(0), "bend middle"),
    ] {
        let mut value = document();
        *value.pointer_mut(&format!("{note}{suffix}")).unwrap() = replacement;
        rejects(&value, needle);
    }
    let mut value = document();
    value.pointer_mut(&format!("{note}/bend")).unwrap()["points"] = json!([]);
    rejects(&value, "points");
}

#[test]
fn container_entries_use_a_kind_tag() {
    let value = document();
    assert_eq!(value["container"][0]["kind"], json!("file"));
    assert_eq!(
        value["container"][1],
        json!({"kind": "score", "name": "Content/score.gpif"})
    );
}

#[test]
fn at_most_one_score_entry_is_allowed() {
    let mut score = rich_score();
    score.container.push(ContainerEntry::Score {
        name: "Other.gpif".to_string(),
    });
    assert_eq!(
        to_json(&score),
        Err(ModelError::InvalidContainer { scores: 2 })
    );
    let mut value = document();
    value["container"]
        .as_array_mut()
        .unwrap()
        .push(json!({"kind": "score", "name": "Other.gpif"}));
    assert_eq!(
        from_json(&value.to_string()),
        Err(ModelError::InvalidContainer { scores: 2 })
    );
    score.container.retain(|entry| !entry.is_score());
    assert_eq!(from_json(&to_json(&score).unwrap()), Ok(score));
}

#[test]
fn container_entry_names_are_reachable() {
    let score = rich_score();
    let names: Vec<&str> = score.container.iter().map(ContainerEntry::name).collect();
    assert_eq!(names[1], "Content/score.gpif");
}
