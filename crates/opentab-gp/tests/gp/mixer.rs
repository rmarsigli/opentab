use std::io::{Cursor, Read};

use opentab_gp::{GpError, Refusal, read_gp, write_gp};
use opentab_model::{
    Automation, AutomationValue, Balance, Base64Bytes, ContainerEntry, ElementId, Fraction,
    InstrumentKind, InstrumentSet, Level, MasterBarId, MidiBank, MidiChannel, Score, Sound,
    SoundOverride, SoundRef, TrackId, TrackMidi, diagnose,
};
use zip::ZipArchive;

use crate::anchors::gpif_text;
use crate::support::{Doc, payloads, read_gpif, track};

const OVERRIDES_ENTRY: &str = opentab_model::SOUND_OVERRIDES_ENTRY;

fn with_track(track: &str) -> Score {
    Doc {
        track: track.to_string(),
        ..Doc::default()
    }
    .read()
}

fn strip(parameters: &str, automations: &str) -> String {
    format!(
        "<RSE><ChannelStrip version=\"E56\"><Parameters>{parameters}</Parameters><Automations>{automations}</Automations></ChannelStrip></RSE>"
    )
}

fn automation(kind: &str, position: &str, value: &str) -> String {
    format!(
        "<Automation><Type>{kind}</Type><Linear>false</Linear><Bar>0</Bar><Position>{position}</Position><Visible>true</Visible><Value>{value}</Value></Automation>"
    )
}

fn sound(name: &str, path: &str, program: u8) -> String {
    format!(
        "<Sound><Name><![CDATA[{name}]]></Name><Label><![CDATA[{name}]]></Label><Path>{path}</Path><Role>Factory</Role><MIDI><LSB>0</LSB><MSB>0</MSB><Program>{program}</Program></MIDI></Sound>"
    )
}

fn round_trip(score: &Score) -> Score {
    let bytes = write_gp(score).expect("write");
    let again = read_gp(&bytes).expect("read");
    assert_eq!(&again, score);
    again
}

fn refusal(score: &Score) -> Refusal {
    assert!(!diagnose(score).is_empty(), "a refused score must diagnose");
    match write_gp(score) {
        Err(GpError::Unsupported { reason, .. }) => reason,
        other => panic!(
            "expected a refusal, got {:?}",
            other.map(|bytes| bytes.len())
        ),
    }
}

fn entry_names(bytes: &[u8]) -> Vec<String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).expect("archive");
    (0..archive.len())
        .map(|index| archive.by_index(index).expect("entry").name().to_string())
        .collect()
}

fn entry_data(bytes: &[u8], name: &str) -> String {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).expect("archive");
    let mut file = archive.by_name(name).expect("entry");
    let mut text = String::new();
    file.read_to_string(&mut text).expect("text");
    text
}

#[test]
fn the_midi_connection_is_typed_and_defaults_follow_the_instrument_kind() {
    let score = with_track(concat!(
        "<MidiConnection><Port>2</Port><PrimaryChannel>4</PrimaryChannel>",
        "<SecondaryChannel>5</SecondaryChannel><ForeOneChannelPerString>true</ForeOneChannelPerString>",
        "</MidiConnection>"
    ));
    let midi = track(&score).midi;
    assert_eq!(midi.port, 2);
    assert_eq!(midi.primary, MidiChannel::new(4).unwrap());
    assert_eq!(midi.secondary, MidiChannel::new(5).unwrap());
    assert!(midi.per_string);
    assert_eq!(payloads(&score, track(&score).id.0), Vec::<String>::new());
    round_trip(&score);
    let plain = Doc::default().read();
    assert_eq!(
        track(&plain).midi,
        TrackMidi::default_for(InstrumentKind::Fretted)
    );
    let drums = Doc {
        instrument: "drumKit".to_string(),
        tuning: "0 0 0 0 0 0".to_string(),
        note: "<InstrumentArticulation>3</InstrumentArticulation>".to_string(),
        ..Doc::default()
    }
    .read();
    assert_eq!(track(&drums).midi.primary, MidiChannel::DRUMS);
    assert_eq!(track(&drums).midi.secondary, MidiChannel::DRUMS);
}

#[test]
fn a_channel_outside_the_sixteen_midi_channels_is_an_error() {
    let error = Doc {
        track: "<MidiConnection><PrimaryChannel>16</PrimaryChannel></MidiConnection>".to_string(),
        ..Doc::default()
    }
    .error();
    assert!(error.to_string().contains("MIDI channel 16"), "{error}");
}

#[test]
fn parameter_twelve_is_the_volume_and_eleven_the_pan() {
    let score = with_track(&strip(
        "0.5 0.5 0.5 0.5 0.5 0.5 0.5 0.5 0.5 0 0.5 0.25 0.75 0.5 0.5 0.5",
        &format!(
            "{}{}",
            automation("DSPParam_12", "0", "0.8125"),
            automation("DSPParam_11", "0.5", "0.1")
        ),
    ));
    let track = track(&score);
    assert_eq!(track.midi.volume, Level::new(750_000).unwrap());
    assert_eq!(track.midi.pan, Balance::new(250_000).unwrap());
    let values: Vec<AutomationValue> = track
        .automations
        .iter()
        .map(|automation| automation.value)
        .collect();
    assert_eq!(
        values,
        vec![
            AutomationValue::Pan(Balance::new(100_000).unwrap()),
            AutomationValue::Volume(Level::new(812_500).unwrap()),
        ]
    );
    assert_eq!(track.automations[0].position, Fraction::new(1, 2));
    round_trip(&score);
    let text = gpif_text(&write_gp(&score).expect("write"));
    let pan = text.find("<Type>DSPParam_11</Type>").expect("pan lane");
    let volume = text.find("<Type>DSPParam_12</Type>").expect("volume lane");
    assert!(pan < volume);
}

#[test]
fn a_volume_edit_rewrites_only_its_parameter() {
    let parameters = "0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 0 0.11 0.250 0.62 0.13 0.14 0.15";
    let mut score = with_track(&strip(parameters, ""));
    assert_eq!(track(&score).midi.pan, Balance::new(250_000).unwrap());
    assert!(gpif_text(&write_gp(&score).unwrap()).contains(parameters));
    score.tracks[0].midi.volume = Level::new(5).unwrap();
    let text = gpif_text(&write_gp(&score).unwrap());
    assert!(
        text.contains("0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 0 0.11 0.250 0.000005 0.13 0.14 0.15"),
        "{text}"
    );
    let again = read_gp(&write_gp(&score).unwrap()).unwrap();
    assert_eq!(again.tracks[0].midi, score.tracks[0].midi);
    round_trip(&again);
}

#[test]
fn a_non_default_mixer_without_a_strip_writes_gp_defaults_around_it() {
    let mut score = Doc::default().read();
    score.tracks[0].midi.pan = Balance::LEFT;
    let text = gpif_text(&write_gp(&score).unwrap());
    assert!(
        text.contains(
            "<Parameters>0.5 0.5 0.5 0.5 0.5 0.5 0.5 0.5 0.5 0 0.5 0 0.5 0.5 0.5 0.5</Parameters>"
        ),
        "{text}"
    );
    let again = round_trip(&score);
    assert_eq!(payloads(&again, again.tracks[0].id.0), Vec::<String>::new());
}

#[test]
fn sound_automations_name_their_sound_by_path_name_and_role() {
    let sounds = format!(
        "<Sounds>{}{}</Sounds>",
        sound("Clean", "Stringed/Clean", 27),
        sound("Dirty", "Stringed/Dirty", 30)
    );
    let automations = format!(
        "<Automations>{}{}{}</Automations>",
        automation("Sound", "0", "<![CDATA[Stringed/Clean;Clean;Factory]]>"),
        automation("Sound", "1", "<![CDATA[Stringed/Dirty;Dirty;Factory]]>"),
        automation("Sound", "0.5", "<![CDATA[Stringed/Gone;Gone;Factory]]>"),
    );
    let score = with_track(&format!("{sounds}{automations}"));
    let track = track(&score);
    assert_eq!(
        track.automations,
        vec![
            Automation {
                master_bar: score.master_bars[0].id,
                position: Fraction::from_integer(0),
                linear: false,
                visible: true,
                value: AutomationValue::Sound(SoundRef(0)),
            },
            Automation {
                master_bar: score.master_bars[0].id,
                position: Fraction::new(1, 4),
                linear: false,
                visible: true,
                value: AutomationValue::Sound(SoundRef(1)),
            },
        ]
    );
    let kept = payloads(&score, track.id.0);
    assert!(
        kept.iter().any(
            |data| data.starts_with("<Keep parent=\"Automations\" index=\"2\"")
                && data.contains("Stringed/Gone")
        ),
        "{kept:?}"
    );
    round_trip(&score);
}

struct Placed {
    time: &'static str,
    kind: &'static str,
    text: &'static str,
    position: Option<Fraction>,
}

const PLACED: [Placed; 12] = [
    Placed {
        time: "4/4",
        kind: "Sound",
        text: "0.25",
        position: Some(Fraction::new_raw(1, 16)),
    },
    Placed {
        time: "4/4",
        kind: "Sound",
        text: "1",
        position: Some(Fraction::new_raw(1, 4)),
    },
    Placed {
        time: "4/4",
        kind: "Sound",
        text: "2",
        position: Some(Fraction::new_raw(1, 2)),
    },
    Placed {
        time: "4/4",
        kind: "Sound",
        text: "4",
        position: Some(Fraction::new_raw(1, 1)),
    },
    Placed {
        time: "4/4",
        kind: "Sound",
        text: "4.25",
        position: None,
    },
    Placed {
        time: "2/2",
        kind: "Sound",
        text: "1",
        position: Some(Fraction::new_raw(1, 4)),
    },
    Placed {
        time: "7/8",
        kind: "Sound",
        text: "2.75",
        position: Some(Fraction::new_raw(11, 16)),
    },
    Placed {
        time: "12/4",
        kind: "Sound",
        text: "10.5",
        position: Some(Fraction::new_raw(21, 8)),
    },
    Placed {
        time: "12/8",
        kind: "Sound",
        text: "0.333333",
        position: Some(Fraction::new_raw(1, 12)),
    },
    Placed {
        time: "7/8",
        kind: "DSPParam_12",
        text: "0.785714",
        position: Some(Fraction::new_raw(11, 16)),
    },
    Placed {
        time: "4/4",
        kind: "DSPParam_11",
        text: "0.25",
        position: Some(Fraction::new_raw(1, 4)),
    },
    Placed {
        time: "12/4",
        kind: "DSPParam_12",
        text: "0.875",
        position: Some(Fraction::new_raw(21, 8)),
    },
];

#[test]
fn sound_positions_count_quarter_notes_and_strip_positions_bar_fractions() {
    for placed in PLACED {
        let value = if placed.kind == "Sound" {
            "<![CDATA[Stringed/Clean;Clean;Factory]]>"
        } else {
            "0.5"
        };
        let lane = if placed.kind == "Sound" {
            format!(
                "<Sounds>{}</Sounds><Automations>{}</Automations>",
                sound("Clean", "Stringed/Clean", 27),
                automation(placed.kind, placed.text, value)
            )
        } else {
            strip("0.5", &automation(placed.kind, placed.text, value))
        };
        let score = Doc {
            track: lane,
            time: placed.time.to_string(),
            ..Doc::default()
        }
        .read();
        let found = track(&score)
            .automations
            .first()
            .map(|automation| automation.position);
        assert_eq!(
            found, placed.position,
            "{} {} {}",
            placed.time, placed.kind, placed.text
        );
        let text = gpif_text(&write_gp(&score).unwrap());
        assert!(
            text.contains(&format!("<Position>{}</Position>", placed.text)),
            "{} {}: {text}",
            placed.kind,
            placed.text
        );
        round_trip(&score);
    }
}

#[test]
fn ambiguous_or_non_canonical_automations_stay_opaque() {
    let sounds = format!(
        "<Sounds>{}{}</Sounds>",
        sound("Same", "Same/Path", 27),
        sound("Same", "Same/Path", 30)
    );
    let automations = format!(
        "<Automations>{}</Automations>",
        automation("Sound", "0", "<![CDATA[Same/Path;Same;Factory]]>"),
    );
    let score = with_track(&format!(
        "{}{sounds}{automations}",
        strip(
            "0.5",
            &format!(
                "{}{}",
                automation("DSPParam_12", "0.50", "0.5"),
                automation("DSPParam_11", "0", "0.50")
            )
        )
    ));
    assert_eq!(track(&score).automations, Vec::new());
    round_trip(&score);
}

#[test]
fn a_generated_sound_follows_the_link_rule_guitar_pro_resolves() {
    let mut score = Doc::default().read();
    let track = &mut score.tracks[0];
    track.instrument.midi_program = 27;
    track.sounds = vec![
        Sound {
            name: "Clean".to_string(),
            program: 27,
            bank: MidiBank::default(),
        },
        Sound {
            name: "Lead".to_string(),
            program: 29,
            bank: MidiBank::new(1, 3).unwrap(),
        },
    ];
    track.automations = vec![Automation {
        master_bar: score.master_bars[0].id,
        position: Fraction::new(1, 2),
        linear: false,
        visible: true,
        value: AutomationValue::Sound(SoundRef(1)),
    }];
    let text = gpif_text(&write_gp(&score).unwrap());
    assert!(
        text.contains("<Sound>\n<Name><![CDATA[Lead]]></Name>\n<Label><![CDATA[Lead]]></Label>\n<MIDI>\n<LSB>3</LSB>\n<MSB>1</MSB>\n<Program>29</Program>\n</MIDI>\n</Sound>"),
        "{text}"
    );
    assert!(!text.contains("<Path>"), "{text}");
    assert!(
        text.contains("<Value><![CDATA[;Lead;User]]></Value>"),
        "{text}"
    );
    assert!(text.contains("<Position>2</Position>"), "{text}");
    round_trip(&score);
}

#[test]
fn a_label_of_its_own_is_kept_until_the_sound_is_renamed() {
    let score = with_track(
        "<Sounds><Sound><Name><![CDATA[Long name]]></Name><Label><![CDATA[Short]]></Label><MIDI><Program>0</Program></MIDI></Sound></Sounds>",
    );
    let kept = payloads(&score, track(&score).id.0);
    assert_eq!(
        kept,
        vec![
            "<Keep parent=\"Sounds/Sound#0\" index=\"1\" guard=\"sound-label:0 Long name\"><Label><![CDATA[Short]]></Label></Keep>"
                .to_string()
        ]
    );
    assert!(gpif_text(&write_gp(&score).unwrap()).contains("<Label><![CDATA[Short]]></Label>"));
    let mut renamed = score.clone();
    renamed.tracks[0].sounds[0].name = "Other".to_string();
    let text = gpif_text(&write_gp(&renamed).unwrap());
    assert!(text.contains("<Label><![CDATA[Other]]></Label>"), "{text}");
    assert!(!text.contains("CDATA[Short]"), "{text}");
}

#[test]
fn a_bare_sound_is_the_implicit_sound_of_the_track() {
    let score = with_track(
        "<Sounds><Sound><MIDI><LSB>3</LSB><MSB>1</MSB><Program>33</Program></MIDI></Sound></Sounds>",
    );
    let track = track(&score);
    assert!(track.sounds.is_empty());
    assert_eq!(track.instrument.midi_program, 33);
    assert_eq!(track.midi.bank, MidiBank::new(1, 3).unwrap());
    round_trip(&score);
    let mut plain = Doc::default().read();
    plain.tracks[0].instrument.midi_program = 25;
    let text = gpif_text(&write_gp(&plain).unwrap());
    assert!(
        text.contains(
            "<Sounds>\n<Sound>\n<MIDI>\n<Program>25</Program>\n</MIDI>\n</Sound>\n</Sounds>"
        ),
        "{text}"
    );
    assert!(!text.contains("<RSE>"), "{text}");
    assert!(!text.contains("<MidiConnection>"), "{text}");
}

#[test]
fn sound_overrides_travel_in_one_container_entry_of_ours() {
    let mut score = Doc::default().read();
    score.container.clear();
    let plain = write_gp(&score).unwrap();
    assert!(!entry_names(&plain).contains(&OVERRIDES_ENTRY.to_string()));
    score.sound_overrides = vec![SoundOverride {
        track: score.tracks[0].id,
        bank: "fluid-r3 \"gm\"".to_string(),
    }];
    let bytes = write_gp(&score).unwrap();
    assert_eq!(
        entry_names(&bytes),
        vec![
            "Content/PartConfiguration".to_string(),
            "Content/score.gpif".to_string(),
            "VERSION".to_string(),
            OVERRIDES_ENTRY.to_string(),
        ]
    );
    assert!(entry_data(&bytes, OVERRIDES_ENTRY).contains("\"track\": 0"));
    let again = read_gp(&bytes).unwrap();
    assert_eq!(again, score);
    assert!(again.container.is_empty());
}

#[test]
fn a_broken_override_entry_is_kept_diagnosed_and_dropped_on_write() {
    let gpif = Doc::default().render();
    for data in [
        "{",
        "{\"sound_overrides\":[{\"track\":3,\"bank\":\"x\"}]}",
        "{\"sound_overrides\":[{\"track\":0,\"bank\":\"a\"},{\"track\":0,\"bank\":\"b\"}]}",
        "{\"sound_overrides\":[],\"extra\":1}",
    ] {
        let bytes = crate::support::archive(&[
            ("VERSION", b"7.0"),
            ("Content/score.gpif", gpif.as_bytes()),
            (OVERRIDES_ENTRY, data.as_bytes()),
        ]);
        let score = read_gp(&bytes).expect(data);
        assert!(score.sound_overrides.is_empty(), "{data}");
        assert_eq!(
            score.container.last(),
            Some(&ContainerEntry::File {
                name: OVERRIDES_ENTRY.to_string(),
                data: Base64Bytes(data.as_bytes().to_vec()),
            }),
            "{data}"
        );
        assert_eq!(
            diagnose(&score),
            vec![opentab_model::Diagnostic::UnreadSoundOverrides],
            "{data}"
        );
        let written = write_gp(&score).expect(data);
        assert_eq!(entry_names(&written), ["VERSION", "Content/score.gpif"]);
        assert!(
            diagnose(&read_gp(&written).expect(data)).is_empty(),
            "{data}"
        );
    }
}

#[test]
fn the_writer_refuses_midi_data_gpif_cannot_hold() {
    let base = Doc::default().read();
    let master = base.master_bars[0].id;
    let at = |position: Fraction, value: AutomationValue| Automation {
        master_bar: master,
        position,
        linear: false,
        visible: true,
        value,
    };
    let mut score = base.clone();
    score.tracks[0].automations = vec![at(
        Fraction::new(1, 7),
        AutomationValue::Volume(Level::FULL),
    )];
    assert!(matches!(
        refusal(&score),
        Refusal::AutomationPosition { .. }
    ));
    score.tracks[0].automations = vec![at(
        Fraction::new(5, 4),
        AutomationValue::Volume(Level::FULL),
    )];
    assert!(matches!(
        refusal(&score),
        Refusal::AutomationPosition { .. }
    ));
    let mut stray = at(
        Fraction::from_integer(0),
        AutomationValue::Pan(Balance::LEFT),
    );
    stray.master_bar = MasterBarId(ElementId(999));
    score.tracks[0].automations = vec![stray];
    assert_eq!(refusal(&score), Refusal::AutomationBar { master_bar: 999 });
    score.tracks[0].automations = vec![at(
        Fraction::from_integer(0),
        AutomationValue::Sound(SoundRef(0)),
    )];
    assert_eq!(refusal(&score), Refusal::SoundRef { index: 0 });
    let twin = Sound {
        name: "Twin".to_string(),
        program: 0,
        bank: MidiBank::default(),
    };
    score.tracks[0].sounds = vec![twin.clone(), twin];
    assert_eq!(refusal(&score), Refusal::AmbiguousSound { index: 0 });
    score.tracks[0].automations.clear();
    score.tracks[0].sounds[0].program = 5;
    assert_eq!(refusal(&score), Refusal::FirstSound);
    let mut kinds = base.clone();
    kinds.tracks[0].instrument.set = Some(InstrumentSet::DrumKit);
    assert_eq!(refusal(&kinds), Refusal::InstrumentSet);
    let mut programs = base.clone();
    programs.tracks[0].instrument.midi_program = 200;
    assert_eq!(refusal(&programs), Refusal::Program { program: 200 });
    let mut orphan = base.clone();
    orphan.sound_overrides = vec![SoundOverride {
        track: TrackId(ElementId(999)),
        bank: "x".to_string(),
    }];
    assert_eq!(refusal(&orphan), Refusal::SoundOverrideTrack { track: 999 });
    let mut twice = base.clone();
    let track = twice.tracks[0].id;
    twice.sound_overrides = ["a", "b"]
        .into_iter()
        .map(|bank| SoundOverride {
            track,
            bank: bank.to_string(),
        })
        .collect();
    assert_eq!(
        refusal(&twice),
        Refusal::DuplicateSoundOverride { track: track.0.0 }
    );
    let mut order = base.clone();
    order.tracks[0].sounds = vec![Sound {
        name: "First".to_string(),
        program: order.tracks[0].instrument.midi_program,
        bank: order.tracks[0].midi.bank,
    }];
    order.tracks[0].automations = vec![
        at(
            Fraction::from_integer(0),
            AutomationValue::Sound(SoundRef(0)),
        ),
        at(
            Fraction::from_integer(0),
            AutomationValue::Volume(Level::FULL),
        ),
    ];
    assert_eq!(refusal(&order), Refusal::AutomationOrder);
    order.tracks[0].automations.reverse();
    assert!(write_gp(&order).is_ok());
    order.tracks[0].automations = vec![
        at(
            Fraction::from_integer(0),
            AutomationValue::Volume(Level::FULL),
        ),
        at(
            Fraction::from_integer(0),
            AutomationValue::Pan(Balance::CENTRE),
        ),
    ];
    assert_eq!(refusal(&order), Refusal::AutomationOrder);
    order.tracks[0].automations.reverse();
    assert!(write_gp(&order).is_ok());
}

#[test]
fn an_unknown_instrument_type_survives_with_no_set() {
    let score = Doc {
        instrument: "banjo".to_string(),
        ..Doc::default()
    }
    .read();
    let again = round_trip(&score);
    assert!(gpif_text(&write_gp(&again).unwrap()).contains("<Type>banjo</Type>"));
    let reread = read_gpif(&gpif_text(&write_gp(&again).unwrap()));
    assert_eq!(reread.tracks[0].instrument.set, None);
}

fn drum_doc(connection: &str) -> Doc {
    Doc {
        instrument: "drumKit".to_string(),
        tuning: "0 0 0 0 0 0".to_string(),
        note: "<InstrumentArticulation>3</InstrumentArticulation>".to_string(),
        track: connection.to_string(),
        ..Doc::default()
    }
}

const DEFAULT_DRUMS: &str = "<MidiConnection><Port>0</Port><PrimaryChannel>9</PrimaryChannel><SecondaryChannel>9</SecondaryChannel><ForeOneChannelPerString>false</ForeOneChannelPerString></MidiConnection>";

#[test]
fn a_midi_connection_that_was_read_is_written_even_at_its_default() {
    let drums = drum_doc(DEFAULT_DRUMS).read();
    let mut kits = drums.clone();
    kits.tracks[0].drum_kit = Some(opentab_model::gm_drum_kit());
    let text = gpif_text(&write_gp(&kits).unwrap());
    assert!(
        text.contains("<MidiConnection>\n<Port>0</Port>\n<PrimaryChannel>9</PrimaryChannel>\n<SecondaryChannel>9</SecondaryChannel>\n<ForeOneChannelPerString>false</ForeOneChannelPerString>\n</MidiConnection>"),
        "{text}"
    );
    round_trip(&kits);
    let pitched = with_track(
        "<MidiConnection><Port>0</Port><PrimaryChannel>0</PrimaryChannel><SecondaryChannel>0</SecondaryChannel><ForeOneChannelPerString>false</ForeOneChannelPerString></MidiConnection>",
    );
    let again = round_trip(&pitched);
    assert!(gpif_text(&write_gp(&again).unwrap()).contains("<MidiConnection>"));
    let mut moved = pitched;
    moved.tracks[0].midi.primary = MidiChannel::new(3).unwrap();
    let text = gpif_text(&write_gp(&moved).unwrap());
    assert_eq!(text.matches("<MidiConnection>").count(), 1);
    assert!(
        text.contains("<PrimaryChannel>3</PrimaryChannel>"),
        "{text}"
    );
    let absent = Doc::default().read();
    assert!(!gpif_text(&write_gp(&absent).unwrap()).contains("<MidiConnection>"));
}

#[test]
fn a_track_without_an_instrument_set_saves_with_a_derived_one() {
    let rendered = Doc::default().render();
    let start = rendered.find("<InstrumentSet>").unwrap();
    let end = rendered.find("</InstrumentSet>").unwrap() + "</InstrumentSet>".len();
    let gpif = format!("{}{}", &rendered[..start], &rendered[end..]);
    let mut score = read_gpif(&gpif);
    assert_eq!(score.tracks[0].instrument.set, None);
    assert_eq!(diagnose(&score), Vec::new());
    for (program, set) in [
        (0, InstrumentSet::AcousticPiano),
        (25, InstrumentSet::ElectricGuitar),
        (33, InstrumentSet::ElectricBass),
        (90, InstrumentSet::ElectricGuitar),
    ] {
        score.tracks[0].instrument.midi_program = program;
        let again = read_gp(&write_gp(&score).unwrap()).unwrap();
        assert_eq!(again.tracks[0].instrument.set, Some(set), "{program}");
    }
}

#[test]
fn the_reserved_entry_is_always_regenerated_from_the_overrides() {
    let base = Doc::default().read();
    for data in [
        &b"garbage"[..],
        br#"{"sound_overrides":[{"track":0,"bank":"stale"}]}"#,
    ] {
        let mut score = base.clone();
        score.container.push(ContainerEntry::File {
            name: OVERRIDES_ENTRY.to_string(),
            data: Base64Bytes(data.to_vec()),
        });
        let bytes = write_gp(&score).unwrap();
        assert!(!entry_names(&bytes).contains(&OVERRIDES_ENTRY.to_string()));
        let again = read_gp(&bytes).unwrap();
        assert!(again.sound_overrides.is_empty());
        assert!(
            again
                .container
                .iter()
                .all(|entry| entry.name() != OVERRIDES_ENTRY)
        );
        score.sound_overrides = vec![SoundOverride {
            track: score.tracks[0].id,
            bank: "fresh".to_string(),
        }];
        let bytes = write_gp(&score).unwrap();
        assert_eq!(
            entry_names(&bytes)
                .iter()
                .filter(|name| *name == OVERRIDES_ENTRY)
                .count(),
            1
        );
        assert_eq!(
            read_gp(&bytes).unwrap().sound_overrides,
            score.sound_overrides
        );
    }
}

#[test]
fn a_non_canonical_bare_sound_keeps_its_explicit_bank() {
    let score = with_track(
        "<Sounds><Sound><MIDI><Program>25</Program><MSB>0</MSB><LSB>0</LSB></MIDI></Sound></Sounds>",
    );
    assert_eq!(track(&score).sounds.len(), 1);
    let text = gpif_text(&write_gp(&score).unwrap());
    assert!(
        text.contains("<LSB>0</LSB>") && text.contains("<MSB>0</MSB>"),
        "{text}"
    );
    round_trip(&score);
    let canonical = with_track(
        "<Sounds><Sound><MIDI><LSB>3</LSB><MSB>1</MSB><Program>25</Program></MIDI></Sound></Sounds>",
    );
    assert!(track(&canonical).sounds.is_empty());
    assert!(gpif_text(&write_gp(&canonical).unwrap()).contains(
        "<Sound>\n<MIDI>\n<LSB>3</LSB>\n<MSB>1</MSB>\n<Program>25</Program>\n</MIDI>\n</Sound>"
    ));
}

#[test]
fn parameters_beyond_six_places_round_to_millionths_and_keep_their_text() {
    let parameters = "0.5 0.5 0.5 0.5 0.5 0.5 0.5 0.5 0.5 0 0.5 0.1234565 0.6666667 0.5 0.5 0.5";
    let score = with_track(&strip(parameters, ""));
    assert_eq!(track(&score).midi.volume, Level::new(666_667).unwrap());
    assert_eq!(track(&score).midi.pan, Balance::new(123_457).unwrap());
    assert!(gpif_text(&write_gp(&score).unwrap()).contains(parameters));
    round_trip(&score);
}

#[test]
fn out_of_range_programs_are_read_errors() {
    for track in [
        "<Sounds><Sound><MIDI><Program>200</Program></MIDI></Sound></Sounds>",
        "<Sounds><Sound><Name><![CDATA[x]]></Name><MIDI><Program>128</Program></MIDI></Sound></Sounds>",
    ] {
        let error = Doc {
            track: track.to_string(),
            ..Doc::default()
        }
        .error();
        assert!(error.to_string().contains("outside 0..=127"), "{error}");
    }
}

#[test]
fn twin_sound_names_the_writer_tells_apart_survive_an_edit_and_its_undo() {
    let sounds = format!(
        "<Sounds>{}{}</Sounds>",
        sound("Clean", "Stringed/Clean", 0),
        sound("Clean", "Stringed/Other", 30)
    );
    let automations = format!(
        "<Automations>{}</Automations>",
        automation("Sound", "0", "<![CDATA[Stringed/Other;Clean;Factory]]>"),
    );
    let mut score = with_track(&format!("{sounds}{automations}"));
    assert_eq!(
        track(&score).automations[0].value,
        AutomationValue::Sound(SoundRef(1))
    );
    let original = write_gp(&score).expect("the writer tells the twins apart");
    let mut history = opentab_command::History::new();
    let track = score.tracks[0].id;
    history
        .execute(
            &mut score,
            opentab_command::Command::RemoveAutomation { track, index: 0 },
        )
        .expect("remove");
    assert!(history.undo(&mut score).expect("undo"));
    assert_eq!(write_gp(&score).expect("write"), original);
}
