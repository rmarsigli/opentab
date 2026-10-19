use opentab_model::{
    Automation, AutomationValue, Balance, Diagnostic, ElementId, Fraction, InstrumentFamily,
    InstrumentKind, InstrumentSet, Level, MasterBarId, MidiBank, MidiChannel, ModelError, Score,
    Sound, SoundOverride, SoundRef, TrackId, TrackMidi, diagnose, from_json, to_json,
};
use serde_json::{Value, json};

use crate::support::{four_four, quarters};

const SETS: [InstrumentSet; 12] = [
    InstrumentSet::ElectricGuitar,
    InstrumentSet::SteelGuitar,
    InstrumentSet::NylonGuitar,
    InstrumentSet::ElectricBass,
    InstrumentSet::AcousticBass,
    InstrumentSet::Violin,
    InstrumentSet::AcousticPiano,
    InstrumentSet::Saxophone,
    InstrumentSet::Trumpet,
    InstrumentSet::ElectricOrgan,
    InstrumentSet::Voice,
    InstrumentSet::DrumKit,
];

fn mixed_score() -> Score {
    let mut score = four_four(&quarters(4));
    let master_bar = score.master_bars[0].id;
    let track = &mut score.tracks[0];
    track.instrument.set = Some(InstrumentSet::SteelGuitar);
    track.midi = TrackMidi {
        port: 1,
        primary: MidiChannel::new(2).unwrap(),
        secondary: MidiChannel::new(3).unwrap(),
        per_string: true,
        bank: MidiBank::new(1, 3).unwrap(),
        volume: Level::new(812_500).unwrap(),
        pan: Balance::new(0).unwrap(),
    };
    track.sounds = vec![
        Sound {
            name: "Clean".to_string(),
            program: track.instrument.midi_program,
            bank: MidiBank::new(1, 3).unwrap(),
        },
        Sound {
            name: "Lead".to_string(),
            program: 29,
            bank: MidiBank::default(),
        },
    ];
    track.automations = vec![
        Automation {
            master_bar,
            position: Fraction::new(1, 2),
            linear: true,
            visible: false,
            value: AutomationValue::Volume(Level::FULL),
        },
        Automation {
            master_bar,
            position: Fraction::from_integer(1),
            linear: false,
            visible: true,
            value: AutomationValue::Sound(SoundRef(1)),
        },
    ];
    score.sound_overrides = vec![SoundOverride {
        track: score.tracks[0].id,
        bank: "fluid".to_string(),
    }];
    score
}

fn document() -> Value {
    serde_json::from_str(&to_json(&mixed_score()).unwrap()).unwrap()
}

fn rejected(value: &Value) -> String {
    match from_json(&value.to_string()) {
        Err(ModelError::Json { message }) => message,
        other => panic!("expected a JSON error, got {other:?}"),
    }
}

#[test]
fn midi_data_round_trips_through_json_and_diagnoses_clean() {
    let score = mixed_score();
    assert_eq!(diagnose(&score), Vec::new());
    let back = from_json(&to_json(&score).unwrap()).unwrap();
    assert_eq!(back, score);
    let value = document();
    assert_eq!(
        value["tracks"][0]["midi"],
        json!({"port": 1, "primary": 2, "secondary": 3, "per_string": true,
               "bank": {"msb": 1, "lsb": 3}, "volume": 812500, "pan": 0})
    );
    assert_eq!(
        value["tracks"][0]["automations"][1]["value"],
        json!({"kind": "sound", "value": 1})
    );
    assert_eq!(value["tracks"][0]["instrument"]["set"], "steel_guitar");
    assert_eq!(
        value["sound_overrides"],
        json!([{"track": value["tracks"][0]["id"], "bank": "fluid"}])
    );
}

#[test]
fn out_of_range_midi_values_are_rejected_on_load() {
    let cases = [
        ("/tracks/0/midi/primary", json!(16), "MIDI channel 16"),
        ("/tracks/0/midi/bank/msb", json!(128), "MIDI bank 128"),
        ("/tracks/0/midi/volume", json!(1_000_001), "level 1000001"),
        ("/tracks/0/midi/pan", json!(1_000_001), "balance 1000001"),
        (
            "/tracks/0/automations/0/position",
            json!([-1, 2]),
            "non-negative",
        ),
        (
            "/tracks/0/automations/0/value",
            json!({"kind": "tempo", "value": 1}),
            "unknown variant",
        ),
        (
            "/tracks/0/instrument/set",
            json!("banjo"),
            "unknown variant",
        ),
    ];
    for (pointer, replacement, needle) in cases {
        let mut value = document();
        *value.pointer_mut(pointer).unwrap() = replacement;
        let message = rejected(&value);
        assert!(message.contains(needle), "{pointer}: {message}");
    }
    let mut value = document();
    value["tracks"][0]["midi"]["extra"] = json!(1);
    assert!(rejected(&value).contains("unknown field"));
    let mut value = document();
    value["tracks"][0]["instrument"]
        .as_object_mut()
        .unwrap()
        .remove("set");
    assert_eq!(
        from_json(&value.to_string()).unwrap().tracks[0]
            .instrument
            .set,
        None
    );
}

#[test]
fn the_constructors_hold_their_ranges_exactly() {
    for channel in 0..=u8::MAX {
        assert_eq!(MidiChannel::new(channel).is_ok(), channel <= 15);
    }
    for byte in 0..=u8::MAX {
        assert_eq!(MidiBank::new(byte, 0).is_ok(), byte <= 127);
        assert_eq!(MidiBank::new(0, byte).is_ok(), byte <= 127);
    }
    for millionths in [0, 1, 500_000, 999_999, 1_000_000, 1_000_001, u32::MAX] {
        assert_eq!(Level::new(millionths).is_ok(), millionths <= 1_000_000);
        assert_eq!(Balance::new(millionths).is_ok(), millionths <= 1_000_000);
    }
    assert_eq!(
        TrackMidi::default_for(InstrumentKind::Percussion).primary,
        MidiChannel::DRUMS
    );
    assert_eq!(MidiChannel::DRUMS.get(), 9);
}

#[test]
fn the_family_prefers_the_set_then_drums_then_the_program() {
    let program_families = [
        InstrumentFamily::Piano,
        InstrumentFamily::ChromaticPercussion,
        InstrumentFamily::Organ,
        InstrumentFamily::Guitar,
        InstrumentFamily::Bass,
        InstrumentFamily::Strings,
        InstrumentFamily::Ensemble,
        InstrumentFamily::Brass,
        InstrumentFamily::Reed,
        InstrumentFamily::Pipe,
        InstrumentFamily::SynthLead,
        InstrumentFamily::SynthPad,
        InstrumentFamily::SynthEffects,
        InstrumentFamily::Ethnic,
        InstrumentFamily::Percussive,
        InstrumentFamily::SoundEffects,
    ];
    for program in 0..=u8::MAX {
        let expected = program_families
            .get(usize::from(program) / 8)
            .copied()
            .unwrap_or(InstrumentFamily::SoundEffects);
        assert_eq!(
            InstrumentFamily::of(InstrumentKind::Fretted, None, program),
            expected,
            "{program}"
        );
        assert_eq!(
            InstrumentFamily::of(InstrumentKind::Percussion, None, program),
            InstrumentFamily::Drums
        );
        for set in SETS {
            assert_eq!(
                InstrumentFamily::of(InstrumentKind::Fretted, Some(set), program),
                set.family()
            );
        }
    }
    assert_eq!(InstrumentSet::Voice.family(), InstrumentFamily::Ensemble);
    assert_eq!(
        InstrumentSet::NylonGuitar.family(),
        InstrumentFamily::Guitar
    );
    assert_eq!(InstrumentSet::AcousticBass.family(), InstrumentFamily::Bass);
    assert_eq!(InstrumentSet::DrumKit.family(), InstrumentFamily::Drums);
    let percussion: Vec<InstrumentSet> = SETS
        .into_iter()
        .filter(|set| set.kind() == InstrumentKind::Percussion)
        .collect();
    assert_eq!(percussion, vec![InstrumentSet::DrumKit]);
}

#[test]
fn broken_midi_data_is_diagnosed() {
    let mut score = mixed_score();
    let track = score.tracks[0].id;
    let master_bar = score.master_bars[0].id;
    let stray = MasterBarId(ElementId(999));
    score.tracks[0].automations.extend([
        Automation {
            master_bar,
            position: Fraction::new(5, 4),
            linear: false,
            visible: true,
            value: AutomationValue::Pan(Balance::CENTRE),
        },
        Automation {
            master_bar: stray,
            position: Fraction::from_integer(0),
            linear: false,
            visible: true,
            value: AutomationValue::Sound(SoundRef(2)),
        },
    ]);
    score.tracks[0].sounds[0].bank = MidiBank::default();
    score.tracks[0].instrument.set = Some(InstrumentSet::DrumKit);
    let orphan = TrackId(ElementId(998));
    score.sound_overrides.push(SoundOverride {
        track: orphan,
        bank: "x".to_string(),
    });
    assert_eq!(
        diagnose(&score),
        vec![
            Diagnostic::InstrumentSetMismatch { track },
            Diagnostic::FirstSoundMismatch { track },
            Diagnostic::AutomationOutsideBar {
                track,
                master_bar,
                position: Fraction::new(5, 4),
            },
            Diagnostic::AutomationOutsideBar {
                track,
                master_bar: stray,
                position: Fraction::from_integer(0),
            },
            Diagnostic::MissingSound {
                track,
                sound: SoundRef(2),
            },
            Diagnostic::SoundOverrideWithoutTrack { track: orphan },
        ]
    );
    let mut program = mixed_score();
    program.tracks[0].sounds[0].program = 99;
    assert_eq!(
        diagnose(&program),
        vec![Diagnostic::FirstSoundMismatch {
            track: program.tracks[0].id
        }]
    );
}
