use std::io::{Cursor, Read};

use opentab_gp::{GpError, Refusal, read_gp, write_gp};
use opentab_model::{
    Automation, AutomationValue, Balance, Fraction, InstrumentKind, InstrumentSet, Level, MidiBank,
    MidiChannel, Score, Sound, SoundOverride, SoundRef, TrackMidi,
};
use zip::ZipArchive;

use crate::anchors::gpif_text;
use crate::support::{Doc, payloads, read_gpif, track};

const OVERRIDES_ENTRY: &str = "Content/OpenTab/sound-overrides.json";

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
            AutomationValue::Volume(Level::new(812_500).unwrap()),
            AutomationValue::Pan(Balance::new(100_000).unwrap()),
        ]
    );
    assert_eq!(track.automations[1].position, Fraction::new(1, 2));
    round_trip(&score);
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
                position: Fraction::from_integer(1),
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
    assert!(text.contains("<Position>0.5</Position>"), "{text}");
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
fn a_broken_override_entry_is_an_error() {
    let gpif = Doc::default().render();
    for data in ["{", "{\"sound_overrides\":[{\"track\":3,\"bank\":\"x\"}]}"] {
        let bytes = crate::support::archive(&[
            ("VERSION", b"7.0"),
            ("Content/score.gpif", gpif.as_bytes()),
            (OVERRIDES_ENTRY, data.as_bytes()),
        ]);
        assert!(
            matches!(read_gp(&bytes), Err(GpError::Entry { ref entry, .. }) if entry == OVERRIDES_ENTRY),
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
    stray.master_bar = opentab_model::MasterBarId(opentab_model::ElementId(999));
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
    kinds.tracks[0].instrument.set = None;
    assert_eq!(refusal(&kinds), Refusal::NoInstrumentSet);
    let mut orphan = base;
    orphan.sound_overrides = vec![SoundOverride {
        track: opentab_model::TrackId(opentab_model::ElementId(999)),
        bank: "x".to_string(),
    }];
    assert_eq!(refusal(&orphan), Refusal::SoundOverrideTrack { track: 999 });
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
