use opentab_command::{Command, CommandError, apply};
use opentab_model::{BendPoint, NoteId, NoteValue, Score, TremoloBar, Trill, TrillSpeed, diagnose};
use serde_json::json;

use crate::checks::{rejected, round_trip};
use crate::support::fixture;

fn dive() -> TremoloBar {
    let point = |offset, value| BendPoint::new(offset, value).unwrap();
    TremoloBar::new(
        point(0, 0),
        point(40, -100),
        point(60, -100),
        point(100, -400),
    )
    .unwrap()
}

fn trill(fret: u8, speed: NoteValue) -> Trill {
    Trill {
        fret,
        speed: TrillSpeed::new(speed).unwrap(),
    }
}

fn trill_of(score: &Score, note: NoteId) -> Option<Trill> {
    let path = score.find_note(note).unwrap();
    score.tracks[path.beat.track].staves[path.beat.staff].bars[path.beat.bar].voices
        [path.beat.voice]
        .beats[path.beat.beat]
        .notes[path.note]
        .effects
        .trill
}

#[test]
fn set_tremolo_bar_sets_and_clears_the_curve_with_undo() {
    let mut fx = fixture();
    let beat = fx.beats[0];
    let command = Command::SetTremoloBar {
        beat,
        tremolo_bar: Some(dive()),
    };
    let inverse = round_trip(&mut fx.score, &command);
    assert_eq!(
        inverse,
        Command::SetTremoloBar {
            beat,
            tremolo_bar: None
        }
    );
    apply(&mut fx.score, &command).unwrap();
    let path = fx.score.find_beat(beat).unwrap();
    assert_eq!(
        fx.score.tracks[path.track].staves[0].bars[path.bar].voices[0].beats[path.beat]
            .effects
            .tremolo_bar,
        Some(dive())
    );
    let clear = Command::SetTremoloBar {
        beat,
        tremolo_bar: None,
    };
    assert_eq!(
        round_trip(&mut fx.score, &clear),
        Command::SetTremoloBar {
            beat,
            tremolo_bar: Some(dive())
        }
    );
    let missing = Command::SetTremoloBar {
        beat: fx.notes[0].0.into(),
        tremolo_bar: None,
    };
    assert!(matches!(
        rejected(&mut fx.score, &missing),
        CommandError::NotFound { what: "beat", .. }
    ));
}

#[test]
fn set_trill_sets_a_playable_trill_and_undoes() {
    let mut fx = fixture();
    let note = fx.notes[0];
    let command = Command::SetTrill {
        note,
        trill: Some(trill(5, NoteValue::Eighth)),
    };
    let inverse = round_trip(&mut fx.score, &command);
    assert_eq!(inverse, Command::SetTrill { note, trill: None });
    apply(&mut fx.score, &command).unwrap();
    assert_eq!(trill_of(&fx.score, note), Some(trill(5, NoteValue::Eighth)));
    assert_eq!(diagnose(&fx.score), Vec::new());
}

#[test]
fn set_trill_refuses_drums_and_auxiliary_notes_above_midi_127() {
    let mut fx = fixture();
    let drums = Command::SetTrill {
        note: fx.drum_note,
        trill: Some(trill(2, NoteValue::Sixteenth)),
    };
    assert_eq!(
        rejected(&mut fx.score, &drums),
        CommandError::WrongInstrument { track: fx.drums }
    );
    let path = fx.score.find_note(fx.notes[0]).unwrap();
    let open = fx.score.tracks[path.beat.track].tuning.strings[0].0;
    let highest = 127 - open;
    let fits = Command::SetTrill {
        note: fx.notes[0],
        trill: Some(trill(highest, NoteValue::Quarter)),
    };
    round_trip(&mut fx.score, &fits);
    let above = Command::SetTrill {
        note: fx.notes[0],
        trill: Some(trill(highest + 1, NoteValue::Quarter)),
    };
    assert!(matches!(
        rejected(&mut fx.score, &above),
        CommandError::InvalidValue { .. }
    ));
}

#[test]
fn set_trill_on_a_note_whose_trill_is_already_unplayable_is_refused() {
    let mut fx = fixture();
    let note = fx.notes[0];
    let path = fx.score.find_note(note).unwrap();
    fx.score.tracks[path.beat.track].staves[0].bars[path.beat.bar].voices[0].beats
        [path.beat.beat]
        .notes[path.note]
        .effects
        .trill = Some(trill(200, NoteValue::Quarter));
    let command = Command::SetTrill {
        note,
        trill: Some(trill(1, NoteValue::Quarter)),
    };
    match rejected(&mut fx.score, &command) {
        CommandError::InvalidValue { message } => {
            assert!(message.contains("run diagnose"), "{message}")
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn restoring_a_note_validates_its_trill_with_the_forward_rule() {
    let mut fx = fixture();
    let note = fx.notes[0];
    let removed = apply(&mut fx.score, &Command::RemoveNote { note }).unwrap();
    let Command::RestoreNote {
        beat,
        index,
        note: mut kept,
        extensions,
    } = removed
    else {
        panic!("remove_note inverts to restore_note");
    };
    kept.effects.trill = Some(trill(250, NoteValue::Sixteenth));
    let bad = Command::RestoreNote {
        beat,
        index,
        note: kept.clone(),
        extensions: extensions.clone(),
    };
    assert!(matches!(
        rejected(&mut fx.score, &bad),
        CommandError::InvalidValue { .. }
    ));
    kept.effects.trill = Some(trill(2, NoteValue::Sixteenth));
    let good = Command::RestoreNote {
        beat,
        index,
        note: kept,
        extensions,
    };
    round_trip(&mut fx.score, &good);
}

#[test]
fn effect_commands_have_a_closed_json_shape() {
    let command: Command = serde_json::from_value(json!({
        "op": "set_trill", "note": 9, "trill": {"fret": 2, "speed": "sixteenth"}
    }))
    .unwrap();
    assert_eq!(
        command,
        Command::SetTrill {
            note: NoteId(opentab_model::ElementId(9)),
            trill: Some(trill(2, NoteValue::Sixteenth)),
        }
    );
    assert!(
        serde_json::from_value::<Command>(json!({
            "op": "set_trill", "note": 9, "trill": {"fret": 2, "speed": "half"}
        }))
        .is_err()
    );
    let value = serde_json::to_value(Command::SetTremoloBar {
        beat: opentab_model::BeatId(opentab_model::ElementId(4)),
        tremolo_bar: Some(dive()),
    })
    .unwrap();
    assert_eq!(value["op"], json!("set_tremolo_bar"));
    assert_eq!(value["tremolo_bar"]["destination"]["value"], json!(-400));
    let mut split = value.clone();
    split["tremolo_bar"]["middle_end"]["value"] = json!(-50);
    assert!(serde_json::from_value::<Command>(split).is_err());
    let cleared: Command = serde_json::from_value(json!({
        "op": "set_tremolo_bar", "beat": 4, "tremolo_bar": null
    }))
    .unwrap();
    assert!(matches!(
        cleared,
        Command::SetTremoloBar {
            tremolo_bar: None,
            ..
        }
    ));
}
