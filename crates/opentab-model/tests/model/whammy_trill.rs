use opentab_model::{
    BendPoint, Diagnostic, InstrumentKind, ModelError, NoteKind, NoteValue, Pitch, TremoloBar,
    Trill, TrillSpeed, diagnose, from_json, to_json,
};
use serde_json::{Value, json};

use crate::support::{four_four, note, quarters};

const NOTE_VALUES: [NoteValue; 8] = [
    NoteValue::Whole,
    NoteValue::Half,
    NoteValue::Quarter,
    NoteValue::Eighth,
    NoteValue::Sixteenth,
    NoteValue::ThirtySecond,
    NoteValue::SixtyFourth,
    NoteValue::OneTwentyEighth,
];
const TRILL_SPEEDS: [NoteValue; 4] = [
    NoteValue::ThirtySecond,
    NoteValue::Sixteenth,
    NoteValue::Eighth,
    NoteValue::Quarter,
];

fn point(offset: u8, value: i16) -> BendPoint {
    BendPoint::new(offset, value).unwrap()
}

fn dive() -> TremoloBar {
    TremoloBar::new(
        point(0, 0),
        point(35, -100),
        point(35, -100),
        point(99, -200),
    )
    .unwrap()
}

#[test]
fn a_tremolo_bar_keeps_its_signed_points_and_shares_the_middle_value() {
    let bar = dive();
    assert_eq!(bar.origin(), point(0, 0));
    assert_eq!(bar.middle_value(), -100);
    assert_eq!(bar.destination().value(), -200);
    assert!(bar.moves());
    let flat = TremoloBar::new(point(0, 0), point(50, 0), point(50, 0), point(100, 0)).unwrap();
    assert!(!flat.moves());
    assert_eq!(
        TremoloBar::new(point(0, 0), point(10, -50), point(20, 50), point(100, 0)),
        Err(ModelError::InvalidTremoloBar {
            start: -50,
            end: 50
        })
    );
    let rise = TremoloBar::new(point(0, 0), point(10, 50), point(20, 50), point(100, 100)).unwrap();
    assert_ne!(rise, dive());
}

#[test]
fn a_tremolo_bar_round_trips_through_json_and_rejects_split_middles() {
    let text = serde_json::to_string(&dive()).unwrap();
    assert_eq!(
        text,
        r#"{"origin":{"offset":0,"value":0},"middle_start":{"offset":35,"value":-100},"middle_end":{"offset":35,"value":-100},"destination":{"offset":99,"value":-200}}"#
    );
    assert_eq!(serde_json::from_str::<TremoloBar>(&text).unwrap(), dive());
    let split = text.replacen("\"value\":-100}", "\"value\":-90}", 1);
    assert!(serde_json::from_str::<TremoloBar>(&split).is_err());
    let extra = text.replacen("{\"origin\"", "{\"x\":1,\"origin\"", 1);
    assert!(serde_json::from_str::<TremoloBar>(&extra).is_err());
}

#[test]
fn a_trill_speed_is_one_of_the_four_values_guitar_pro_plays() {
    for value in NOTE_VALUES {
        let speed = TrillSpeed::new(value);
        if TRILL_SPEEDS.contains(&value) {
            assert_eq!(speed.unwrap().value(), value);
        } else {
            assert_eq!(speed, Err(ModelError::InvalidTrillSpeed { value }));
        }
        let text = serde_json::to_string(&value).unwrap();
        assert_eq!(
            serde_json::from_str::<TrillSpeed>(&text).is_ok(),
            TRILL_SPEEDS.contains(&value),
            "{text}"
        );
    }
    assert_eq!(TrillSpeed::default().value(), NoteValue::ThirtySecond);
}

fn trilled(fret: u8) -> opentab_model::Score {
    let mut score = four_four(&quarters(4));
    let mut trilled = note(&mut score, 0, 3);
    trilled.effects.trill = Some(Trill {
        fret,
        speed: TrillSpeed::new(NoteValue::Sixteenth).unwrap(),
    });
    score.tracks[0].staves[0].bars[0].voices[0].beats[0].notes = vec![trilled];
    score
}

#[test]
fn a_trill_sounds_open_string_plus_capo_plus_its_fret() {
    let mut score = trilled(5);
    let track = &score.tracks[0];
    let first = &track.staves[0].bars[0].voices[0].beats[0].notes[0];
    assert_eq!(track.trill_pitch_of(first), Some(Pitch(69)));
    score.tracks[0].capo = 2;
    let track = &score.tracks[0];
    let first = &track.staves[0].bars[0].voices[0].beats[0].notes[0];
    assert_eq!(track.trill_pitch_of(first), Some(Pitch(71)));
}

#[test]
fn one_trill_rule_gives_the_sounding_number_and_the_midi_pitch() {
    for fret in 0..=u8::MAX {
        let score = trilled(fret);
        let track = &score.tracks[0];
        let first = &track.staves[0].bars[0].voices[0].beats[0].notes[0];
        let number = 64 + u16::from(fret);
        assert_eq!(track.trill_number_of(first), Some(number));
        let pitch = u8::try_from(number)
            .ok()
            .filter(|key| *key <= 127)
            .map(Pitch);
        assert_eq!(track.trill_pitch_of(first), pitch);
        assert_eq!(track.trill_fits(first), pitch.is_some());
        let tuning = track.trill_tuning();
        assert_eq!(tuning.base(first.kind), Some(64));
        assert_eq!(tuning.base(NoteKind::Percussion { articulation: 0 }), None);
    }
}

#[test]
fn a_trill_above_midi_127_or_on_a_drum_note_is_diagnosed() {
    assert_eq!(diagnose(&trilled(63)), Vec::new());
    let high = trilled(64);
    let id = high.tracks[0].staves[0].bars[0].voices[0].beats[0].notes[0].id;
    assert_eq!(
        diagnose(&high),
        vec![Diagnostic::TrillOutOfRange { note: id }]
    );
    let mut drums = trilled(2);
    drums.tracks[0].instrument.kind = InstrumentKind::Percussion;
    for beat in &mut drums.tracks[0].staves[0].bars[0].voices[0].beats[1..] {
        beat.notes.clear();
    }
    drums.tracks[0].staves[0].bars[0].voices[0].beats[0].notes[0].kind =
        NoteKind::Percussion { articulation: 3 };
    assert_eq!(
        diagnose(&drums),
        vec![Diagnostic::TrillOutOfRange { note: id }]
    );
    let json = serde_json::to_string(&Diagnostic::TrillOutOfRange { note: id }).unwrap();
    assert_eq!(
        json,
        format!(r#"{{"kind":"trill_out_of_range","note":{}}}"#, id.0.0)
    );
}

#[test]
fn trills_and_tremolo_bars_round_trip_through_json_and_default_to_none() {
    let mut score = trilled(2);
    score.tracks[0].staves[0].bars[0].voices[0].beats[1]
        .effects
        .tremolo_bar = Some(dive());
    let text = to_json(&score).unwrap();
    assert_eq!(from_json(&text).unwrap(), score);
    let value: Value = serde_json::from_str(&text).unwrap();
    let beats = "/tracks/0/staves/0/bars/0/voices/0/beats";
    assert_eq!(
        value.pointer(&format!("{beats}/0/notes/0/effects/trill")),
        Some(&json!({"fret": 2, "speed": "sixteenth"}))
    );
    let mut missing = value.clone();
    missing
        .pointer_mut(&format!("{beats}/0/notes/0/effects"))
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("trill");
    missing
        .pointer_mut(&format!("{beats}/1/effects"))
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("tremolo_bar");
    let loaded = from_json(&missing.to_string()).unwrap();
    let beats_loaded = &loaded.tracks[0].staves[0].bars[0].voices[0].beats;
    assert_eq!(beats_loaded[0].notes[0].effects.trill, None);
    assert_eq!(beats_loaded[1].effects.tremolo_bar, None);
    let mut slow = value;
    *slow
        .pointer_mut(&format!("{beats}/0/notes/0/effects/trill/speed"))
        .unwrap() = json!("half");
    assert!(from_json(&slow.to_string()).is_err());
}
