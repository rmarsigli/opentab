use opentab_command::{Command, MetaField, apply};
use opentab_model::{
    Bar, BarId, BeatId, Clef, ElementId, NoteId, NoteValue, StaffId, TimeSignature, TrackId,
    Tuplet, Voice, VoiceId, VoiceSlot,
};
use serde_json::{Value, json};

use crate::support::{duration, fixture};

fn shape(command: &Command, expected: Value) {
    assert_eq!(serde_json::to_value(command).unwrap(), expected);
    let back: Command = serde_json::from_value(expected).unwrap();
    assert_eq!(&back, command);
}

#[test]
fn forward_commands_have_the_documented_json_shape() {
    let id = ElementId(7);
    shape(
        &Command::SetMeta {
            field: MetaField::TabAuthor,
            value: "Ana".to_owned(),
        },
        json!({"op": "set_meta", "field": "tab_author", "value": "Ana"}),
    );
    shape(
        &Command::InsertMasterBar {
            index: 2,
            time_signature: TimeSignature::new(6, 8).unwrap(),
        },
        json!({"op": "insert_master_bar", "index": 2,
               "time_signature": {"numerator": 6, "denominator": 8}}),
    );
    shape(
        &Command::RemoveMasterBar { index: 0 },
        json!({"op": "remove_master_bar", "index": 0}),
    );
    shape(
        &Command::InsertBeat {
            voice: VoiceId(id),
            index: 1,
            duration: opentab_model::Duration::new(
                NoteValue::Eighth,
                1,
                Some(Tuplet::new(3, 2).unwrap()),
            )
            .unwrap(),
        },
        json!({"op": "insert_beat", "voice": 7, "index": 1,
               "duration": {"value": "eighth", "dots": 1, "tuplet": {"num": 3, "den": 2}}}),
    );
    shape(
        &Command::RemoveBeat { beat: BeatId(id) },
        json!({"op": "remove_beat", "beat": 7}),
    );
    shape(
        &Command::SetBeatDuration {
            beat: BeatId(id),
            duration: duration(NoteValue::Half),
        },
        json!({"op": "set_beat_duration", "beat": 7,
               "duration": {"value": "half", "dots": 0, "tuplet": null}}),
    );
    shape(
        &Command::AddNote {
            beat: BeatId(id),
            string: 0,
            fret: 12,
        },
        json!({"op": "add_note", "beat": 7, "string": 0, "fret": 12}),
    );
    shape(
        &Command::RemoveNote { note: NoteId(id) },
        json!({"op": "remove_note", "note": 7}),
    );
    shape(
        &Command::SetFret {
            note: NoteId(id),
            fret: 3,
        },
        json!({"op": "set_fret", "note": 7, "fret": 3}),
    );
    shape(
        &Command::SetTempo {
            master_bar: 0,
            bpm: 96.5,
        },
        json!({"op": "set_tempo", "master_bar": 0, "bpm": 96.5}),
    );
    shape(
        &Command::PadStaffBars {
            track: TrackId(ElementId(3)),
            staff: StaffId(id),
        },
        json!({"op": "pad_staff_bars", "track": 3, "staff": 7}),
    );
    shape(
        &Command::TrimStaffBars {
            track: TrackId(ElementId(3)),
            staff: StaffId(id),
            len: 2,
        },
        json!({"op": "trim_staff_bars", "track": 3, "staff": 7, "len": 2}),
    );
}

#[test]
fn restore_staff_bars_has_the_documented_json_shape() {
    let bar = Bar {
        id: BarId(ElementId(8)),
        clef: Clef::Treble,
        voices: vec![Voice {
            id: VoiceId(ElementId(9)),
            slot: VoiceSlot::default(),
            beats: Vec::new(),
        }],
    };
    shape(
        &Command::RestoreStaffBars {
            track: TrackId(ElementId(3)),
            staff: StaffId(ElementId(7)),
            bars: vec![bar],
            extensions: Vec::new(),
        },
        json!({"op": "restore_staff_bars", "track": 3, "staff": 7,
               "bars": [{"id": 8, "clef": "treble", "voices": [{"id": 9, "slot": 0, "beats": []}]}],
               "extensions": []}),
    );
}

#[test]
fn every_meta_field_is_snake_case() {
    let names = [
        (MetaField::Title, "title"),
        (MetaField::Subtitle, "subtitle"),
        (MetaField::Artist, "artist"),
        (MetaField::Album, "album"),
        (MetaField::Words, "words"),
        (MetaField::Music, "music"),
        (MetaField::Copyright, "copyright"),
        (MetaField::TabAuthor, "tab_author"),
        (MetaField::Instructions, "instructions"),
        (MetaField::Notices, "notices"),
    ];
    for (field, name) in names {
        assert_eq!(serde_json::to_value(field).unwrap(), json!(name));
    }
}

#[test]
fn restore_commands_have_the_documented_json_shape() {
    let mut fx = fixture();
    let ids = fx.score.ids.clone();
    let inverse = apply(
        &mut fx.score,
        &Command::AddNote {
            beat: fx.beats[1],
            string: 1,
            fret: 1,
        },
    )
    .unwrap();
    let fresh = ids.peek().0;
    shape(&inverse, json!({"op": "remove_note", "note": fresh}));
    let tempo = apply(
        &mut fx.score,
        &Command::SetTempo {
            master_bar: 1,
            bpm: 80.0,
        },
    )
    .unwrap();
    shape(
        &tempo,
        json!({"op": "restore_tempo_changes", "master_bar": 1,
               "tempo_changes": [{"position": [0, 1], "bpm": 120.0, "unit": "quarter"}]}),
    );
    let note = fx.notes[1];
    let note_raw = ElementId::from(note).0;
    let beat_raw = ElementId::from(fx.beats[0]).0;
    let effects = serde_json::to_value(opentab_model::NoteEffects::default()).unwrap();
    let restore = apply(&mut fx.score, &Command::RemoveNote { note }).unwrap();
    shape(
        &restore,
        json!({"op": "restore_note", "beat": beat_raw, "index": 1,
               "note": {"id": note_raw, "kind": {"type": "fretted", "string": 2, "fret": 5},
                        "effects": effects},
               "extensions": [{"id": note_raw, "payloads": [{"format": "gp", "data": "note"}]}]}),
    );
}

#[test]
fn every_inverse_survives_a_json_round_trip() {
    let mut fx = fixture();
    let commands = [
        Command::RemoveMasterBar { index: 1 },
        Command::RemoveBeat { beat: fx.beats[0] },
        Command::InsertMasterBar {
            index: 0,
            time_signature: TimeSignature::new(7, 8).unwrap(),
        },
    ];
    for command in &commands {
        let inverse = apply(&mut fx.score, command).unwrap();
        let text = serde_json::to_string(&inverse).unwrap();
        let back: Command = serde_json::from_str(&text).unwrap();
        assert_eq!(back, inverse);
    }
}

#[test]
fn unknown_fields_and_ops_are_rejected() {
    let extra = r#"{"op": "remove_beat", "beat": 1, "extra": true}"#;
    assert!(serde_json::from_str::<Command>(extra).is_err());
    let unknown = r#"{"op": "explode", "beat": 1}"#;
    assert!(serde_json::from_str::<Command>(unknown).is_err());
    let missing = r#"{"op": "add_note", "beat": 1, "string": 0}"#;
    assert!(serde_json::from_str::<Command>(missing).is_err());
    let wide = r#"{"op": "add_note", "beat": 1, "string": 256, "fret": 0}"#;
    assert!(serde_json::from_str::<Command>(wide).is_err());
    let zero = r#"{"op": "insert_master_bar", "index": 0, "time_signature": {"numerator": 0, "denominator": 4}}"#;
    assert!(serde_json::from_str::<Command>(zero).is_err());
    let dots = r#"{"op": "set_beat_duration", "beat": 1, "duration": {"value": "half", "dots": 4, "tuplet": null}}"#;
    assert!(serde_json::from_str::<Command>(dots).is_err());
}
