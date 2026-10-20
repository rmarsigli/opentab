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
use crate::support::{PROFILES, built_in_voices, config, fixture, fixture_names, performance};

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
        paused in any::<bool>(),
    ) {
        let profile = if musical { Profile::Musical } else { Profile::Gp };
        let performance = performance(&fixture(SEEK_FIXTURES[fixture_index]), profile);
        let bar = bar_pick % performance.order.bars.len();
        let frame = 1 + frame_pick * song_frames(&performance, percent).saturating_sub(2) / 10_000;
        let resumed = frame + 777;
        let script = if paused {
            vec![
                step(frame, TransportCommand::Pause),
                step(frame, TransportCommand::Seek(bar)),
                step(resumed, TransportCommand::Play),
            ]
        } else {
            vec![step(frame, TransportCommand::Seek(bar))]
        };
        let checked = if paused { resumed } else { frame };
        let logs = record(&performance, session_config(percent), &script, checked + 1);
        assert_restored(&performance, &logs, bar, checked, percent)?;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ClickNote {
    key: u8,
    velocity: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HeardClick {
    frame: u64,
    note: ClickNote,
}

struct ClickRecording {
    logs: Vec<Vec<Heard>>,
    clicks: Vec<Heard>,
}

fn clicks_of(log: &[Heard]) -> Vec<HeardClick> {
    log.iter()
        .filter_map(|heard| match heard.message {
            MidiMessage::NoteOn { key, velocity } => {
                assert_eq!(heard.channel, Channel::DRUMS);
                Some(HeardClick {
                    frame: heard.frame,
                    note: ClickNote { key, velocity },
                })
            }
            _ => None,
        })
        .collect()
}

fn click_note(downbeat: bool) -> ClickNote {
    if downbeat {
        ClickNote {
            key: CLICK_DOWNBEAT_KEY,
            velocity: CLICK_DOWNBEAT_VELOCITY,
        }
    } else {
        ClickNote {
            key: CLICK_BEAT_KEY,
            velocity: CLICK_BEAT_VELOCITY,
        }
    }
}

fn record_script_with_click(
    performance: &Performance,
    config: SessionConfig,
    script: &[ScriptStep],
    frames: u64,
) -> ClickRecording {
    let recorders = recorders(performance);
    let click = recording();
    let mut session =
        Session::with_click(performance.clone(), recorders.voices, click.synth, config)
            .expect("session");
    render_session(&mut session, script, frames).expect("render");
    ClickRecording {
        logs: recorders.logs.iter().map(heard).collect(),
        clicks: heard(&click.log),
    }
}

fn record_with_click(
    performance: &Performance,
    config: SessionConfig,
    frames: u64,
) -> ClickRecording {
    record_script_with_click(performance, config, &[], frames)
}

struct CountInExpectation {
    lead: u64,
    clicks: Vec<HeardClick>,
    tracks: Vec<Vec<Heard>>,
}

fn count_in_expectation(
    performance: &Performance,
    played: usize,
    percent: u16,
) -> CountInExpectation {
    count_in_heard_by(
        performance,
        played,
        percent,
        &vec![true; performance.tracks.len()],
    )
}

fn count_in_heard_by(
    performance: &Performance,
    played: usize,
    percent: u16,
    audible: &[bool],
) -> CountInExpectation {
    let tempo = &performance.tempo;
    let bar = &performance.order.bars[played];
    let bar_end = ScoreTime::new(bar.start.whole_notes() + bar.length).expect("time");
    let origin = unit_of(tempo, bar.start, RATE);
    let length = unit_of(tempo, bar_end, RATE) - origin;
    let entry = performance
        .events
        .iter()
        .filter(
            |event| matches!(event.message, MidiMessage::NoteOn { velocity, .. } if velocity > 0),
        )
        .filter(|event| audible[event.track])
        .map(|event| unit_of(tempo, event.at, RATE))
        .find(|unit| (origin..origin + length).contains(unit))
        .unwrap_or(origin);
    let step = u64::from(percent);
    let mut clicks: Vec<HeardClick> = performance
        .clicks
        .iter()
        .filter(|click| click.at >= bar.start && click.at < bar_end)
        .map(|click| {
            let unit = unit_of(tempo, click.at, RATE);
            let elapsed = if unit >= entry {
                unit - entry
            } else {
                unit + length - entry
            };
            HeardClick {
                frame: elapsed / step,
                note: click_note(click.downbeat),
            }
        })
        .collect();
    clicks.sort_by_key(|click| click.frame);
    let count_in = length / step;
    let tracks = (0..performance.tracks.len())
        .map(|track| {
            performance
                .events
                .iter()
                .filter(|event| event.track == track)
                .map(|event| Heard {
                    frame: count_in + unit_of(tempo, event.at, RATE).saturating_sub(entry) / step,
                    channel: event.channel,
                    message: event.message,
                })
                .collect()
        })
        .collect();
    CountInExpectation {
        lead: entry - origin,
        clicks,
        tracks,
    }
}

fn count_in_config(percent: u16) -> SessionConfig {
    SessionConfig {
        count_in: true,
        ..session_config(percent)
    }
}

#[test]
fn the_count_in_plays_one_bar_of_accented_clicks_before_the_song() {
    for (name, percent) in [
        ("signatures.gp", 100u16),
        ("durations.gp", 63),
        ("tempo.gp", 180),
    ] {
        let performance = performance(&fixture(name), Profile::Musical);
        let expected = count_in_expectation(&performance, 0, percent);
        let frames = expected
            .tracks
            .iter()
            .flatten()
            .map(|heard| heard.frame)
            .max()
            .unwrap_or(0)
            + song_frames(&performance, percent)
            + 10;
        let ClickRecording { logs, clicks } =
            record_with_click(&performance, count_in_config(percent), frames);
        assert!(expected.clicks.len() >= 2, "{name}");
        if expected.lead == 0 {
            assert_eq!(expected.clicks[0].note.key, CLICK_DOWNBEAT_KEY, "{name}");
        }
        assert_eq!(clicks_of(&clicks), expected.clicks, "{name}");
        assert_eq!(notes(&clicks).len(), expected.clicks.len());
        assert_eq!(logs, expected.tracks, "{name}");
    }
}

pub fn with_pickup(name: &str) -> Performance {
    let mut score = fixture(name);
    score.anacrusis = true;
    for track in &mut score.tracks {
        for staff in &mut track.staves {
            if let Some(bar) = staff.bars.first_mut() {
                for voice in &mut bar.voices {
                    let keep = voice.beats.len().saturating_sub(1);
                    voice.beats.drain(..keep);
                }
            }
        }
    }
    performance(&score, Profile::Musical)
}

#[test]
fn a_count_in_into_a_pickup_bar_fills_the_bar_before_its_first_note() {
    for (name, percent) in [("note-effects.gp", 100u16), ("mixer.gp", 150)] {
        let performance = with_pickup(name);
        let expected = count_in_expectation(&performance, 0, percent);
        assert!(expected.lead > 0, "{name} has a pickup");
        let beats = performance
            .clicks
            .iter()
            .filter(|click| click.at < performance.order.bars[1].start)
            .count();
        assert_eq!(expected.clicks.len(), beats);
        let frames = expected
            .tracks
            .iter()
            .flatten()
            .map(|heard| heard.frame)
            .max()
            .unwrap_or(0)
            + 10;
        let ClickRecording { logs, clicks } =
            record_with_click(&performance, count_in_config(percent), frames);
        assert_eq!(clicks_of(&clicks), expected.clicks, "{name}");
        assert_eq!(logs, expected.tracks, "{name}");
        let first_note = logs
            .iter()
            .flatten()
            .filter(|heard| matches!(heard.message, MidiMessage::NoteOn { velocity, .. } if velocity > 0))
            .map(|heard| heard.frame)
            .min()
            .expect("a note");
        let bar = &performance.order.bars[0];
        let length = unit_of(
            &performance.tempo,
            ScoreTime::new(bar.start.whole_notes() + bar.length).expect("time"),
            RATE,
        );
        assert_eq!(first_note, length / u64::from(percent), "{name}");
        let last_click = expected.clicks.last().expect("a click").frame;
        assert!(last_click < first_note);
        let gaps: Vec<u64> = expected
            .clicks
            .windows(2)
            .map(|pair| pair[1].frame - pair[0].frame)
            .collect();
        let beat = gaps.first().copied().expect("two clicks");
        assert!(
            gaps.iter().all(|gap| gap.abs_diff(beat) <= 1),
            "{name} {gaps:?}"
        );
        assert!((first_note - last_click).abs_diff(beat) <= 1, "{name}");
    }
}

#[test]
fn a_seek_during_the_count_in_counts_the_new_bar_in() {
    let performance = performance(&fixture("signatures.gp"), Profile::Musical);
    let percent = 100u16;
    let bars = &performance.order.bars;
    let target = (1..bars.len())
        .find(|index| bars[*index].length != bars[0].length)
        .expect("a bar of another length");
    let seek = 1_000;
    let script = [step(seek, TransportCommand::Seek(target))];
    let before = count_in_expectation(&performance, 0, percent);
    let after = count_in_expectation(&performance, target, percent);
    let tempo = &performance.tempo;
    let bar_end =
        ScoreTime::new(bars[target].start.whole_notes() + bars[target].length).expect("time");
    let length = unit_of(tempo, bar_end, RATE) - unit_of(tempo, bars[target].start, RATE);
    let frames = seek + length / u64::from(percent) + 10;
    let ClickRecording { logs, clicks } =
        record_script_with_click(&performance, count_in_config(percent), &script, frames);
    let expected: Vec<HeardClick> = before
        .clicks
        .iter()
        .filter(|click| click.frame < seek)
        .copied()
        .chain(after.clicks.iter().map(|click| HeardClick {
            frame: seek + click.frame,
            note: click.note,
        }))
        .collect();
    assert!(after.clicks.len() >= 2);
    assert_eq!(clicks_of(&clicks), expected);
    let first_note = logs
        .iter()
        .flatten()
        .filter(
            |heard| matches!(heard.message, MidiMessage::NoteOn { velocity, .. } if velocity > 0),
        )
        .map(|heard| heard.frame)
        .min()
        .expect("a note");
    assert_eq!(first_note, seek + length / u64::from(percent));
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
    let clicks = record_with_click(&performance, config, frames).clicks;
    let expected: Vec<HeardClick> = performance
        .clicks
        .iter()
        .map(|click| HeardClick {
            frame: unit_of(&performance.tempo, click.at, RATE) / u64::from(percent),
            note: click_note(click.downbeat),
        })
        .collect();
    assert!(
        expected
            .iter()
            .any(|click| click.note.key == CLICK_BEAT_KEY)
    );
    assert_eq!(clicks_of(&clicks), expected);
    let silent = record_with_click(&performance, session_config(percent), frames).clicks;
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
    assert_eq!(
        played_range(&order, range(2, usize::MAX)),
        Some(range(2, 2))
    );
    assert_eq!(played_range(&order, range(usize::MAX, usize::MAX)), None);
    for first in 0..5 {
        for last in first..5 {
            let expected = (first..=last)
                .filter_map(|source| order.played_of(source).first().copied())
                .min();
            assert_eq!(
                played_range(&order, range(first, last)).map(|run| run.first),
                expected
            );
        }
    }
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

#[test]
fn an_sfz_session_decodes_every_sample_it_plays_before_the_first_block() {
    for name in ["multi-track.gp", "drums.gp", "sound-change.gp"] {
        let bank = crate::support::tiny_sfz_bank();
        let performance = performance(&fixture(name), Profile::Musical);
        let voices = crate::support::bank_voices(&bank, &performance, RATE);
        assert_eq!(bank.decode_count(), 0);
        let mut session = Session::new(performance.clone(), voices, config(RATE)).expect("session");
        let prepared = bank.decode_count();
        assert!(prepared > 0, "{name}");
        let frames = song_frames(&performance, 100) + 48_000;
        let audio = render_session(&mut session, &[], frames).expect("render");
        assert!(loud(&audio), "{name}");
        assert_eq!(
            bank.decode_count(),
            prepared,
            "{name} decoded while playing"
        );
        assert!(bank.faults().is_empty(), "{:?}", bank.faults());
    }
}

#[test]
fn the_render_report_names_the_played_bars_length_and_backends() {
    let performance = performance(&fixture("repeats.gp"), Profile::Musical);
    let mut session = Session::new(
        performance.clone(),
        built_in_voices(&performance, RATE),
        config(RATE),
    )
    .expect("session");
    let script = [step(0, TransportCommand::Loop(range(1, 2)))];
    let frames = 4 * range_frames(&performance, range(1, 2), Speed::NORMAL, RATE).expect("frames");
    render_session(&mut session, &script, frames).expect("render");
    let backends: Vec<opentab_audio::TrackBackend> = (0..performance.tracks.len())
        .map(opentab_audio::TrackBackend::built_in)
        .collect();
    let report = session.report(backends.clone());
    assert_eq!(report.played_bars, vec![1, 2]);
    assert_eq!(
        report.seconds,
        Fraction::new(i64::try_from(frames).expect("frames"), i64::from(RATE.hz()))
    );
    assert_eq!(report.backends, backends);
    assert_eq!(report.backends[0].bank, opentab_audio::BUILT_IN_BANK_NAME);
    assert_eq!(report.unplayed, performance.unplayed);
    assert_eq!(report.warnings, performance.warnings);
    let mut whole = Session::new(
        performance.clone(),
        built_in_voices(&performance, RATE),
        config(RATE),
    )
    .expect("session");
    render_session(&mut whole, &[], song_frames(&performance, 100) + 10).expect("render");
    let all: Vec<usize> = (0..performance.order.bars.len()).collect();
    assert_eq!(whole.report(Vec::new()).played_bars, all);
}

#[test]
fn stop_with_a_loop_set_returns_to_the_loop_start() {
    let performance = performance(&fixture("repeats.gp"), Profile::Musical);
    let mut session = Session::new(
        performance.clone(),
        built_in_voices(&performance, RATE),
        config(RATE),
    )
    .expect("session");
    let mut left = vec![0.0f32; 1_000];
    let mut right = vec![0.0f32; 1_000];
    session
        .handle
        .send(TransportCommand::Loop(range(2, 3)))
        .expect("send");
    session.handle.send(TransportCommand::Play).expect("send");
    session.transport.process(&mut left, &mut right);
    assert_eq!(session.handle.status().played_bar, 2);
    session.handle.send(TransportCommand::Stop).expect("send");
    session.transport.process(&mut left[..1], &mut right[..1]);
    let stopped = session.handle.status();
    assert_eq!(stopped.state, opentab_audio::TransportState::Stopped);
    assert_eq!(
        (stopped.played_bar, stopped.offset),
        (2, Fraction::new_raw(0, 1))
    );
    session
        .handle
        .send(TransportCommand::ClearLoop)
        .expect("send");
    session.handle.send(TransportCommand::Stop).expect("send");
    session.transport.process(&mut left[..1], &mut right[..1]);
    assert_eq!(session.handle.status().played_bar, 0);
}

type SharedHandle = std::sync::Arc<std::sync::Mutex<Option<opentab_audio::TransportHandle>>>;

struct Refill {
    handle: SharedHandle,
    armed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Synth for Refill {
    fn reset(&mut self) {}
    fn handle(&mut self, _channel: Channel, message: &MidiMessage) {
        if matches!(message, MidiMessage::NoteOff { .. })
            && self.armed.swap(false, std::sync::atomic::Ordering::SeqCst)
            && let Some(handle) = self.handle.lock().expect("handle").as_mut()
        {
            handle.send(TransportCommand::Pause).expect("room");
        }
    }
    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        right.fill(0.0);
    }
}

#[test]
fn a_block_applies_only_the_commands_waiting_when_it_starts() {
    let performance = performance(&fixture("note-effects.gp"), Profile::Musical);
    let shared: SharedHandle = std::sync::Arc::new(std::sync::Mutex::new(None));
    let armed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let voices: Vec<Box<dyn Synth>> = performance
        .tracks
        .iter()
        .map(|_| {
            Box::new(Refill {
                handle: std::sync::Arc::clone(&shared),
                armed: std::sync::Arc::clone(&armed),
            }) as Box<dyn Synth>
        })
        .collect();
    let session = Session::new(performance.clone(), voices, config(RATE)).expect("session");
    let (mut transport, handle) = session.split();
    *shared.lock().expect("handle") = Some(handle);
    let send = |command: TransportCommand| {
        shared
            .lock()
            .expect("handle")
            .as_mut()
            .expect("handle")
            .send(command)
            .expect("room");
    };
    let state = || {
        shared
            .lock()
            .expect("handle")
            .as_ref()
            .expect("handle")
            .status()
            .state
    };
    let tempo = &performance.tempo;
    let on = performance
        .events
        .iter()
        .find(|event| matches!(event.message, MidiMessage::NoteOn { velocity, .. } if velocity > 0))
        .expect("a note");
    let off = performance
        .events
        .iter()
        .find(|event| {
            event.at > on.at
                && event.track == on.track
                && matches!(event.message, MidiMessage::NoteOff { key, .. } | MidiMessage::NoteOn { key, velocity: 0 } if matches!(on.message, MidiMessage::NoteOn { key: struck, .. } if struck == key))
        })
        .expect("its note-off");
    let middle = (frame_at(tempo, on.at, Speed::NORMAL, RATE).expect("frame")
        + frame_at(tempo, off.at, Speed::NORMAL, RATE).expect("frame"))
        / 2;
    let mut left = vec![0.0f32; 1];
    let mut right = vec![0.0f32; 1];
    send(TransportCommand::Play);
    for _ in 0..middle {
        transport.process(&mut left, &mut right);
    }
    armed.store(true, std::sync::atomic::Ordering::SeqCst);
    send(TransportCommand::Seek(0));
    transport.process(&mut left, &mut right);
    assert!(
        !armed.load(std::sync::atomic::Ordering::SeqCst),
        "the seek silenced a sounding note"
    );
    assert_eq!(state(), opentab_audio::TransportState::Playing);
    transport.process(&mut left, &mut right);
    assert_eq!(state(), opentab_audio::TransportState::Paused);
}

#[test]
fn several_seeks_in_one_block_chase_once() {
    let performance = performance(&fixture("note-effects.gp"), Profile::Musical);
    let frame = song_frames(&performance, 100) / 2;
    let single = [step(frame, TransportCommand::Seek(2))];
    let double = [
        step(frame, TransportCommand::Seek(1)),
        step(frame, TransportCommand::Seek(2)),
    ];
    let once = record(&performance, session_config(100), &single, frame + 1);
    let twice = record(&performance, session_config(100), &double, frame + 1);
    let chased: usize = once
        .iter()
        .flatten()
        .filter(|heard| {
            heard.frame == frame
                && !matches!(
                    heard.message,
                    MidiMessage::NoteOn { .. } | MidiMessage::NoteOff { .. }
                )
        })
        .count();
    assert!(chased > 0);
    assert_eq!(twice, once);
}

#[test]
fn the_built_in_click_is_heard_when_every_track_is_muted() {
    let performance = performance(&fixture("multi-track.gp"), Profile::Musical);
    let mutes: Vec<ScriptStep> = (0..performance.tracks.len())
        .map(|track| step(0, TransportCommand::Mute(track, true)))
        .collect();
    let render = |metronome: bool| {
        let config = SessionConfig {
            metronome,
            ..config(RATE)
        };
        let mut session = Session::new(
            performance.clone(),
            built_in_voices(&performance, RATE),
            config,
        )
        .expect("session");
        render_session(&mut session, &mutes, MIX_FRAMES).expect("render")
    };
    assert!(loud(&render(true)));
    assert!(!loud(&render(false)));
}

#[derive(Debug, Clone, Copy)]
struct BendAtSeam {
    bar: usize,
    track: usize,
    channel: Channel,
}

#[test]
fn a_bend_sounding_at_the_loop_end_is_reset_at_the_wrap() {
    let mut checked = 0;
    for name in ["note-effects.gp", "tremolo-bar.gp"] {
        let performance = performance(&fixture(name), Profile::Musical);
        let tempo = &performance.tempo;
        let bars = &performance.order.bars;
        let Some(found) = (0..bars.len()).find_map(|bar| {
            let start = bars[bar].start;
            let end = bars
                .get(bar + 1)
                .map_or(performance.order.end(), |next| next.start);
            let last = performance
                .events
                .iter()
                .filter(|event| event.at >= start && event.at < end)
                .rfind(|event| matches!(event.message, MidiMessage::PitchBend { .. }))?;
            let reset_at_start = performance.events.iter().any(|event| {
                event.at == start
                    && event.track == last.track
                    && event.channel == last.channel
                    && matches!(event.message, MidiMessage::PitchBend { .. })
            });
            (!reset_at_start
                && matches!(last.message, MidiMessage::PitchBend { value } if value != BEND_CENTRE))
            .then_some(BendAtSeam {
                bar,
                track: last.track,
                channel: last.channel,
            })
        }) else {
            continue;
        };
        let BendAtSeam {
            bar,
            track,
            channel,
        } = found;
        let start = bars[bar].start;
        let end = bars
            .get(bar + 1)
            .map_or(performance.order.end(), |next| next.start);
        let period = (unit_of(tempo, end, RATE) - unit_of(tempo, start, RATE)) / 100;
        let script = [step(0, TransportCommand::Loop(range(bar, bar)))];
        let logs = record(&performance, session_config(100), &script, period + 1);
        let index = usize::from(channel.get());
        let before = SynthModel::of_log(&logs[track], period - 1);
        let start_state = reference(
            &performance,
            track,
            start,
            unit_of(tempo, start, RATE) + 100,
        );
        assert_ne!(
            before.states[index].bend, start_state.states[index].bend,
            "{name}"
        );
        assert_restored(&performance, &logs, bar, period, 100).expect("restored");
        checked += 1;
    }
    assert!(checked > 0);
}

fn events_before_bar(performance: &Performance, bar: usize) -> u64 {
    let start = performance.order.bars[bar].start;
    performance
        .events
        .iter()
        .filter(|event| event.at <= start)
        .count() as u64
}

#[test]
fn a_seek_or_a_loop_wrap_to_a_bar_start_chases_no_events() {
    let mut scan_would_exceed = false;
    for name in SEEK_FIXTURES {
        let performance = performance(&fixture(name), Profile::Musical);
        let bars = performance.order.bars.len();
        let mut session = Session::new(
            performance.clone(),
            built_in_voices(&performance, RATE),
            config(RATE),
        )
        .expect("session");
        let mut left = vec![0.0f32; 512];
        let mut right = vec![0.0f32; 512];
        session.handle.send(TransportCommand::Play).expect("send");
        session.transport.process(&mut left, &mut right);
        for bar in (0..bars).rev().chain(0..bars) {
            let before = session.transport.chase_steps();
            session
                .handle
                .send(TransportCommand::Seek(bar))
                .expect("send");
            session.transport.process(&mut left, &mut right);
            assert_eq!(session.handle.status().played_bar, bar, "{name}");
            let steps = session.transport.chase_steps() - before;
            assert_eq!(steps, 0, "{name} bar {bar}: {steps} steps");
            scan_would_exceed |= events_before_bar(&performance, bar) > 0;
        }
        if bars < 2 {
            continue;
        }
        let first = bars - 2;
        session
            .handle
            .send(TransportCommand::Loop(range(first, bars - 1)))
            .expect("send");
        session.transport.process(&mut left, &mut right);
        let mut wraps = 0;
        let mut previous = session.handle.status().played_bar;
        let period =
            range_frames(&performance, range(first, bars - 1), speed(100), RATE).expect("period");
        let cap = 3 * period / 512 + 8;
        let mut blocks = 0;
        while wraps < 2 {
            blocks += 1;
            assert!(
                blocks <= cap,
                "{name}: {wraps} wraps in {blocks} blocks, the loop never wrapped"
            );
            let before = session.transport.chase_steps();
            session.transport.process(&mut left, &mut right);
            let now = session.handle.status().played_bar;
            if now < previous {
                wraps += 1;
                let steps = session.transport.chase_steps() - before;
                assert_eq!(steps, 0, "{name} wrap {wraps}: {steps} steps");
            } else {
                assert_eq!(session.transport.chase_steps(), before, "{name}");
            }
            previous = now;
        }
    }
    assert!(scan_would_exceed);
}

struct QuietFirst {
    name: String,
    performance: Performance,
    bar: usize,
    track: usize,
}

fn with_late_track(name: &str, late: usize) -> Performance {
    let mut score = fixture(name);
    score.anacrusis = true;
    if let Some(track) = score.tracks.get_mut(late) {
        for staff in &mut track.staves {
            if let Some(bar) = staff.bars.first_mut() {
                for voice in &mut bar.voices {
                    let keep = voice.beats.len().saturating_sub(1);
                    voice.beats.drain(..keep);
                }
            }
        }
    }
    performance(&score, Profile::Musical)
}

fn first_note_frame(log: &[Heard]) -> Option<u64> {
    log.iter()
        .filter(
            |heard| matches!(heard.message, MidiMessage::NoteOn { velocity, .. } if velocity > 0),
        )
        .map(|heard| heard.frame)
        .min()
}

#[test]
fn the_count_in_ends_on_the_first_note_of_a_track_that_is_heard() {
    let percent = 100u16;
    let mut found: Vec<QuietFirst> = Vec::new();
    for name in fixture_names() {
        let tracks = performance(&fixture(&name), Profile::Musical).tracks.len();
        for late in 0..tracks.min(2) {
            let performance = with_late_track(&name, late);
            let all = count_in_expectation(&performance, 0, percent);
            for track in 0..tracks {
                let mut hearing = vec![true; tracks];
                hearing[track] = false;
                if count_in_heard_by(&performance, 0, percent, &hearing).lead > all.lead {
                    found.push(QuietFirst {
                        name: name.clone(),
                        performance: performance.clone(),
                        bar: 0,
                        track,
                    });
                }
            }
        }
    }
    assert!(!found.is_empty());
    for case in &found {
        let performance = &case.performance;
        let tracks = performance.tracks.len();
        let others: Vec<usize> = (0..tracks).filter(|track| *track != case.track).collect();
        for solo in [false, true] {
            let mut hearing = vec![true; tracks];
            hearing[case.track] = false;
            let expected = count_in_heard_by(performance, case.bar, percent, &hearing);
            let recorders = recorders(performance);
            let click = recording();
            let mut session = Session::with_click(
                performance.clone(),
                recorders.voices,
                click.synth,
                count_in_config(percent),
            )
            .expect("session");
            if solo {
                for track in &others {
                    session
                        .handle
                        .send(TransportCommand::Solo(*track, true))
                        .expect("send");
                }
            } else {
                session
                    .handle
                    .send(TransportCommand::Mute(case.track, true))
                    .expect("send");
            }
            session
                .handle
                .send(TransportCommand::Seek(case.bar))
                .expect("send");
            session.handle.send(TransportCommand::Play).expect("send");
            let bar = &performance.order.bars[case.bar];
            let tempo = &performance.tempo;
            let length = unit_of(
                tempo,
                ScoreTime::new(bar.start.whole_notes() + bar.length).expect("time"),
                RATE,
            ) - unit_of(tempo, bar.start, RATE);
            let count_in = length / u64::from(percent);
            render_session(&mut session, &[], count_in + 10).expect("render");
            let logs: Vec<Vec<Heard>> = recorders.logs.iter().map(heard).collect();
            assert_eq!(
                clicks_of(&heard(&click.log)),
                expected.clicks,
                "{}",
                case.name
            );
            let first = others
                .iter()
                .filter_map(|track| first_note_frame(&logs[*track]))
                .min();
            assert_eq!(first, Some(count_in), "{} bar {}", case.name, case.bar);
        }
    }
}

fn with_zero_length_tail(name: &str) -> Performance {
    let mut performance = performance(&fixture(name), Profile::Musical);
    let last = performance.order.bars.last().cloned().expect("a bar");
    let start = ScoreTime::new(performance.order.end().whole_notes() + Fraction::new_raw(1, 4))
        .expect("time");
    performance.order.bars.push(PlayedBar {
        source: last.source,
        start,
        length: Fraction::new_raw(0, 1),
        pass: last.pass,
    });
    performance
}

fn silent_session(performance: &Performance) -> Session {
    Session::new(
        performance.clone(),
        built_in_voices(performance, RATE),
        config(RATE),
    )
    .expect("session")
}

#[test]
fn the_song_end_inside_a_loop_wraps_to_the_loop_start() {
    let performance = with_zero_length_tail("repeats.gp");
    let tail = performance.order.bars.len() - 1;
    let mut session = silent_session(&performance);
    let mut left = vec![0.0f32; 512];
    let mut right = vec![0.0f32; 512];
    let span = range(tail - 1, tail);
    let period = range_frames(&performance, span, speed(100), RATE).expect("period");
    let cap = 3 * period / 512 + 8;
    session
        .handle
        .send(TransportCommand::Loop(span))
        .expect("send");
    session.handle.send(TransportCommand::Play).expect("send");
    let mut wraps = 0;
    let mut previous = tail - 1;
    let mut blocks = 0;
    while wraps < 2 {
        blocks += 1;
        assert!(
            blocks <= cap,
            "{wraps} wraps in {blocks} blocks, the loop never wrapped at the song end"
        );
        session.transport.process(&mut left, &mut right);
        let status = session.handle.status();
        assert_eq!(status.state, opentab_audio::TransportState::Playing);
        assert!(status.played_bar >= tail - 1);
        if status.played_bar < previous {
            wraps += 1;
        }
        previous = status.played_bar;
    }
    session
        .handle
        .send(TransportCommand::ClearLoop)
        .expect("send");
    let song = song_frames(&performance, 100) / 512 + 8;
    let mut blocks = 0;
    loop {
        blocks += 1;
        assert!(
            blocks <= song,
            "the song never stopped after the loop was cleared"
        );
        session.transport.process(&mut left, &mut right);
        if session.handle.status().state == opentab_audio::TransportState::Stopped {
            break;
        }
    }
    assert_eq!(session.handle.status().played_bar, 0);
}

#[test]
fn a_loop_over_a_zero_length_range_is_refused() {
    let performance = with_zero_length_tail("repeats.gp");
    let tail = performance.order.bars.len() - 1;
    let mut session = silent_session(&performance);
    let mut left = vec![0.0f32; 512];
    let mut right = vec![0.0f32; 512];
    session
        .handle
        .send(TransportCommand::Loop(range(tail, tail)))
        .expect("send");
    session.handle.send(TransportCommand::Play).expect("send");
    for _ in 0..8 {
        session.transport.process(&mut left, &mut right);
        let status = session.handle.status();
        assert_eq!(status.state, opentab_audio::TransportState::Playing);
        assert_eq!(status.played_bar, 0);
    }
}

#[test]
fn a_mute_or_solo_during_the_count_in_aims_it_at_the_first_note_still_heard() {
    let percent = 100u16;
    let mut found: Vec<QuietFirst> = Vec::new();
    for name in fixture_names() {
        let tracks = performance(&fixture(&name), Profile::Musical).tracks.len();
        for late in 0..tracks.min(2) {
            let performance = with_late_track(&name, late);
            let all = count_in_expectation(&performance, 0, percent);
            for track in 0..tracks {
                let mut hearing = vec![true; tracks];
                hearing[track] = false;
                if count_in_heard_by(&performance, 0, percent, &hearing).lead > all.lead {
                    found.push(QuietFirst {
                        name: name.clone(),
                        performance: performance.clone(),
                        bar: 0,
                        track,
                    });
                }
            }
        }
    }
    assert!(!found.is_empty());
    for case in &found {
        let performance = &case.performance;
        let tracks = performance.tracks.len();
        let others: Vec<usize> = (0..tracks).filter(|track| *track != case.track).collect();
        let mut hearing = vec![true; tracks];
        hearing[case.track] = false;
        let before = count_in_expectation(performance, case.bar, percent);
        let after = count_in_heard_by(performance, case.bar, percent, &hearing);
        let bar = &performance.order.bars[case.bar];
        let tempo = &performance.tempo;
        let length = unit_of(
            tempo,
            ScoreTime::new(bar.start.whole_notes() + bar.length).expect("time"),
            RATE,
        ) - unit_of(tempo, bar.start, RATE);
        let count_in = length / u64::from(percent);
        let cut = (1..8)
            .map(|part| count_in * part / 8)
            .find(|cut| {
                before
                    .clicks
                    .iter()
                    .chain(&after.clicks)
                    .all(|click| click.frame.abs_diff(*cut) > 2)
            })
            .expect("a frame between the clicks");
        for solo in [false, true] {
            let commands: Vec<TransportCommand> = if solo {
                others
                    .iter()
                    .map(|track| TransportCommand::Solo(*track, true))
                    .collect()
            } else {
                vec![TransportCommand::Mute(case.track, true)]
            };
            let script: Vec<ScriptStep> =
                commands.iter().map(|command| step(cut, *command)).collect();
            let ClickRecording { logs, clicks } = record_script_with_click(
                performance,
                count_in_config(percent),
                &script,
                count_in + 10,
            );
            let expected: Vec<HeardClick> = before
                .clicks
                .iter()
                .filter(|click| click.frame < cut)
                .chain(after.clicks.iter().filter(|click| click.frame >= cut))
                .copied()
                .collect();
            assert_eq!(clicks_of(&clicks), expected, "{} solo {solo}", case.name);
            let first = others
                .iter()
                .filter_map(|track| first_note_frame(&logs[*track]))
                .min();
            assert_eq!(first, Some(count_in), "{} solo {solo}", case.name);
        }
    }
}
