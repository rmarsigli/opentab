use std::collections::{BTreeMap, BTreeSet, VecDeque};

use opentab_model::{Fraction, InstrumentKind, MidiChannel, NoteValue, Score};
use opentab_playback::{
    Channel, ChannelState, MidiMessage, PAN_CONTROLLER, Performance, PlaybackOptions,
    PlaybackWarning, Profile, ScoreTime, VOLUME_CONTROLLER, perform,
};
use opentab_testkit::arb_score;
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::{FileFailurePersistence, RngSeed};

use crate::build::{BeatSpec, Builder, beat, fretted, rest, with};

const PROFILES: [Profile; 2] = [Profile::Gp, Profile::Musical];
const PROPERTY_SEED: u64 = 0x0074_3109;
const MAX_TRACKS: usize = 24;
const MAX_KEY: u8 = 127;
const MAX_CHAIN: usize = 16;
const BEATS_PER_BAR: usize = 4;

fn config() -> ProptestConfig {
    ProptestConfig {
        failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
        rng_seed: RngSeed::Fixed(PROPERTY_SEED),
        ..ProptestConfig::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct NoteKey {
    track: usize,
    channel: u8,
    key: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Sounded {
    track: usize,
    start: Fraction,
    end: Fraction,
    key: u8,
}

fn paired(performance: &Performance) -> Result<Vec<Sounded>, TestCaseError> {
    let mut open: BTreeMap<NoteKey, VecDeque<ScoreTime>> = BTreeMap::new();
    let mut out = Vec::new();
    for event in &performance.events {
        let (key, on) = match event.message {
            MidiMessage::NoteOn { key, velocity } => {
                prop_assert!(velocity > 0 && velocity <= MAX_KEY);
                (key, true)
            }
            MidiMessage::NoteOff { key, .. } => (key, false),
            _ => continue,
        };
        prop_assert!(key <= MAX_KEY);
        let slot = NoteKey {
            track: event.track,
            channel: event.channel.get(),
            key,
        };
        if on {
            open.entry(slot).or_default().push_back(event.at);
        } else {
            let start = open.get_mut(&slot).and_then(VecDeque::pop_front);
            prop_assert!(start.is_some(), "a note-off without a note-on: {:?}", event);
            let start = start.unwrap_or(ScoreTime::ZERO);
            prop_assert!(start < event.at, "a note without a positive duration");
            out.push(Sounded {
                track: event.track,
                start: start.whole_notes(),
                end: event.at.whole_notes(),
                key,
            });
        }
    }
    prop_assert!(
        open.values().all(VecDeque::is_empty),
        "a note-on is never released"
    );
    out.sort();
    Ok(out)
}

fn replay(performance: &Performance, track: usize, at: ScoreTime) -> Vec<ChannelState> {
    let mut states: Vec<(Channel, ChannelState)> = performance.tracks[track]
        .channels
        .iter()
        .map(|setup| (setup.channel, setup.state))
        .collect();
    for event in performance
        .events
        .iter()
        .filter(|event| event.track == track && event.at <= at)
    {
        let Some((_, state)) = states
            .iter_mut()
            .find(|(channel, _)| *channel == event.channel)
        else {
            continue;
        };
        match event.message {
            MidiMessage::Program { program } => state.program = program,
            MidiMessage::Bank { bank } => state.bank = bank,
            MidiMessage::Control {
                controller: VOLUME_CONTROLLER,
                value,
            } => state.volume = value,
            MidiMessage::Control {
                controller: PAN_CONTROLLER,
                value,
            } => state.pan = value,
            MidiMessage::Rpn { value, .. } => state.bend_range = value,
            MidiMessage::PitchBend { value } => state.bend = value,
            _ => {}
        }
    }
    states.into_iter().map(|(_, state)| state).collect()
}

fn chased(performance: &Performance, track: usize, at: ScoreTime) -> Vec<ChannelState> {
    let events = performance.chase(track, at);
    let mut states: Vec<(Channel, ChannelState)> = Vec::new();
    for event in events {
        if !states.iter().any(|(channel, _)| *channel == event.channel) {
            states.push((event.channel, ChannelState::default()));
        }
        if let Some((_, state)) = states
            .iter_mut()
            .find(|(channel, _)| *channel == event.channel)
        {
            state.apply(event.message);
        }
    }
    states.into_iter().map(|(_, state)| state).collect()
}

fn check(score: &Score, profile: Profile) -> Result<(), TestCaseError> {
    let options = PlaybackOptions { profile };
    let performance = match perform(score, &options) {
        Ok(performance) => performance,
        Err(error) => return Err(TestCaseError::fail(format!("{error:?}"))),
    };
    prop_assert_eq!(&performance, &perform(score, &options).expect("second run"));
    for pair in performance.events.windows(2) {
        prop_assert!(pair[0].at <= pair[1].at && pair[0].seq < pair[1].seq);
    }
    paired(&performance)?;
    if profile == Profile::Musical {
        shared_only_with_fallback(score, &performance)?;
    }
    for (index, track) in score.tracks.iter().enumerate() {
        let setup = &performance.tracks[index];
        if track.instrument.kind == InstrumentKind::Percussion {
            prop_assert_eq!(setup.channel, Channel::DRUMS);
        }
        let mut times: Vec<ScoreTime> = vec![ScoreTime::ZERO, performance.order.end()];
        times.extend(
            performance
                .events
                .iter()
                .filter(|event| event.track == index)
                .map(|event| event.at),
        );
        for at in times.iter().step_by(3) {
            prop_assert_eq!(
                chased(&performance, index, *at),
                replay(&performance, index, *at)
            );
        }
    }
    prop_assert_eq!(
        performance.clicks.len(),
        opentab_playback::beat_grid(score, &performance.order).len()
    );
    Ok(())
}

fn shared_only_with_fallback(
    score: &Score,
    performance: &Performance,
) -> Result<(), TestCaseError> {
    let fallbacks: Vec<usize> = performance
        .warnings
        .iter()
        .filter_map(|warning| match warning {
            PlaybackWarning::ChannelFallback { track, .. } => Some(*track),
            _ => None,
        })
        .collect();
    let mut owners: BTreeSet<Channel> = BTreeSet::new();
    for (index, track) in score.tracks.iter().enumerate() {
        let channel = performance.tracks[index].channel;
        if track.instrument.kind == InstrumentKind::Percussion {
            prop_assert_eq!(channel, Channel::DRUMS);
            prop_assert!(!fallbacks.contains(&index));
            continue;
        }
        prop_assert_ne!(channel, Channel::DRUMS);
        let first = owners.insert(channel);
        prop_assert_eq!(
            fallbacks.contains(&index),
            !first,
            "track {} on {:?}",
            index,
            channel
        );
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct WideTrack {
    percussion: bool,
    primary: u8,
}

fn wide_score(tracks: &[WideTrack]) -> Score {
    let mut builder = Builder::new(1);
    for spec in tracks {
        let kind = if spec.percussion {
            InstrumentKind::Percussion
        } else {
            InstrumentKind::Fretted
        };
        let track = builder.track(kind, &crate::build::STANDARD, 0);
        builder.score.tracks[track].midi.primary = MidiChannel::new(spec.primary).expect("channel");
        let note = if spec.percussion {
            crate::build::hit(0)
        } else {
            fretted(0, 0)
        };
        builder.voice(track, 0, 0, vec![beat(NoteValue::Whole, vec![note])]);
    }
    builder.score
}

fn arb_wide() -> impl Strategy<Value = Vec<WideTrack>> {
    prop::collection::vec(
        (prop::bool::weighted(0.2), 0u8..16).prop_map(|(percussion, primary)| WideTrack {
            percussion,
            primary,
        }),
        1..=MAX_TRACKS,
    )
}

#[derive(Debug, Clone, Copy)]
struct ChainStep {
    fret: Option<u8>,
    origin: bool,
    destination: bool,
}

fn arb_chain() -> impl Strategy<Value = Vec<ChainStep>> {
    prop::collection::vec(
        (
            prop::option::weighted(0.85, 0u8..2),
            any::<bool>(),
            any::<bool>(),
        )
            .prop_map(|(fret, origin, destination)| ChainStep {
                fret,
                origin,
                destination,
            }),
        1..=MAX_CHAIN,
    )
}

fn chain_score(steps: &[ChainStep]) -> Score {
    let bars = steps.len().div_ceil(BEATS_PER_BAR);
    let mut builder = Builder::new(bars);
    let track = builder.guitar();
    for (bar, chunk) in steps.chunks(BEATS_PER_BAR).enumerate() {
        let beats: Vec<BeatSpec> = chunk
            .iter()
            .map(|step| match step.fret {
                Some(fret) => beat(
                    NoteValue::Quarter,
                    vec![with(fretted(0, fret), |effects| {
                        effects.tie_origin = step.origin;
                        effects.tie_destination = step.destination;
                    })],
                ),
                None => rest(NoteValue::Quarter),
            })
            .collect();
        builder.voice(track, bar, 0, beats);
    }
    builder.score
}

fn chain_reference(steps: &[ChainStep]) -> Vec<Sounded> {
    let quarter = Fraction::new(1, 4);
    let mut out: Vec<Sounded> = Vec::new();
    let mut previous: Option<(ChainStep, usize)> = None;
    for (index, step) in steps.iter().enumerate() {
        let start = quarter * Fraction::from_integer(i64::try_from(index).expect("index"));
        let Some(fret) = step.fret else {
            previous = None;
            continue;
        };
        let key = 64 + fret;
        let continues = previous.is_some_and(|(before, _)| {
            step.destination && before.origin && before.fret == Some(fret)
        });
        match previous {
            Some((_, sounding)) if continues => out[sounding].end = start + quarter,
            _ => {
                out.push(Sounded {
                    track: 0,
                    start,
                    end: start + quarter,
                    key,
                });
            }
        }
        previous = Some((*step, out.len() - 1));
    }
    out.sort();
    out
}

#[derive(Debug, Clone)]
struct HighNote {
    string: u8,
    fret: u8,
}

fn arb_high() -> impl Strategy<Value = (Vec<u8>, u8, Vec<Vec<HighNote>>)> {
    (prop::collection::vec(96u8..=127, 1..=6), 0u8..=12).prop_flat_map(|(tuning, capo)| {
        let strings = u8::try_from(tuning.len()).expect("strings");
        let beats = prop::collection::vec(
            prop::collection::vec(
                (0..strings, 0u8..=24).prop_map(|(string, fret)| HighNote { string, fret }),
                0..=3,
            ),
            1..=8,
        );
        (Just(tuning), Just(capo), beats)
    })
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn every_score_performs_deterministically_with_paired_notes_and_chase(score in arb_score()) {
        for profile in PROFILES {
            check(&score, profile)?;
        }
    }

    #[test]
    fn under_musical_melodic_tracks_share_a_channel_only_with_a_reported_fallback(tracks in arb_wide()) {
        let score = wide_score(&tracks);
        let musical = perform(&score, &PlaybackOptions { profile: Profile::Musical }).expect("perform");
        shared_only_with_fallback(&score, &musical)?;
        paired(&musical)?;
        let gp = perform(&score, &PlaybackOptions { profile: Profile::Gp }).expect("perform");
        prop_assert!(gp.warnings.is_empty());
    }

    #[test]
    fn tied_notes_pair_once_for_their_whole_chain(steps in arb_chain()) {
        let score = chain_score(&steps);
        for profile in PROFILES {
            let performance = perform(&score, &PlaybackOptions { profile }).expect("perform");
            prop_assert_eq!(paired(&performance)?, chain_reference(&steps));
        }
    }

    #[test]
    fn notes_above_127_are_dropped_reported_and_never_unpair((tuning, capo, beats) in arb_high()) {
        let mut builder = Builder::new(2);
        let track = builder.track(InstrumentKind::Fretted, &tuning, capo);
        let mut expected_drops = 0;
        let mut total = 0;
        let mut specs = Vec::new();
        for notes in &beats {
            let mut chord = Vec::new();
            for note in notes {
                total += 1;
                let pitch = u16::from(tuning[usize::from(note.string)]) + u16::from(capo) + u16::from(note.fret);
                if pitch > u16::from(MAX_KEY) {
                    expected_drops += 1;
                }
                chord.push(fretted(note.string, note.fret));
            }
            specs.push(beat(NoteValue::Eighth, chord));
        }
        builder.voice(track, 0, 0, specs);
        let performance = perform(&builder.score, &PlaybackOptions { profile: Profile::Gp }).expect("perform");
        let sounded = paired(&performance)?;
        let drops = performance
            .warnings
            .iter()
            .filter(|warning| matches!(warning, PlaybackWarning::PitchOutOfRange { pitch, .. } if *pitch > u16::from(MAX_KEY)))
            .count();
        prop_assert_eq!(drops, expected_drops);
        prop_assert_eq!(sounded.len() + drops, total);
    }
}

#[test]
fn every_generated_score_performs() {
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let mut performed = 0;
    let cases = 512;
    for _ in 0..cases {
        let score = arb_score().new_tree(&mut runner).expect("tree").current();
        if perform(&score, &PlaybackOptions::default()).is_ok() {
            performed += 1;
        }
    }
    assert!(performed == cases, "{performed} of {cases}");
}
