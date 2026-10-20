use opentab_audio::{
    BarRange, CLICK_BEAT_KEY, CLICK_BEAT_VELOCITY, CLICK_DOWNBEAT_KEY, CLICK_DOWNBEAT_VELOCITY,
    RenderedAudio, SampleRate, ScriptStep, Session, SessionConfig, Synth, TransportCommand,
    frame_at, played_range, range_frames, render_session,
};
use opentab_model::Fraction;
use opentab_playback::{
    BEND_CENTRE, Channel, MidiMessage, Performance, PlayOrder, PlayedBar, Profile, ScoreTime, Speed,
};
use proptest::prelude::*;
use proptest::test_runner::{FileFailurePersistence, RngSeed};

use crate::recording::{Heard, SynthModel, heard, notes, recorders, recording, unit_of};
use crate::support::{PROFILES, built_in_voices, config, fixture, performance};

const PROPERTY_SEED: u64 = 0x0074_3115;
const SEEK_FIXTURES: [&str; 6] = [
    "note-effects.gp",
    "mixer.gp",
    "sound-change.gp",
    "multi-track.gp",
    "tremolo-bar.gp",
    "repeats.gp",
];
const LOOP_FIXTURES: [&str; 4] = [
    "note-effects.gp",
    "repeats.gp",
    "multi-track.gp",
    "trill.gp",
];
const MAX_LOOP_BARS: usize = 4;
const WRAPS: u64 = 3;
const RATE: SampleRate = SampleRate::STUDIO;

fn property_config(cases: u32) -> ProptestConfig {
    ProptestConfig {
        cases,
        failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
        rng_seed: RngSeed::Fixed(PROPERTY_SEED),
        ..ProptestConfig::default()
    }
}

fn speed(percent: u16) -> Speed {
    Speed::new(percent).expect("speed")
}

fn session_config(percent: u16) -> SessionConfig {
    SessionConfig {
        speed: speed(percent),
        ..config(RATE)
    }
}

fn step(frame: u64, command: TransportCommand) -> ScriptStep {
    ScriptStep { frame, command }
}

fn record(
    performance: &Performance,
    config: SessionConfig,
    script: &[ScriptStep],
    frames: u64,
) -> Vec<Vec<Heard>> {
    let recorders = recorders(performance);
    let mut session = Session::new(performance.clone(), recorders.voices, config).expect("session");
    render_session(&mut session, script, frames).expect("render");
    recorders.logs.iter().map(heard).collect()
}

fn reference(performance: &Performance, track: usize, start: ScoreTime, limit: u64) -> SynthModel {
    let mut model = SynthModel::default();
    for event in performance
        .events
        .iter()
        .filter(|event| event.track == track)
        .filter(|event| unit_of(&performance.tempo, event.at, RATE) < limit)
    {
        if event.at < start
            && matches!(
                event.message,
                MidiMessage::NoteOn { .. } | MidiMessage::NoteOff { .. }
            )
        {
            continue;
        }
        model.apply(event.channel, event.message);
    }
    model
}

fn song_frames(performance: &Performance, percent: u16) -> u64 {
    frame_at(
        &performance.tempo,
        performance.order.end(),
        speed(percent),
        RATE,
    )
    .expect("end")
}

fn assert_restored(
    performance: &Performance,
    logs: &[Vec<Heard>],
    bar: usize,
    frame: u64,
    percent: u16,
) -> Result<(), TestCaseError> {
    let start = performance.order.bars[bar].start;
    let limit = unit_of(&performance.tempo, start, RATE) + u64::from(percent);
    for (track, log) in logs.iter().enumerate() {
        let after = SynthModel::of_log(log, frame);
        let expected = reference(performance, track, start, limit);
        prop_assert_eq!(
            &after,
            &expected,
            "track {} bar {} frame {}",
            track,
            bar,
            frame
        );
    }
    Ok(())
}

proptest! {
    #![proptest_config(property_config(48))]

    #[test]
    fn after_a_seek_every_synth_holds_the_state_of_continuous_play_at_that_bar(
        fixture_index in 0..SEEK_FIXTURES.len(),
        musical in any::<bool>(),
        bar_pick in 0usize..10_000,
        frame_pick in 1u64..10_000,
        percent in 25u16..=200,
    ) {
        let profile = if musical { Profile::Musical } else { Profile::Gp };
        let performance = performance(&fixture(SEEK_FIXTURES[fixture_index]), profile);
        let bar = bar_pick % performance.order.bars.len();
        let frame = 1 + frame_pick * song_frames(&performance, percent).saturating_sub(2) / 10_000;
        let script = [step(frame, TransportCommand::Seek(bar))];
        let logs = record(&performance, session_config(percent), &script, frame + 1);
        assert_restored(&performance, &logs, bar, frame, percent)?;
    }

    #[test]
    fn a_loop_wraps_with_every_note_closed_and_the_loop_start_restored(
        fixture_index in 0..LOOP_FIXTURES.len(),
        musical in any::<bool>(),
        first_pick in 0usize..10_000,
        length in 1usize..=MAX_LOOP_BARS,
        percent in 25u16..=200,
    ) {
        let profile = if musical { Profile::Musical } else { Profile::Gp };
        let performance = performance(&fixture(LOOP_FIXTURES[fixture_index]), profile);
        let bars = performance.order.bars.len();
        let first = first_pick % bars;
        let last = (first + length - 1).min(bars - 1);
        let range = BarRange { first, last };
        let start = performance.order.bars[first].start;
        let end = performance
            .order
            .bars
            .get(last + 1)
            .map_or(performance.order.end(), |bar| bar.start);
        let period = (unit_of(&performance.tempo, end, RATE)
            - unit_of(&performance.tempo, start, RATE))
            / u64::from(percent);
        prop_assert!(period > 0);
        prop_assert_eq!(range_frames(&performance, range, speed(percent), RATE), Ok(period));
        let wraps: Vec<u64> = (1..=WRAPS).map(|count| count * period).collect();
        let stop = WRAPS * period + period / 2;
        let script = [
            step(0, TransportCommand::Loop(range)),
            step(stop, TransportCommand::Stop),
        ];
        let logs = record(&performance, session_config(percent), &script, stop + 1);
        for log in &logs {
            let notes = notes(log);
            for note in &notes {
                let off = note.off.expect("every note is closed after the stop");
                for wrap in &wraps {
                    prop_assert!(
                        !(note.on < *wrap && off > *wrap),
                        "{:?} sounds across the wrap at {}", note, wrap
                    );
                }
            }
        }
        for wrap in &wraps {
            assert_restored(&performance, &logs, first, *wrap, percent)?;
        }
        assert_restored(&performance, &logs, first, 0, percent)?;
    }
}

#[test]
fn a_seek_during_a_sounding_bend_restores_the_bend_range_and_centre() {
    let performance = performance(&fixture("note-effects.gp"), Profile::Musical);
    let bent = performance
        .events
        .iter()
        .find(|event| matches!(event.message, MidiMessage::PitchBend { value } if value > BEND_CENTRE + 1000))
        .expect("a bent note");
    let frame = frame_at(&performance.tempo, bent.at, Speed::NORMAL, RATE).expect("frame") + 1;
    let script = [step(frame, TransportCommand::Seek(0))];
    let logs = record(&performance, session_config(100), &script, frame + 1);
    let channel = usize::from(bent.channel.get());
    let before = SynthModel::of_log(&logs[bent.track], frame - 1);
    assert!(before.states[channel].bend > BEND_CENTRE);
    let after = SynthModel::of_log(&logs[bent.track], frame);
    assert_eq!(after.states[channel].bend, BEND_CENTRE);
    let expected = reference(
        &performance,
        bent.track,
        ScoreTime::ZERO,
        u64::from(Speed::NORMAL.percent()),
    );
    assert_eq!(after.states, expected.states);
    assert!(after.sounding.is_empty() || after.sounding == expected.sounding);
}

struct Silent;

impl Synth for Silent {
    fn reset(&mut self) {}
    fn handle(&mut self, _channel: Channel, _message: &MidiMessage) {}
    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        right.fill(0.0);
    }
}

struct Gate {
    inner: Box<dyn Synth>,
    rendered: u64,
    from: u64,
}

impl Synth for Gate {
    fn reset(&mut self) {
        self.inner.reset();
        self.rendered = 0;
    }
    fn handle(&mut self, channel: Channel, message: &MidiMessage) {
        self.inner.handle(channel, message);
    }
    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.inner.render(left, right);
        for (index, (left, right)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
            if self.rendered + index as u64 >= self.from {
                *left = 0.0;
                *right = 0.0;
            }
        }
        self.rendered += left.len() as u64;
    }
}

const MIX_FIXTURE: &str = "multi-track.gp";
const MIX_FRAMES: u64 = 96_000;
const ODD_FRAME: u64 = 12_345;

fn mixed(
    performance: &Performance,
    voices: Vec<Box<dyn Synth>>,
    script: &[ScriptStep],
) -> RenderedAudio {
    let mut session = Session::new(performance.clone(), voices, config(RATE)).expect("session");
    render_session(&mut session, script, MIX_FRAMES).expect("render")
}

fn replaced(performance: &Performance, silent: &[usize]) -> Vec<Box<dyn Synth>> {
    built_in_voices(performance, RATE)
        .into_iter()
        .enumerate()
        .map(|(track, voice)| {
            if silent.contains(&track) {
                Box::new(Silent) as Box<dyn Synth>
            } else {
                voice
            }
        })
        .collect()
}

fn loud(audio: &RenderedAudio) -> bool {
    audio.left.iter().any(|sample| *sample != 0.0)
}

#[test]
fn mute_and_solo_silence_exactly_the_chosen_tracks() {
    let performance = performance(&fixture(MIX_FIXTURE), Profile::Musical);
    let tracks = performance.tracks.len();
    assert!(tracks >= 3, "{tracks} tracks");
    let all: Vec<usize> = (0..tracks).collect();
    let others = |kept: &[usize]| -> Vec<usize> {
        all.iter()
            .copied()
            .filter(|track| !kept.contains(track))
            .collect()
    };
    let full = mixed(&performance, built_in_voices(&performance, RATE), &[]);
    let muted = mixed(
        &performance,
        built_in_voices(&performance, RATE),
        &[step(0, TransportCommand::Mute(1, true))],
    );
    assert_eq!(
        muted,
        mixed(&performance, replaced(&performance, &[1]), &[])
    );
    assert_ne!(muted, full);
    let soloed = mixed(
        &performance,
        built_in_voices(&performance, RATE),
        &[
            step(0, TransportCommand::Solo(0, true)),
            step(0, TransportCommand::Solo(2, true)),
        ],
    );
    assert_eq!(
        soloed,
        mixed(&performance, replaced(&performance, &others(&[0, 2])), &[])
    );
    assert!(loud(&soloed));
    let both = mixed(
        &performance,
        built_in_voices(&performance, RATE),
        &[
            step(0, TransportCommand::Solo(0, true)),
            step(0, TransportCommand::Solo(2, true)),
            step(0, TransportCommand::Mute(2, true)),
        ],
    );
    assert_eq!(
        both,
        mixed(&performance, replaced(&performance, &others(&[0])), &[])
    );
    let cleared = mixed(
        &performance,
        built_in_voices(&performance, RATE),
        &[
            step(0, TransportCommand::Solo(0, true)),
            step(0, TransportCommand::Solo(0, false)),
            step(0, TransportCommand::Mute(1, true)),
            step(0, TransportCommand::Mute(1, false)),
            step(0, TransportCommand::Mute(tracks + 5, true)),
        ],
    );
    assert_eq!(cleared, full);
}

#[test]
fn a_command_takes_effect_at_the_first_frame_of_the_next_block() {
    let performance = performance(&fixture(MIX_FIXTURE), Profile::Musical);
    let mut gated = built_in_voices(&performance, RATE);
    let inner = gated.remove(1);
    gated.insert(
        1,
        Box::new(Gate {
            inner,
            rendered: 0,
            from: ODD_FRAME,
        }),
    );
    let muted = mixed(
        &performance,
        built_in_voices(&performance, RATE),
        &[step(ODD_FRAME, TransportCommand::Mute(1, true))],
    );
    let expected = mixed(&performance, gated, &[]);
    assert_eq!(muted, expected);
    let full = mixed(&performance, built_in_voices(&performance, RATE), &[]);
    let cut = usize::try_from(ODD_FRAME).expect("frame");
    assert_eq!(muted.left[..cut], full.left[..cut]);
    assert_ne!(muted.left[cut..], full.left[cut..]);
}

#[test]
fn a_speed_change_moves_every_later_event_from_the_next_block_on() {
    let performance = performance(&fixture("durations.gp"), Profile::Musical);
    for (before, after) in [(100u16, 50u16), (100, 200), (75, 131)] {
        let change = ODD_FRAME;
        let script = [step(change, TransportCommand::Speed(speed(after)))];
        let frames = 4 * song_frames(&performance, after.min(before)) + change;
        let logs = record(&performance, session_config(before), &script, frames);
        let reached = change * u64::from(before);
        let mut expected: Vec<Vec<Heard>> = performance.tracks.iter().map(|_| Vec::new()).collect();
        for event in &performance.events {
            let unit = unit_of(&performance.tempo, event.at, RATE);
            let frame = if unit < reached {
                unit / u64::from(before)
            } else {
                change + (unit - reached) / u64::from(after)
            };
            expected[event.track].push(Heard {
                frame,
                channel: event.channel,
                message: event.message,
            });
        }
        assert!(expected.iter().flatten().any(|heard| heard.frame > change));
        assert!(expected.iter().flatten().any(|heard| heard.frame < change));
        assert_eq!(logs, expected, "{before} -> {after}");
    }
}

fn clicks_of(log: &[Heard]) -> Vec<(u64, u8, u8)> {
    log.iter()
        .filter_map(|heard| match heard.message {
            MidiMessage::NoteOn { key, velocity } => {
                assert_eq!(heard.channel, Channel::DRUMS);
                Some((heard.frame, key, velocity))
            }
            _ => None,
        })
        .collect()
}

fn click_note(downbeat: bool) -> (u8, u8) {
    if downbeat {
        (CLICK_DOWNBEAT_KEY, CLICK_DOWNBEAT_VELOCITY)
    } else {
        (CLICK_BEAT_KEY, CLICK_BEAT_VELOCITY)
    }
}

fn record_with_click(
    performance: &Performance,
    config: SessionConfig,
    frames: u64,
) -> (Vec<Vec<Heard>>, Vec<Heard>) {
    let recorders = recorders(performance);
    let (click_log, click) = recording();
    let mut session =
        Session::with_click(performance.clone(), recorders.voices, click, config).expect("session");
    render_session(&mut session, &[], frames).expect("render");
    (
        recorders.logs.iter().map(heard).collect(),
        heard(&click_log),
    )
}

#[test]
fn the_count_in_plays_one_bar_of_accented_clicks_before_the_song() {
    for (name, percent) in [
        ("signatures.gp", 100u16),
        ("durations.gp", 63),
        ("tempo.gp", 180),
    ] {
        let performance = performance(&fixture(name), Profile::Musical);
        let tempo = &performance.tempo;
        let bar = &performance.order.bars[0];
        let length = unit_of(tempo, ScoreTime::new(bar.length).expect("time"), RATE);
        let count_in = length / u64::from(percent);
        let frames = count_in + song_frames(&performance, percent) + 10;
        let config = SessionConfig {
            count_in: true,
            ..session_config(percent)
        };
        let (logs, clicks) = record_with_click(&performance, config, frames);
        let expected_clicks: Vec<(u64, u8, u8)> = performance
            .clicks
            .iter()
            .filter(|click| click.at < ScoreTime::new(bar.length).expect("time"))
            .map(|click| {
                let (key, velocity) = click_note(click.downbeat);
                (
                    unit_of(tempo, click.at, RATE) / u64::from(percent),
                    key,
                    velocity,
                )
            })
            .collect();
        assert!(expected_clicks.len() >= 2, "{name}");
        assert_eq!(expected_clicks[0].1, CLICK_DOWNBEAT_KEY);
        assert_eq!(clicks_of(&clicks), expected_clicks, "{name}");
        assert_eq!(notes(&clicks).len(), expected_clicks.len());
        for (track, log) in logs.iter().enumerate() {
            let expected: Vec<Heard> = performance
                .events
                .iter()
                .filter(|event| event.track == track)
                .map(|event| Heard {
                    frame: count_in + unit_of(tempo, event.at, RATE) / u64::from(percent),
                    channel: event.channel,
                    message: event.message,
                })
                .collect();
            assert_eq!(*log, expected, "{name} track {track}");
        }
    }
}

#[test]
fn the_metronome_clicks_every_beat_with_an_accented_downbeat() {
    let performance = performance(&fixture("signatures.gp"), Profile::Musical);
    let percent = 90u16;
    let frames = song_frames(&performance, percent) + 10;
    let config = SessionConfig {
        metronome: true,
        ..session_config(percent)
    };
    let (_, clicks) = record_with_click(&performance, config, frames);
    let expected: Vec<(u64, u8, u8)> = performance
        .clicks
        .iter()
        .map(|click| {
            let (key, velocity) = click_note(click.downbeat);
            (
                unit_of(&performance.tempo, click.at, RATE) / u64::from(percent),
                key,
                velocity,
            )
        })
        .collect();
    assert!(expected.iter().any(|click| click.1 == CLICK_BEAT_KEY));
    assert_eq!(clicks_of(&clicks), expected);
    let (_, silent) = record_with_click(&performance, session_config(percent), frames);
    assert!(clicks_of(&silent).is_empty());
}

fn played(sources: &[usize]) -> PlayOrder {
    PlayOrder {
        bars: sources
            .iter()
            .enumerate()
            .map(|(index, source)| PlayedBar {
                source: *source,
                start: ScoreTime::new(Fraction::from_integer(index as i64)).expect("time"),
                length: Fraction::from_integer(1),
                pass: 1,
            })
            .collect(),
        truncated: false,
    }
}

fn range(first: usize, last: usize) -> BarRange {
    BarRange { first, last }
}

#[test]
fn a_source_bar_range_maps_to_its_first_contiguous_run_of_played_bars() {
    let order = played(&[0, 1, 2, 1, 2, 3]);
    let jumped = played(&[0, 1, 2, 3, 1, 2, 4]);
    assert_eq!(played_range(&jumped, range(1, 2)), Some(range(1, 2)));
    assert_eq!(played_range(&jumped, range(4, 4)), Some(range(6, 6)));
    assert_eq!(played_range(&order, range(1, 2)), Some(range(1, 4)));
    assert_eq!(played_range(&order, range(2, 3)), Some(range(2, 2)));
    assert_eq!(played_range(&order, range(3, 3)), Some(range(5, 5)));
    assert_eq!(played_range(&order, range(0, 3)), Some(range(0, 5)));
    assert_eq!(played_range(&order, range(1, 1)), Some(range(1, 1)));
    assert_eq!(played_range(&order, range(4, 9)), None);
    assert_eq!(played_range(&order, range(2, 1)), None);
    let repeats = performance(&fixture("repeats.gp"), Profile::Musical);
    let sources = repeats.order.sources();
    let runs = |source: usize| -> Vec<usize> {
        (0..sources.len())
            .filter(|played| {
                sources[*played] == source && (*played == 0 || sources[*played - 1] != source)
            })
            .collect()
    };
    let repeated = (0..sources.len())
        .map(|played| sources[played])
        .find(|source| runs(*source).len() >= 2)
        .expect("repeats.gp plays a bar again after other bars");
    let starts = runs(repeated);
    let run = played_range(&repeats.order, range(repeated, repeated)).expect("played");
    assert_eq!(run.first, starts[0]);
    assert!(run.last < starts[1]);
    assert!((run.first..=run.last).all(|played| sources[played] == repeated));
    assert_ne!(sources.get(run.last + 1), Some(&repeated));
}

#[test]
fn a_session_renders_identically_in_any_block_split() {
    let performance = performance(&fixture("note-effects.gp"), Profile::Musical);
    let script = [
        step(1_000, TransportCommand::Loop(range(2, 3))),
        step(70_000, TransportCommand::Speed(speed(150))),
        step(90_001, TransportCommand::Metronome(true)),
    ];
    let whole = mixed(&performance, built_in_voices(&performance, RATE), &script);
    let mut split: Vec<ScriptStep> = script.to_vec();
    for frame in (0..MIX_FRAMES).step_by(777) {
        split.push(step(frame, TransportCommand::CountIn(false)));
    }
    let pieces = mixed(&performance, built_in_voices(&performance, RATE), &split);
    assert_eq!(whole, pieces);
    for profile in PROFILES {
        let performance = crate::support::performance(&fixture("repeats.gp"), profile);
        assert!(loud(&mixed(
            &performance,
            built_in_voices(&performance, RATE),
            &script
        )));
    }
}
