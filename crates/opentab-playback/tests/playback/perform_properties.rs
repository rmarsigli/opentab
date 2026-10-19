use std::collections::{BTreeMap, BTreeSet, VecDeque};

use opentab_model::{Fraction, InstrumentKind, MidiChannel, NoteValue, Score};
use opentab_playback::{
    BEND_CENTRE, Channel, ChannelState, MidiMessage, PAN_CONTROLLER, Performance, PlaybackOptions,
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
const CHASE_POINTS: usize = 48;

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
        bends_reach_only_their_own_note(&performance)?;
        channels_are_set_up_only_while_idle(&performance)?;
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
        let stride = (times.len() / CHASE_POINTS).max(1);
        for at in times.iter().step_by(stride) {
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
    let fallbacks = reported(performance);
    let mut owners: BTreeSet<Channel> = BTreeSet::new();
    for (index, track) in score.tracks.iter().enumerate() {
        let channel = performance.tracks[index].channel;
        if track.instrument.kind == InstrumentKind::Percussion {
            prop_assert_eq!(channel, Channel::DRUMS);
            prop_assert!(!fallbacks.contains(&(index, ScoreTime::ZERO)));
            continue;
        }
        prop_assert_ne!(channel, Channel::DRUMS);
        let first = owners.insert(channel);
        if !first {
            prop_assert!(
                fallbacks.contains(&(index, ScoreTime::ZERO)),
                "track {} shares {:?} unreported",
                index,
                channel
            );
        }
    }
    Ok(())
}

fn reported(performance: &Performance) -> BTreeSet<(usize, ScoreTime)> {
    performance
        .warnings
        .iter()
        .filter_map(|warning| match warning {
            PlaybackWarning::ChannelFallback { track, at } => Some((*track, *at)),
            _ => None,
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Group {
    track: usize,
    key: u8,
}

#[derive(Debug, Clone, Default)]
struct Opened {
    starts: Vec<ScoreTime>,
    released: usize,
}

type Channels = BTreeMap<Channel, BTreeMap<Group, Opened>>;

fn track_notes(channels: &mut Channels, event: &opentab_playback::TimedEvent) {
    let (key, on) = match event.message {
        MidiMessage::NoteOn { key, .. } => (key, true),
        MidiMessage::NoteOff { key, .. } => (key, false),
        _ => return,
    };
    let groups = channels.entry(event.channel).or_default();
    let group = Group {
        track: event.track,
        key,
    };
    let opened = groups.entry(group).or_default();
    if on {
        opened.starts.push(event.at);
    } else {
        opened.released += 1;
    }
    if opened.starts.len() <= opened.released {
        groups.remove(&group);
    }
}

fn sounding(groups: Option<&BTreeMap<Group, Opened>>) -> usize {
    groups.map_or(0, |groups| {
        groups
            .values()
            .map(|opened| opened.starts.len() - opened.released)
            .sum()
    })
}

fn bends_reach_only_their_own_note(performance: &Performance) -> Result<(), TestCaseError> {
    let fallbacks = reported(performance);
    let mut channels = Channels::new();
    for event in &performance.events {
        track_notes(&mut channels, event);
        let MidiMessage::PitchBend { value } = event.message else {
            continue;
        };
        if value == BEND_CENTRE || event.channel == Channel::DRUMS {
            continue;
        }
        let groups = channels.get(&event.channel);
        if sounding(groups) < 2 {
            continue;
        }
        let reported_own = groups.into_iter().flatten().any(|(group, opened)| {
            group.track == event.track
                && opened
                    .starts
                    .iter()
                    .any(|start| fallbacks.contains(&(group.track, *start)))
        });
        prop_assert!(
            reported_own,
            "a bend at {:?} on {:?} reaches {} notes without a reported fallback",
            event.at,
            event.channel,
            sounding(groups)
        );
    }
    Ok(())
}

fn channels_are_set_up_only_while_idle(performance: &Performance) -> Result<(), TestCaseError> {
    let mut channels = Channels::new();
    for event in &performance.events {
        track_notes(&mut channels, event);
        if !matches!(event.message, MidiMessage::Rpn { .. }) {
            continue;
        }
        let groups = channels.get(&event.channel);
        prop_assert_eq!(
            sounding(groups),
            0,
            "{:?} is set up at {:?} while {:?} sound",
            event.channel,
            event.at,
            groups
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

const MAX_DENSE_TRACKS: usize = 16;
const MAX_CHORD: usize = 4;
const DENSE_BEATS: usize = 4;
const DENSE_EFFECTS: u8 = 11;
const MIN_COVERED: usize = 1;
const COVERAGE_CASES: usize = 96;
const CHASES_PER_KIND: usize = 2;

#[derive(Debug, Clone, Copy)]
struct DenseNote {
    fret: u8,
    effect: u8,
}

fn arb_dense() -> impl Strategy<Value = Vec<Vec<Vec<DenseNote>>>> {
    let note = (0u8..12, 0u8..DENSE_EFFECTS).prop_map(|(fret, effect)| DenseNote { fret, effect });
    let chord = prop::collection::vec(note, 0..=MAX_CHORD);
    let track = prop::collection::vec(chord, DENSE_BEATS..=DENSE_BEATS);
    prop::collection::vec(track, 1..=MAX_DENSE_TRACKS)
}

fn dense_effect(effects: &mut opentab_model::NoteEffects, effect: u8) {
    let rise = opentab_model::BendPoint::new(50, 200).expect("point");
    match effect {
        1 => {
            effects.bend =
                opentab_model::Bend::new(opentab_model::BendPoint::default(), rise, rise, rise)
                    .ok();
        }
        2 => effects.vibrato = Some(opentab_model::Vibrato::Slight),
        3 => effects.slide = Some(opentab_model::Slide::Shift),
        4 => effects.slide = Some(opentab_model::Slide::Legato),
        5 => effects.hammer_on_pull_off = true,
        6 => effects.palm_mute = true,
        7 => effects.slide = Some(opentab_model::Slide::IntoFromBelow),
        8 => effects.let_ring = true,
        9 => effects.dead = true,
        _ => {}
    }
}

fn dense_score(tracks: &[Vec<Vec<DenseNote>>]) -> Score {
    let mut builder = Builder::new(2);
    for beats in tracks {
        let track = builder.guitar();
        let specs: Vec<BeatSpec> = beats
            .iter()
            .map(|chord| {
                let notes = chord
                    .iter()
                    .enumerate()
                    .map(|(string, note)| {
                        let string = u8::try_from(string).expect("string");
                        with(fretted(string, note.fret), |effects| {
                            dense_effect(effects, note.effect);
                        })
                    })
                    .collect();
                beat(NoteValue::Quarter, notes)
            })
            .collect();
        builder.voice(track, 0, 0, specs.clone());
        builder.voice(track, 1, 0, specs);
    }
    builder.score
}

#[derive(Debug, Default)]
struct Covered {
    program: usize,
    bank: usize,
    volume: usize,
    pan: usize,
    bend: usize,
}

#[test]
fn chase_replays_mid_song_program_bank_volume_pan_and_bend_changes() {
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let mut covered = Covered::default();
    for _ in 0..COVERAGE_CASES {
        let score = arb_score().new_tree(&mut runner).expect("tree").current();
        let performance = perform(&score, &PlaybackOptions::default()).expect("perform");
        let mut here = Covered::default();
        for event in &performance.events {
            if event.at == ScoreTime::ZERO {
                continue;
            }
            let counter = match event.message {
                MidiMessage::Program { .. } => &mut here.program,
                MidiMessage::Bank { .. } => &mut here.bank,
                MidiMessage::Control {
                    controller: VOLUME_CONTROLLER,
                    ..
                } => &mut here.volume,
                MidiMessage::Control {
                    controller: PAN_CONTROLLER,
                    ..
                } => &mut here.pan,
                MidiMessage::PitchBend { .. } => &mut here.bend,
                _ => continue,
            };
            if *counter >= CHASES_PER_KIND {
                continue;
            }
            *counter += 1;
            assert_eq!(
                chased(&performance, event.track, event.at),
                replay(&performance, event.track, event.at),
                "{:?}",
                event
            );
        }
        covered.program += here.program;
        covered.bank += here.bank;
        covered.volume += here.volume;
        covered.pan += here.pan;
        covered.bend += here.bend;
    }
    for (kind, count) in [
        ("program", covered.program),
        ("bank", covered.bank),
        ("volume", covered.volume),
        ("pan", covered.pan),
        ("bend", covered.bend),
    ] {
        assert!(count >= MIN_COVERED, "no mid-song {kind} change was chased");
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn dense_musical_bends_reach_only_their_own_note_and_channels_reset_only_while_idle(tracks in arb_dense()) {
        let score = dense_score(&tracks);
        let options = PlaybackOptions { profile: Profile::Musical };
        let performance = perform(&score, &options).expect("perform");
        prop_assert_eq!(&performance, &perform(&score, &options).expect("second run"));
        paired(&performance)?;
        shared_only_with_fallback(&score, &performance)?;
        bends_reach_only_their_own_note(&performance)?;
        channels_are_set_up_only_while_idle(&performance)?;
    }
}
