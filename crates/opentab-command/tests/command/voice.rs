use opentab_command::{Command, CommandError, apply};
use opentab_model::{BarId, BeatId, ElementId, NoteValue, VoiceId, VoiceSlot};

use crate::checks::{rejected, round_trip};
use crate::support::{duration, fixture, payload};

fn voiceless_second_bar() -> (crate::support::Fixture, BarId) {
    let mut fx = fixture();
    let bar = &mut fx.score.tracks[0].staves[0].bars[1];
    bar.voices.clear();
    let id = bar.id;
    (fx, id)
}

fn message_of(error: CommandError) -> String {
    let CommandError::InvalidValue { message } = error else {
        panic!("expected invalid value, got {error:?}");
    };
    message
}

#[test]
fn insert_beat_into_a_voiceless_bar_creates_voice_slot_zero() {
    let (mut fx, bar) = voiceless_second_bar();
    let voice = VoiceId(fx.score.ids.peek());
    let beat = BeatId(ElementId(voice.0.0 + 1));
    let command = Command::InsertBeat {
        voice: None,
        bar: Some(bar),
        index: 0,
        duration: duration(NoteValue::Whole),
    };
    let inverse = round_trip(&mut fx.score, &command);
    assert_eq!(inverse, Command::RemoveVoice { voice });
    apply(&mut fx.score, &command).unwrap();
    let voices = &fx.score.tracks[0].staves[0].bars[1].voices;
    assert_eq!(voices.len(), 1);
    assert_eq!(voices[0].id, voice);
    assert_eq!(voices[0].slot, VoiceSlot::default());
    assert_eq!(voices[0].beats.len(), 1);
    assert_eq!(voices[0].beats[0].id, beat);
}

#[test]
fn insert_beat_into_a_bar_uses_its_slot_zero_voice() {
    let mut fx = fixture();
    let bar = fx.score.tracks[0].staves[0].bars[0].id;
    let fresh = BeatId(fx.score.ids.peek());
    let command = Command::InsertBeat {
        voice: None,
        bar: Some(bar),
        index: 4,
        duration: duration(NoteValue::Quarter),
    };
    assert_eq!(
        round_trip(&mut fx.score, &command),
        Command::RemoveBeat { beat: fresh }
    );
}

#[test]
fn insert_beat_into_a_bar_without_slot_zero_puts_the_new_voice_first() {
    let mut fx = fixture();
    let bar = &mut fx.score.tracks[0].staves[0].bars[1];
    bar.voices[0].slot = VoiceSlot::new(2).unwrap();
    let id = bar.id;
    let command = Command::InsertBeat {
        voice: None,
        bar: Some(id),
        index: 0,
        duration: duration(NoteValue::Whole),
    };
    round_trip(&mut fx.score, &command);
    apply(&mut fx.score, &command).unwrap();
    let slots: Vec<u8> = fx.score.tracks[0].staves[0].bars[1]
        .voices
        .iter()
        .map(|voice| voice.slot.get())
        .collect();
    assert_eq!(slots, vec![0, 2]);
}

#[test]
fn insert_beat_needs_exactly_one_target_and_a_fitting_index() {
    let (mut fx, bar) = voiceless_second_bar();
    let both = Command::InsertBeat {
        voice: Some(fx.voice),
        bar: Some(bar),
        index: 0,
        duration: duration(NoteValue::Whole),
    };
    assert!(message_of(rejected(&mut fx.score, &both)).contains("exactly one target"));
    let neither = Command::InsertBeat {
        voice: None,
        bar: None,
        index: 0,
        duration: duration(NoteValue::Whole),
    };
    assert!(message_of(rejected(&mut fx.score, &neither)).contains("exactly one target"));
    let beyond = Command::InsertBeat {
        voice: None,
        bar: Some(bar),
        index: 1,
        duration: duration(NoteValue::Whole),
    };
    assert!(matches!(
        rejected(&mut fx.score, &beyond),
        CommandError::IndexOutOfRange { .. }
    ));
    let missing = Command::InsertBeat {
        voice: None,
        bar: Some(BarId(ElementId(9999))),
        index: 0,
        duration: duration(NoteValue::Whole),
    };
    assert!(matches!(
        rejected(&mut fx.score, &missing),
        CommandError::NotFound { what: "bar", .. }
    ));
}

#[test]
fn remove_voice_leaves_a_voiceless_bar_and_restores_it_with_its_payloads() {
    let mut fx = fixture();
    let voice = fx.voice;
    let beat = fx.beats[2];
    let inverse = round_trip(&mut fx.score, &Command::RemoveVoice { voice });
    let Command::RestoreVoice {
        bar,
        index,
        voice: removed,
        extensions,
    } = &inverse
    else {
        panic!("restore_voice expected, got {inverse:?}");
    };
    assert_eq!(*bar, fx.score.tracks[0].staves[0].bars[0].id);
    assert_eq!(*index, 0);
    assert_eq!(removed.id, voice);
    assert_eq!(extensions.len(), 2);
    assert!(extensions.iter().any(|entry| entry.id == beat.0));
    apply(&mut fx.score, &Command::RemoveVoice { voice }).unwrap();
    assert!(fx.score.tracks[0].staves[0].bars[0].voices.is_empty());
    assert!(!fx.score.extensions.0.contains_key(&beat.0));
}

#[test]
fn restore_voice_keeps_slots_strictly_increasing() {
    let mut fx = fixture();
    let mut restore = apply(&mut fx.score, &Command::RemoveVoice { voice: fx.voice }).unwrap();
    let Command::RestoreVoice { index, .. } = &mut restore else {
        panic!("restore_voice expected");
    };
    *index = 1;
    assert!(matches!(
        rejected(&mut fx.score, &restore),
        CommandError::IndexOutOfRange { .. }
    ));
    let Command::RestoreVoice { index, .. } = &mut restore else {
        panic!("restore_voice expected");
    };
    *index = 0;
    let bar = &mut fx.score.tracks[0].staves[0].bars[0];
    let mut other = fixture().score.tracks[0].staves[0].bars[1].voices[0].clone();
    other.id = VoiceId(ElementId(fx.score.ids.peek().0 + 50));
    other.beats.clear();
    bar.voices.push(other);
    fx.score.ids = serde_json::from_value(serde_json::json!({"next": 100_000})).unwrap();
    assert!(message_of(rejected(&mut fx.score, &restore)).contains("strictly increasing"));
    fx.score.tracks[0].staves[0].bars[0].voices[0].slot = VoiceSlot::new(1).unwrap();
    apply(&mut fx.score, &restore).expect("slot 0 fits before slot 1");
}

#[test]
fn restore_voice_validates_its_beats_and_ids() {
    let mut fx = fixture();
    let restore = apply(&mut fx.score, &Command::RemoveVoice { voice: fx.voice }).unwrap();
    let mut clash = restore.clone();
    let Command::RestoreVoice { voice, .. } = &mut clash else {
        panic!("restore_voice expected");
    };
    voice.id = fx.score.tracks[0].staves[0].bars[1].voices[0].id;
    assert!(message_of(rejected(&mut fx.score, &clash)).contains("already exists"));
    let mut doubled = restore.clone();
    let Command::RestoreVoice { voice, .. } = &mut doubled else {
        panic!("restore_voice expected");
    };
    let mut copy = voice.beats[0].notes[0].clone();
    copy.id = opentab_model::NoteId(ElementId(fx.score.ids.peek().0 - 1));
    voice.beats[0].notes.push(copy);
    assert!(message_of(rejected(&mut fx.score, &doubled)).contains("already has a note"));
    let own = fx.voice.0;
    fx.score.extensions.0.insert(own, payload("orphan"));
    assert!(message_of(rejected(&mut fx.score, &restore)).contains("orphan"));
}

#[test]
fn remove_voice_refuses_a_shared_bar_id_and_a_missing_voice() {
    let mut fx = fixture();
    let first = fx.score.tracks[0].staves[0].bars[0].id;
    fx.score.tracks[1].staves[0].bars[0].id = first;
    let drums_voice = fx.score.tracks[1].staves[0].bars[0].voices[0].id;
    assert!(
        message_of(rejected(
            &mut fx.score,
            &Command::RemoveVoice { voice: drums_voice }
        ))
        .contains("not unique")
    );
    assert!(matches!(
        rejected(
            &mut fx.score,
            &Command::RemoveVoice {
                voice: VoiceId(ElementId(9999))
            }
        ),
        CommandError::NotFound { what: "voice", .. }
    ));
}
