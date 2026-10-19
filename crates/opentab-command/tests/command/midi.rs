use opentab_command::{AutomationEntry, Command, CommandError, apply};
use opentab_model::{
    Automation, AutomationValue, Balance, ElementId, Fraction, InstrumentKind, Level, MasterBarId,
    MidiBank, MidiChannel, Sound, SoundOverride, SoundRef, TrackId, TrackMidi, diagnose,
};
use serde_json::json;

use crate::support::{Fixture, fixture};

fn volume(fx: &Fixture, bar: usize, position: Fraction, millionths: u32) -> Automation {
    Automation {
        master_bar: fx.score.master_bars[bar].id,
        position,
        linear: false,
        visible: true,
        value: AutomationValue::Volume(Level::new(millionths).unwrap()),
    }
}

fn with_automations() -> Fixture {
    let mut fx = fixture();
    let automations = vec![
        volume(&fx, 0, Fraction::from_integer(0), 100),
        volume(&fx, 1, Fraction::new(1, 4), 200),
        volume(&fx, 0, Fraction::new(1, 2), 300),
        volume(&fx, 1, Fraction::from_integer(1), 400),
    ];
    fx.score.tracks[0].automations = automations;
    fx
}

fn rejected(fx: &mut Fixture, command: &Command) -> CommandError {
    let before = fx.score.clone();
    let error = apply(&mut fx.score, command).expect_err("the command is refused");
    assert_eq!(fx.score, before);
    error
}

#[test]
fn set_track_midi_returns_the_old_midi_and_keeps_the_first_sound_in_step() {
    let mut fx = fixture();
    fx.score.tracks[0].sounds = vec![Sound {
        name: "Clean".to_owned(),
        program: 0,
        bank: MidiBank::default(),
    }];
    let original = fx.score.clone();
    let mut midi = TrackMidi::default_for(InstrumentKind::Fretted);
    midi.primary = MidiChannel::new(4).unwrap();
    midi.bank = MidiBank::new(1, 3).unwrap();
    midi.volume = Level::FULL;
    midi.pan = Balance::LEFT;
    let inverse = apply(
        &mut fx.score,
        &Command::SetTrackMidi {
            track: fx.guitar,
            midi,
        },
    )
    .unwrap();
    assert_eq!(fx.score.tracks[0].midi, midi);
    assert_eq!(fx.score.tracks[0].sounds[0].bank, midi.bank);
    assert_eq!(diagnose(&fx.score), Vec::new());
    assert_eq!(
        inverse,
        Command::SetTrackMidi {
            track: fx.guitar,
            midi: original.tracks[0].midi,
        }
    );
    apply(&mut fx.score, &inverse).unwrap();
    assert_eq!(fx.score, original);
}

#[test]
fn set_track_midi_refuses_a_track_whose_first_sound_is_out_of_step() {
    let mut fx = fixture();
    fx.score.tracks[0].sounds = vec![Sound {
        name: "Clean".to_owned(),
        program: 0,
        bank: MidiBank::new(2, 0).unwrap(),
    }];
    let midi = TrackMidi::default_for(InstrumentKind::Fretted);
    let command = Command::SetTrackMidi {
        track: fx.guitar,
        midi,
    };
    let CommandError::InvalidValue { message } = rejected(&mut fx, &command) else {
        panic!("invalid value expected");
    };
    assert!(message.contains("run diagnose"), "{message}");
    assert!(!diagnose(&fx.score).is_empty());
    let missing = Command::SetTrackMidi {
        track: TrackId(ElementId(999)),
        midi,
    };
    assert_eq!(
        rejected(&mut fx, &missing),
        CommandError::NotFound {
            what: "track",
            id: 999
        }
    );
}

#[test]
fn remove_automation_restores_at_the_exact_index() {
    let mut fx = with_automations();
    let original = fx.score.clone();
    for index in 0..4 {
        let inverse = apply(
            &mut fx.score,
            &Command::RemoveAutomation {
                track: fx.guitar,
                index,
            },
        )
        .unwrap();
        assert_eq!(
            inverse,
            Command::InsertAutomation {
                track: fx.guitar,
                index,
                automation: original.tracks[0].automations[index].clone(),
            }
        );
        let back = apply(&mut fx.score, &inverse).unwrap();
        assert_eq!(
            back,
            Command::RemoveAutomation {
                track: fx.guitar,
                index
            }
        );
        assert_eq!(fx.score, original);
    }
}

#[test]
fn insert_automation_checks_its_bar_position_sound_and_index() {
    let mut fx = with_automations();
    let good = volume(&fx, 0, Fraction::new(3, 4), 1);
    let insert = |automation: Automation, index: usize| Command::InsertAutomation {
        track: TrackId(ElementId(0)),
        index,
        automation,
    };
    let guitar = fx.guitar;
    let on_guitar = |command: Command| match command {
        Command::InsertAutomation {
            index, automation, ..
        } => Command::InsertAutomation {
            track: guitar,
            index,
            automation,
        },
        other => other,
    };
    assert!(matches!(
        rejected(&mut fx, &insert(good.clone(), 0)),
        CommandError::NotFound { what: "track", .. }
    ));
    assert_eq!(
        rejected(&mut fx, &on_guitar(insert(good.clone(), 5))),
        CommandError::IndexOutOfRange {
            what: "automation",
            index: 5,
            len: 4
        }
    );
    let mut stray = good.clone();
    stray.master_bar = MasterBarId(ElementId(999));
    assert!(matches!(
        rejected(&mut fx, &on_guitar(insert(stray, 0))),
        CommandError::NotFound {
            what: "master bar",
            id: 999
        }
    ));
    let late = volume(&fx, 0, Fraction::new(5, 4), 1);
    assert!(matches!(
        rejected(&mut fx, &on_guitar(insert(late, 0))),
        CommandError::InvalidValue { .. }
    ));
    let mut sound = good.clone();
    sound.value = AutomationValue::Sound(SoundRef(0));
    assert!(matches!(
        rejected(&mut fx, &on_guitar(insert(sound, 0))),
        CommandError::InvalidValue { .. }
    ));
    let inverse = apply(&mut fx.score, &on_guitar(insert(good.clone(), 4))).unwrap();
    assert_eq!(fx.score.tracks[0].automations[4], good);
    assert_eq!(
        inverse,
        Command::RemoveAutomation {
            track: fx.guitar,
            index: 4
        }
    );
}

#[test]
fn removing_a_master_bar_takes_its_automations_and_undo_puts_them_back() {
    let mut fx = with_automations();
    let original = fx.score.clone();
    let inverse = apply(&mut fx.score, &Command::RemoveMasterBar { index: 1 }).unwrap();
    let Command::RestoreMasterBar { automations, .. } = &inverse else {
        panic!("restore_master_bar expected");
    };
    assert_eq!(
        automations,
        &vec![
            AutomationEntry {
                track: fx.guitar,
                index: 1,
                automation: original.tracks[0].automations[1].clone(),
            },
            AutomationEntry {
                track: fx.guitar,
                index: 3,
                automation: original.tracks[0].automations[3].clone(),
            },
        ]
    );
    assert_eq!(fx.score.tracks[0].automations.len(), 2);
    assert_eq!(diagnose(&fx.score), Vec::new());
    apply(&mut fx.score, &inverse).unwrap();
    assert_eq!(fx.score, original);
}

#[test]
fn restore_master_bar_refuses_automations_of_another_bar_or_out_of_range() {
    let mut fx = with_automations();
    let inverse = apply(&mut fx.score, &Command::RemoveMasterBar { index: 1 }).unwrap();
    for broken in 0..2 {
        let mut restore = inverse.clone();
        let Command::RestoreMasterBar { automations, .. } = &mut restore else {
            panic!("restore_master_bar expected");
        };
        if broken == 0 {
            automations[0].automation.master_bar = fx.score.master_bars[0].id;
        } else {
            automations[1].index = 9;
        }
        rejected(&mut fx, &restore);
    }
}

#[test]
fn sound_overrides_are_set_replaced_removed_and_restored_in_place() {
    let mut fx = fixture();
    let original = fx.score.clone();
    let mut history = Vec::new();
    for (track, bank) in [
        (fx.guitar, Some("fluid")),
        (fx.drums, Some("kit")),
        (fx.guitar, Some("musescore")),
        (fx.guitar, None),
    ] {
        let inverse = apply(
            &mut fx.score,
            &Command::SetSoundOverride {
                track,
                bank: bank.map(str::to_owned),
            },
        )
        .unwrap();
        history.push(inverse);
        if fx.score.sound_overrides.len() == 2 {
            assert_eq!(fx.score.sound_overrides[0].track, fx.guitar);
        }
    }
    assert_eq!(
        fx.score.sound_overrides,
        vec![SoundOverride {
            track: fx.drums,
            bank: "kit".to_owned(),
        }]
    );
    for inverse in history.iter().rev() {
        apply(&mut fx.score, inverse).unwrap();
    }
    assert_eq!(fx.score, original);
    let missing = Command::SetSoundOverride {
        track: TrackId(ElementId(999)),
        bank: Some("x".to_owned()),
    };
    assert!(matches!(
        rejected(&mut fx, &missing),
        CommandError::NotFound { what: "track", .. }
    ));
}

#[test]
fn midi_commands_have_the_documented_json_shape() {
    let track = TrackId(ElementId(4));
    let automation = Automation {
        master_bar: MasterBarId(ElementId(1)),
        position: Fraction::new(1, 2),
        linear: true,
        visible: false,
        value: AutomationValue::Pan(Balance::CENTRE),
    };
    let cases = [
        (
            Command::SetTrackMidi {
                track,
                midi: TrackMidi::default_for(InstrumentKind::Percussion),
            },
            json!({"op": "set_track_midi", "track": 4, "midi": {
                "port": 0, "primary": 9, "secondary": 9, "per_string": false,
                "bank": {"msb": 0, "lsb": 0}, "volume": 500000, "pan": 500000}}),
        ),
        (
            Command::InsertAutomation {
                track,
                index: 2,
                automation,
            },
            json!({"op": "insert_automation", "track": 4, "index": 2, "automation": {
                "master_bar": 1, "position": [1, 2], "linear": true, "visible": false,
                "value": {"kind": "pan", "value": 500000}}}),
        ),
        (
            Command::RemoveAutomation { track, index: 0 },
            json!({"op": "remove_automation", "track": 4, "index": 0}),
        ),
        (
            Command::SetSoundOverride { track, bank: None },
            json!({"op": "set_sound_override", "track": 4, "bank": null}),
        ),
        (
            Command::RestoreSoundOverrides {
                sound_overrides: vec![SoundOverride {
                    track,
                    bank: "fluid".to_owned(),
                }],
            },
            json!({"op": "restore_sound_overrides",
                   "sound_overrides": [{"track": 4, "bank": "fluid"}]}),
        ),
    ];
    for (command, expected) in cases {
        assert_eq!(serde_json::to_value(&command).unwrap(), expected);
        let back: Command = serde_json::from_value(expected).unwrap();
        assert_eq!(back, command);
    }
}
