use opentab_command::{Command, CommandError, ExtensionEntry, History, apply};
use opentab_model::{ElementId, IdAllocator, StaffId, TrackId, diagnose};

use crate::checks::{rejected, round_trip};
use crate::support::{Fixture, fixture, four_four, payload};

fn drum_staff(fx: &Fixture) -> StaffId {
    fx.score.tracks[1].staves[0].id
}

fn guitar_staff(fx: &Fixture) -> StaffId {
    fx.score.tracks[0].staves[0].id
}

fn short_drums() -> Fixture {
    let mut fx = fixture();
    fx.score.tracks[1].staves[0].bars.truncate(0);
    fx
}

fn pad_drums(fx: &Fixture) -> Command {
    Command::PadStaffBars {
        track: fx.drums,
        staff: drum_staff(fx),
    }
}

fn message_of(error: CommandError) -> String {
    let CommandError::InvalidValue { message } = error else {
        panic!("expected invalid value, got {error:?}");
    };
    message
}

fn allocator(next: u64) -> IdAllocator {
    serde_json::from_value(serde_json::json!({ "next": next })).unwrap()
}

#[test]
fn pad_staff_bars_appends_empty_bars_up_to_the_master_count() {
    let mut fx = short_drums();
    let first = fx.score.ids.peek().0;
    let command = pad_drums(&fx);
    let inverse = round_trip(&mut fx.score, &command);
    assert_eq!(
        inverse,
        Command::TrimStaffBars {
            track: fx.drums,
            staff: drum_staff(&fx),
            len: 0,
        }
    );
    apply(&mut fx.score, &command).unwrap();
    let bars = &fx.score.tracks[1].staves[0].bars;
    assert_eq!(bars.len(), fx.score.master_bars.len());
    for (offset, bar) in (0u64..).step_by(2).zip(bars) {
        assert_eq!(bar.id.0, ElementId(first + offset));
        assert_eq!(bar.voices.len(), 1);
        assert_eq!(bar.voices[0].id.0, ElementId(first + offset + 1));
        assert!(bar.voices[0].beats.is_empty());
    }
    assert_eq!(fx.score.ids.peek(), ElementId(first + 4));
    assert_eq!(diagnose(&fx.score), Vec::new());
}

#[test]
fn pad_staff_bars_repairs_master_bar_edits() {
    let mut fx = fixture();
    fx.score.tracks[1].staves[0].bars.pop();
    let insert = Command::InsertMasterBar {
        index: 0,
        time_signature: four_four(),
    };
    rejected(&mut fx.score, &insert);
    let pad = pad_drums(&fx);
    apply(&mut fx.score, &pad).unwrap();
    round_trip(&mut fx.score, &insert);
    round_trip(&mut fx.score, &Command::RemoveMasterBar { index: 1 });
}

#[test]
fn pad_staff_bars_rejects_a_staff_that_is_not_short() {
    let mut fx = fixture();
    let message = message_of({
        let pad = pad_drums(&fx);
        rejected(&mut fx.score, &pad)
    });
    assert!(message.contains("nothing to pad"), "{message}");
    let extra = fx.score.tracks[1].staves[0].bars[0].clone();
    fx.score.tracks[1].staves[0].bars.push(extra);
    let message = message_of({
        let pad = pad_drums(&fx);
        rejected(&mut fx.score, &pad)
    });
    assert!(message.contains("nothing to pad"), "{message}");
}

#[test]
fn pad_staff_bars_needs_the_staff_inside_its_track() {
    let mut fx = short_drums();
    let unknown_track = Command::PadStaffBars {
        track: TrackId(ElementId(9999)),
        staff: drum_staff(&fx),
    };
    assert_eq!(
        rejected(&mut fx.score, &unknown_track),
        CommandError::NotFound {
            what: "track",
            id: 9999
        }
    );
    let foreign_staff = Command::PadStaffBars {
        track: fx.guitar,
        staff: drum_staff(&fx),
    };
    assert_eq!(
        rejected(&mut fx.score, &foreign_staff),
        CommandError::NotFound {
            what: "staff",
            id: drum_staff(&fx).0.0
        }
    );
}

#[test]
fn pad_staff_bars_respects_the_id_allocator() {
    let mut fx = short_drums();
    fx.score.ids = allocator(u64::MAX - 3);
    {
        let pad = pad_drums(&fx);
        rejected(&mut fx.score, &pad)
    };
    let mut fx = short_drums();
    fx.score.ids = allocator(3);
    {
        let pad = pad_drums(&fx);
        rejected(&mut fx.score, &pad)
    };
}

#[test]
fn pad_staff_bars_undo_and_redo_through_history() {
    let mut fx = short_drums();
    let original = fx.score.clone();
    let mut history = History::new();
    let pad = pad_drums(&fx);
    history.execute(&mut fx.score, pad).unwrap();
    let padded = fx.score.clone();
    assert!(history.undo(&mut fx.score).unwrap());
    assert_eq!(fx.score.tracks, original.tracks);
    assert!(history.redo(&mut fx.score).unwrap());
    assert_eq!(fx.score, padded);
}

#[test]
fn trim_staff_bars_removes_only_empty_trailing_bars() {
    let mut fx = short_drums();
    let pad = pad_drums(&fx);
    apply(&mut fx.score, &pad).unwrap();
    let padded = fx.score.tracks[1].staves[0].bars.clone();
    let trim = Command::TrimStaffBars {
        track: fx.drums,
        staff: drum_staff(&fx),
        len: 1,
    };
    let inverse = round_trip(&mut fx.score, &trim);
    assert_eq!(
        inverse,
        Command::RestoreStaffBars {
            track: fx.drums,
            staff: drum_staff(&fx),
            bars: padded[1..].to_vec(),
            extensions: Vec::new(),
        }
    );
    let full = Command::TrimStaffBars {
        track: fx.guitar,
        staff: guitar_staff(&fx),
        len: 1,
    };
    let message = message_of(rejected(&mut fx.score, &full));
    assert!(message.contains("holds beats"), "{message}");
    for len in [2, 3] {
        let nothing = Command::TrimStaffBars {
            track: fx.drums,
            staff: drum_staff(&fx),
            len,
        };
        let message = message_of(rejected(&mut fx.score, &nothing));
        assert!(message.contains("nothing to trim"), "{message}");
    }
}

#[test]
fn trim_staff_bars_carries_the_extensions_of_the_trimmed_bars() {
    let mut fx = short_drums();
    let pad = pad_drums(&fx);
    apply(&mut fx.score, &pad).unwrap();
    let voice = fx.score.tracks[1].staves[0].bars[1].voices[0].id.0;
    fx.score.extensions.0.insert(voice, payload("voice"));
    let trim = Command::TrimStaffBars {
        track: fx.drums,
        staff: drum_staff(&fx),
        len: 1,
    };
    let inverse = round_trip(&mut fx.score, &trim);
    let Command::RestoreStaffBars { extensions, .. } = inverse else {
        panic!("restore_staff_bars expected");
    };
    assert_eq!(
        extensions,
        vec![ExtensionEntry {
            id: voice,
            payloads: payload("voice")
        }]
    );
}

fn trimmed() -> (Fixture, Command) {
    let mut fx = short_drums();
    let pad = pad_drums(&fx);
    apply(&mut fx.score, &pad).unwrap();
    let trim = Command::TrimStaffBars {
        track: fx.drums,
        staff: drum_staff(&fx),
        len: 0,
    };
    let restore = apply(&mut fx.score, &trim).unwrap();
    (fx, restore)
}

fn bars_of(command: &mut Command) -> &mut Vec<opentab_model::Bar> {
    let Command::RestoreStaffBars { bars, .. } = command else {
        panic!("restore_staff_bars expected");
    };
    bars
}

#[test]
fn restore_staff_bars_rejects_a_bar_without_voices() {
    let (mut fx, mut restore) = trimmed();
    bars_of(&mut restore)[1].voices.clear();
    let message = message_of(rejected(&mut fx.score, &restore));
    assert!(message.contains("no voices"), "{message}");
}

#[test]
fn restore_staff_bars_rejects_a_bar_with_beats() {
    let (mut fx, mut restore) = trimmed();
    let mut moved = fixture().score.tracks[1].staves[0].bars[1].voices[0].beats[0].clone();
    moved.id = fx.score.alloc().unwrap();
    bars_of(&mut restore)[0].voices[0].beats.push(moved);
    let message = message_of(rejected(&mut fx.score, &restore));
    assert!(message.contains("holds beats"), "{message}");
}

#[test]
fn restore_staff_bars_rejects_too_many_or_no_bars() {
    let (mut fx, mut restore) = trimmed();
    let extra = bars_of(&mut restore)[0].clone();
    let mut longer = restore.clone();
    bars_of(&mut longer).push(extra);
    let message = message_of(rejected(&mut fx.score, &longer));
    assert!(
        message.contains("restore_staff_bars carries 3 bars"),
        "{message}"
    );
    let mut none = restore.clone();
    bars_of(&mut none).clear();
    let message = message_of(rejected(&mut fx.score, &none));
    assert!(
        message.contains("restore_staff_bars carries 0 bars"),
        "{message}"
    );
    apply(&mut fx.score, &restore).expect("the genuine restore applies");
}

#[test]
fn restore_staff_bars_validates_ids() {
    let (mut fx, mut restore) = trimmed();
    let existing = fx.score.tracks[0].staves[0].bars[0].id;
    let mut clash = restore.clone();
    bars_of(&mut clash)[0].id = existing;
    let message = message_of(rejected(&mut fx.score, &clash));
    assert!(message.contains("already exists"), "{message}");
    let own = bars_of(&mut restore)[0].id.0;
    fx.score.extensions.0.insert(own, payload("orphan"));
    let message = message_of(rejected(&mut fx.score, &restore));
    assert!(message.contains("orphan"), "{message}");
}

#[test]
fn trim_staff_bars_refuses_an_overfull_staff() {
    let mut fx = fixture();
    let bar = opentab_model::Bar {
        id: fx.score.alloc().unwrap(),
        voices: vec![opentab_model::Voice {
            id: fx.score.alloc().unwrap(),
            beats: Vec::new(),
        }],
    };
    fx.score.tracks[1].staves[0].bars.push(bar);
    let trim = Command::TrimStaffBars {
        track: fx.drums,
        staff: drum_staff(&fx),
        len: 2,
    };
    let message = message_of(rejected(&mut fx.score, &trim));
    assert!(message.contains("not overfull"), "{message}");
    fx.score.tracks[1].staves[0].bars.pop();
    let pad = pad_drums(&fx);
    rejected(&mut fx.score, &pad);
}

#[test]
fn trim_staff_bars_refuses_a_bar_without_voices() {
    let mut fx = short_drums();
    let pad = pad_drums(&fx);
    apply(&mut fx.score, &pad).unwrap();
    fx.score.tracks[1].staves[0].bars[1].voices.clear();
    let trim = Command::TrimStaffBars {
        track: fx.drums,
        staff: drum_staff(&fx),
        len: 1,
    };
    let message = message_of(rejected(&mut fx.score, &trim));
    assert!(message.contains("no voices"), "{message}");
    let healthy = Command::TrimStaffBars {
        track: fx.drums,
        staff: drum_staff(&fx),
        len: 1,
    };
    let voice = opentab_model::Voice {
        id: fx.score.alloc().unwrap(),
        beats: Vec::new(),
    };
    fx.score.tracks[1].staves[0].bars[1].voices.push(voice);
    round_trip(&mut fx.score, &healthy);
}

#[test]
fn trim_staff_bars_refuses_bars_with_shared_or_unallocated_ids() {
    let mut fx = short_drums();
    let pad = pad_drums(&fx);
    apply(&mut fx.score, &pad).unwrap();
    let trim = Command::TrimStaffBars {
        track: fx.drums,
        staff: drum_staff(&fx),
        len: 1,
    };
    let shared = fx.score.tracks[0].staves[0].bars[0].voices[0].id;
    fx.score.tracks[1].staves[0].bars[1].voices[0].id = shared;
    let message = message_of(rejected(&mut fx.score, &trim));
    assert!(message.contains("already exists in the score"), "{message}");
    let above = opentab_model::VoiceId(ElementId(fx.score.ids.peek().0 + 2));
    fx.score.tracks[1].staves[0].bars[1].voices[0].id = above;
    let message = message_of(rejected(&mut fx.score, &trim));
    assert!(message.contains("allocator"), "{message}");
}
