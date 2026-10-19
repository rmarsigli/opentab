use std::collections::{BTreeMap, VecDeque};

use midly::{Format, MetaMessage, Smf, Timing, TrackEventKind};
use opentab_gp::read_gp;
use opentab_model::{Bend, BendPoint, Fraction, InstrumentKind, MidiChannel, NoteValue, Score};
use opentab_playback::{
    MidiMessage, Performance, PlaybackOptions, Ppq, Profile, ScoreTime, TimedEvent, TrackLayout,
    export_midi, perform, to_ticks, write_smf,
};
use opentab_testkit::arb_score;
use proptest::prelude::*;
use proptest::test_runner::{FileFailurePersistence, RngSeed};

use crate::build::{Builder, STANDARD, beat, fretted, hit, with};
use crate::gp_oracle::{golden_names, root};

const PROFILES: [Profile; 2] = [Profile::Gp, Profile::Musical];
const PROPERTY_SEED: u64 = 0x0074_3110;
const WIDE_TRACKS: usize = 20;
const DRUM_TRACKS: [usize; 2] = [7, 15];
const BANK_MSB: u8 = 0;
const BANK_LSB: u8 = 32;
const RPN_MSB: u8 = 101;
const RPN_LSB: u8 = 100;
const DATA_ENTRY: u8 = 6;
const SEVEN_BITS: u32 = 7;
const LOW_SEVEN: u16 = 0x7f;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Wire {
    NoteOn { key: u8, velocity: u8 },
    NoteOff { key: u8, velocity: u8 },
    Control { controller: u8, value: u8 },
    Program { program: u8 },
    PitchBend { value: u16 },
    Tempo { micros: u32 },
    TimeSignature { numerator: u8, power: u8 },
    KeySignature { accidentals: i8, minor: bool },
    Name,
    Port,
    EndOfTrack,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Decoded {
    pub tick: u64,
    pub channel: u8,
    pub wire: Wire,
}

#[derive(Debug, Clone)]
pub struct Reparsed {
    pub division: u16,
    pub tracks: Vec<Vec<Decoded>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Slot {
    track: usize,
    channel: u8,
    key: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pairing {
    pub notes: usize,
    pub shortest: Option<u64>,
}

fn midi_wire(message: midly::MidiMessage) -> Wire {
    match message {
        midly::MidiMessage::NoteOn { key, vel } if vel.as_int() == 0 => Wire::NoteOff {
            key: key.as_int(),
            velocity: 0,
        },
        midly::MidiMessage::NoteOn { key, vel } => Wire::NoteOn {
            key: key.as_int(),
            velocity: vel.as_int(),
        },
        midly::MidiMessage::NoteOff { key, vel } => Wire::NoteOff {
            key: key.as_int(),
            velocity: vel.as_int(),
        },
        midly::MidiMessage::Controller { controller, value } => Wire::Control {
            controller: controller.as_int(),
            value: value.as_int(),
        },
        midly::MidiMessage::ProgramChange { program } => Wire::Program {
            program: program.as_int(),
        },
        midly::MidiMessage::PitchBend { bend } => Wire::PitchBend {
            value: bend.0.as_int(),
        },
        midly::MidiMessage::Aftertouch { .. } | midly::MidiMessage::ChannelAftertouch { .. } => {
            Wire::Other
        }
    }
}

fn meta_wire(message: MetaMessage<'_>) -> Wire {
    match message {
        MetaMessage::Tempo(micros) => Wire::Tempo {
            micros: micros.as_int(),
        },
        MetaMessage::TimeSignature(numerator, power, _, _) => {
            Wire::TimeSignature { numerator, power }
        }
        MetaMessage::KeySignature(accidentals, minor) => Wire::KeySignature { accidentals, minor },
        MetaMessage::TrackName(_) => Wire::Name,
        MetaMessage::MidiPort(_) => Wire::Port,
        MetaMessage::EndOfTrack => Wire::EndOfTrack,
        _ => Wire::Other,
    }
}

pub fn reparse(bytes: &[u8]) -> Reparsed {
    let smf = Smf::parse(bytes).expect("midly reparses the file");
    assert_eq!(smf.header.format, Format::Parallel);
    let Timing::Metrical(division) = smf.header.timing else {
        panic!("metrical timing expected");
    };
    let tracks = smf
        .tracks
        .iter()
        .map(|track| {
            let mut tick = 0;
            track
                .iter()
                .map(|event| {
                    tick += u64::from(event.delta.as_int());
                    let (channel, wire) = match event.kind {
                        TrackEventKind::Midi { channel, message } => {
                            (channel.as_int(), midi_wire(message))
                        }
                        TrackEventKind::Meta(message) => (0, meta_wire(message)),
                        TrackEventKind::SysEx(_) | TrackEventKind::Escape(_) => (0, Wire::Other),
                    };
                    Decoded {
                        tick,
                        channel,
                        wire,
                    }
                })
                .collect()
        })
        .collect();
    Reparsed {
        division: division.as_int(),
        tracks,
    }
}

pub fn pair_notes(reparsed: &Reparsed) -> Result<Pairing, String> {
    let mut open: BTreeMap<Slot, VecDeque<u64>> = BTreeMap::new();
    let mut pairing = Pairing::default();
    for (track, events) in reparsed.tracks.iter().enumerate() {
        if events.last().map(|event| event.wire) != Some(Wire::EndOfTrack) {
            return Err(format!("track {track} does not end with an end-of-track"));
        }
        for event in events {
            match event.wire {
                Wire::NoteOn { key, .. } => open
                    .entry(Slot {
                        track,
                        channel: event.channel,
                        key,
                    })
                    .or_default()
                    .push_back(event.tick),
                Wire::NoteOff { key, .. } => {
                    let slot = Slot {
                        track,
                        channel: event.channel,
                        key,
                    };
                    let Some(start) = open.get_mut(&slot).and_then(VecDeque::pop_front) else {
                        return Err(format!(
                            "a note-off without its note-on: {slot:?} {event:?}"
                        ));
                    };
                    let Some(length) = event.tick.checked_sub(start).filter(|length| *length > 0)
                    else {
                        return Err(format!(
                            "a note of zero or negative length: {slot:?} {event:?}"
                        ));
                    };
                    pairing.notes += 1;
                    pairing.shortest = Some(pairing.shortest.map_or(length, |old| old.min(length)));
                }
                Wire::Port => return Err("a port meta event was written".to_string()),
                _ => {}
            }
        }
    }
    match open.iter().find(|(_, starts)| !starts.is_empty()) {
        Some((slot, _)) => Err(format!("a note-on is never released: {slot:?}")),
        None => Ok(pairing),
    }
}

pub fn expected_tracks(performance: &Performance) -> usize {
    1 + match performance.profile.track_layout() {
        TrackLayout::PerTrack => performance.tracks.len(),
        TrackLayout::PerStaff => performance.tracks.iter().map(|track| track.staves).sum(),
    }
}

pub fn note_ons(performance: &Performance) -> usize {
    performance
        .events
        .iter()
        .filter(|event| matches!(event.message, MidiMessage::NoteOn { .. }))
        .count()
}

fn stream_wires(event: &TimedEvent, ppq: Ppq) -> Vec<Decoded> {
    let wire = |wire: Wire| Decoded {
        tick: to_ticks(event.at, ppq),
        channel: event.channel.get(),
        wire,
    };
    let control = |controller: u8, value: u8| wire(Wire::Control { controller, value });
    match event.message {
        MidiMessage::NoteOn { key, velocity } => vec![wire(Wire::NoteOn { key, velocity })],
        MidiMessage::NoteOff { key, velocity } => vec![wire(Wire::NoteOff { key, velocity })],
        MidiMessage::Program { program } => vec![wire(Wire::Program { program })],
        MidiMessage::Bank { bank } => {
            vec![control(BANK_MSB, bank.msb()), control(BANK_LSB, bank.lsb())]
        }
        MidiMessage::Control { controller, value } => vec![control(controller, value)],
        MidiMessage::Rpn { parameter, value } => vec![
            control(
                RPN_MSB,
                u8::try_from(parameter >> SEVEN_BITS).expect("seven bits"),
            ),
            control(
                RPN_LSB,
                u8::try_from(parameter & LOW_SEVEN).expect("seven bits"),
            ),
            control(DATA_ENTRY, value),
        ],
        MidiMessage::PitchBend { value } => vec![wire(Wire::PitchBend { value })],
    }
}

fn channel_events(decoded: &[Decoded]) -> Vec<Decoded> {
    decoded
        .iter()
        .filter(|event| {
            matches!(
                event.wire,
                Wire::NoteOn { .. }
                    | Wire::NoteOff { .. }
                    | Wire::Control { .. }
                    | Wire::Program { .. }
                    | Wire::PitchBend { .. }
            )
        })
        .copied()
        .collect()
}

fn wide_score() -> Score {
    let mut builder = Builder::new(2);
    let rise = Bend::new(
        BendPoint::new(0, 0).expect("point"),
        BendPoint::new(50, 200).expect("point"),
        BendPoint::new(50, 200).expect("point"),
        BendPoint::new(100, 200).expect("point"),
    )
    .expect("bend");
    for index in 0..WIDE_TRACKS {
        let drums = DRUM_TRACKS.contains(&index);
        let kind = if drums {
            InstrumentKind::Percussion
        } else {
            InstrumentKind::Fretted
        };
        let track = builder.track(kind, &STANDARD, 0);
        builder.score.tracks[track].midi.primary =
            MidiChannel::new(u8::try_from(index % 3).expect("channel")).expect("channel");
        builder.score.tracks[track].instrument.midi_program = u8::try_from(index).expect("program");
        let fret = u8::try_from(index).expect("fret");
        let notes = if drums {
            vec![
                beat(NoteValue::Half, vec![hit(0)]),
                beat(NoteValue::Half, vec![hit(1)]),
            ]
        } else {
            vec![
                beat(
                    NoteValue::Half,
                    vec![with(fretted(2, fret), |effects| effects.bend = Some(rise))],
                ),
                beat(NoteValue::Quarter, vec![fretted(1, fret)]),
                beat(NoteValue::Quarter, vec![fretted(0, fret)]),
            ]
        };
        builder.voice(track, 0, 0, notes.clone());
        builder.voice(track, 1, 0, notes);
    }
    builder.score
}

#[test]
fn a_twenty_track_musical_export_writes_each_track_with_the_stream_channels() {
    let score = wide_score();
    let performance = perform(
        &score,
        &PlaybackOptions {
            profile: Profile::Musical,
        },
    )
    .expect("perform");
    let reparsed = reparse(&write_smf(&performance, Ppq::MUSICAL));
    assert_eq!(reparsed.division, Ppq::MUSICAL.get());
    assert_eq!(reparsed.tracks.len(), 1 + WIDE_TRACKS);
    let mut channels: Vec<u8> = performance
        .tracks
        .iter()
        .map(|setup| setup.channel.get())
        .collect();
    channels.sort_unstable();
    channels.dedup();
    assert_eq!(
        channels.len(),
        16,
        "the global pool hands out every channel"
    );
    for (index, setup) in performance.tracks.iter().enumerate() {
        let expected: Vec<Decoded> = performance
            .events
            .iter()
            .filter(|event| event.track == index)
            .flat_map(|event| stream_wires(event, Ppq::MUSICAL))
            .collect();
        let written = channel_events(&reparsed.tracks[index + 1]);
        assert_eq!(written, expected, "track {index}");
        assert!(
            written
                .iter()
                .all(|event| event.channel == setup.channel.get()),
            "track {index} stays on {:?}",
            setup.channel
        );
        assert_eq!(reparsed.tracks[index + 1][0].wire, Wire::Name);
    }
    assert!(
        channel_events(&reparsed.tracks[0]).is_empty(),
        "the conductor holds no channel event"
    );
    assert_eq!(
        pair_notes(&reparsed).map(|pairing| pairing.notes),
        Ok(note_ons(&performance))
    );
}

#[test]
fn every_golden_exports_in_both_profiles_and_reparses_with_midly() {
    for name in golden_names() {
        let bytes = std::fs::read(root().join("fixtures/gp").join(&name)).expect("fixture");
        let score = read_gp(&bytes).expect("fixture reads");
        for profile in PROFILES {
            let performance = perform(&score, &PlaybackOptions { profile }).expect("perform");
            let file = export_midi(&score, profile).expect("export");
            assert_eq!(file, write_smf(&performance, profile.ppq()), "{name}");
            let reparsed = reparse(&file);
            assert_eq!(reparsed.division, profile.ppq().get(), "{name}");
            assert_eq!(
                reparsed.tracks.len(),
                expected_tracks(&performance),
                "{name}"
            );
            let pairing = pair_notes(&reparsed).unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(pairing.notes, note_ons(&performance), "{name} {profile:?}");
        }
    }
}

fn moment(numer: i64, denom: i64) -> ScoreTime {
    ScoreTime::new(Fraction::new(numer, denom)).expect("time")
}

#[test]
fn a_sub_tick_note_keeps_one_tick_and_never_overtakes_the_next_note_on_its_key() {
    let mut builder = Builder::new(1);
    let track = builder.guitar();
    builder.voice(
        track,
        0,
        0,
        vec![beat(NoteValue::Whole, vec![fretted(0, 0)])],
    );
    let mut performance = perform(
        &builder.score,
        &PlaybackOptions {
            profile: Profile::Gp,
        },
    )
    .expect("perform");
    let template = *performance
        .events
        .iter()
        .find(|event| matches!(event.message, MidiMessage::NoteOn { .. }))
        .expect("a note");
    performance.events.retain(|event| {
        !matches!(
            event.message,
            MidiMessage::NoteOn { .. } | MidiMessage::NoteOff { .. }
        )
    });
    let timeline = [
        (moment(0, 1), true),
        (moment(1, 100_000), false),
        (moment(2, 100_000), true),
        (moment(1, 1000), false),
        (moment(1, 1000), true),
        (moment(3, 2000), false),
    ];
    for (at, on) in timeline {
        let message = if on {
            MidiMessage::NoteOn {
                key: 64,
                velocity: 76,
            }
        } else {
            MidiMessage::NoteOff {
                key: 64,
                velocity: 64,
            }
        };
        performance.events.push(TimedEvent {
            at,
            message,
            ..template
        });
    }
    let reparsed = reparse(&write_smf(&performance, Ppq::GP));
    let notes: Vec<(u64, bool)> = reparsed.tracks[1]
        .iter()
        .filter_map(|event| match event.wire {
            Wire::NoteOn { .. } => Some((event.tick, true)),
            Wire::NoteOff { .. } => Some((event.tick, false)),
            _ => None,
        })
        .collect();
    assert_eq!(
        notes,
        vec![
            (0, true),
            (1, false),
            (1, true),
            (2, false),
            (2, true),
            (3, false)
        ]
    );
    assert_eq!(
        pair_notes(&reparsed).map(|pairing| pairing.shortest),
        Ok(Some(1))
    );
}

fn config() -> ProptestConfig {
    ProptestConfig {
        failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
        rng_seed: RngSeed::Fixed(PROPERTY_SEED),
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn every_exported_note_reparses_paired_with_a_positive_length(score in arb_score()) {
        for profile in PROFILES {
            let performance = perform(&score, &PlaybackOptions { profile })
                .map_err(|error| TestCaseError::fail(format!("{error:?}")))?;
            let reparsed = reparse(&write_smf(&performance, profile.ppq()));
            prop_assert_eq!(reparsed.tracks.len(), expected_tracks(&performance));
            let pairing = pair_notes(&reparsed).map_err(TestCaseError::fail)?;
            prop_assert_eq!(pairing.notes, note_ons(&performance));
        }
    }
}
