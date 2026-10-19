use opentab_model::{
    Bend, BendPoint, Dynamic, Fraction, Grace, Harmonic, HarmonicKind, InstrumentKind, MidiBank,
    MidiChannel, NoteValue, OpaquePayload, Transpose, TremoloPicking, Trill, TrillSpeed,
};
use opentab_playback::{
    BEND_CENTRE, Channel, ChannelState, ConductorKind, MidiMessage, PAN_CONTROLLER,
    PITCH_BEND_RANGE_RPN, Performance, PlaybackOptions, PlaybackWarning, Profile, ScoreTime,
    TimedEvent, UnplayedKind, VOLUME_CONTROLLER, beat_grid, perform, velocity_of,
};

use crate::build::{Builder, beat, beat_with, fretted, hit, rest, with};
use crate::support::frac;

const GP: PlaybackOptions = PlaybackOptions {
    profile: Profile::Gp,
};
const MUSICAL: PlaybackOptions = PlaybackOptions {
    profile: Profile::Musical,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Heard {
    start: Fraction,
    end: Fraction,
    key: u8,
    velocity: u8,
    channel: u8,
}

fn heard(performance: &Performance, track: usize) -> Vec<Heard> {
    let mut open: Vec<(u8, u8, Fraction, u8)> = Vec::new();
    let mut out = Vec::new();
    for event in performance
        .events
        .iter()
        .filter(|event| event.track == track)
    {
        match event.message {
            MidiMessage::NoteOn { key, velocity } => {
                open.push((event.channel.get(), key, event.at.whole_notes(), velocity));
            }
            MidiMessage::NoteOff { key, .. } => {
                let index = open
                    .iter()
                    .position(|(channel, candidate, _, _)| {
                        *channel == event.channel.get() && *candidate == key
                    })
                    .expect("note-off pairs with an earlier note-on");
                let (channel, key, start, velocity) = open.remove(index);
                out.push(Heard {
                    start,
                    end: event.at.whole_notes(),
                    key,
                    velocity,
                    channel,
                });
            }
            _ => {}
        }
    }
    assert!(open.is_empty(), "every note-on is released");
    out.sort();
    out
}

fn note(start: Fraction, end: Fraction, key: u8) -> (Fraction, Fraction, u8) {
    (start, end, key)
}

fn shape(notes: &[Heard]) -> Vec<(Fraction, Fraction, u8)> {
    notes
        .iter()
        .map(|heard| note(heard.start, heard.end, heard.key))
        .collect()
}

fn bends(performance: &Performance) -> Vec<(Fraction, u16)> {
    performance
        .events
        .iter()
        .filter_map(|event| match event.message {
            MidiMessage::PitchBend { value } => Some((event.at.whole_notes(), value)),
            _ => None,
        })
        .collect()
}

fn single_note(edit: impl FnOnce(&mut Builder, usize)) -> Performance {
    let mut builder = Builder::new(1);
    let track = builder.guitar();
    edit(&mut builder, track);
    perform(&builder.score, &GP).expect("perform")
}

#[test]
fn a_guitar_with_octave_minus_one_sounds_at_tuning_plus_capo_plus_fret() {
    let mut builder = Builder::new(1);
    let track = builder.track(InstrumentKind::Fretted, &[64, 59, 55, 50, 45, 40], 2);
    builder.score.tracks[track].transpose = Transpose::new(5, -1).expect("transpose");
    builder.voice(
        track,
        0,
        0,
        vec![beat(NoteValue::Whole, vec![fretted(2, 3)])],
    );
    for options in [GP, MUSICAL] {
        let performance = perform(&builder.score, &options).expect("perform");
        let keys: Vec<u8> = heard(&performance, track).iter().map(|h| h.key).collect();
        assert_eq!(keys, vec![55 + 2 + 3]);
    }
}

#[test]
fn velocities_follow_guitar_pros_dynamic_table() {
    let dynamics = [
        Dynamic::Ppp,
        Dynamic::Pp,
        Dynamic::P,
        Dynamic::Mp,
        Dynamic::Mf,
        Dynamic::F,
        Dynamic::Ff,
        Dynamic::Fff,
    ];
    let table: Vec<u8> = dynamics
        .iter()
        .map(|dynamic| velocity_of(*dynamic))
        .collect();
    assert_eq!(table, vec![25, 38, 51, 64, 76, 89, 102, 114]);
    let mut builder = Builder::new(2);
    let track = builder.guitar();
    for (bar, chunk) in dynamics.chunks(4).enumerate() {
        let beats = chunk
            .iter()
            .map(|dynamic| {
                beat_with(beat(NoteValue::Quarter, vec![fretted(0, 0)]), |effects| {
                    effects.dynamic = *dynamic;
                })
            })
            .collect();
        builder.voice(track, bar, 0, beats);
    }
    let performance = perform(&builder.score, &GP).expect("perform");
    let velocities: Vec<u8> = heard(&performance, track)
        .iter()
        .map(|h| h.velocity)
        .collect();
    assert_eq!(velocities, table);
}

#[test]
fn a_tied_note_sounds_once_for_its_whole_length() {
    let mut builder = Builder::new(2);
    let track = builder.guitar();
    let origin = with(fretted(1, 1), |effects| effects.tie_origin = true);
    let middle = with(fretted(1, 1), |effects| {
        effects.tie_origin = true;
        effects.tie_destination = true;
    });
    let last = with(fretted(1, 1), |effects| effects.tie_destination = true);
    builder.voice(
        track,
        0,
        0,
        vec![
            rest(NoteValue::Half),
            beat(NoteValue::Quarter, vec![origin]),
            beat(NoteValue::Quarter, vec![middle]),
        ],
    );
    builder.voice(
        track,
        1,
        0,
        vec![
            beat(NoteValue::Half, vec![last]),
            beat(NoteValue::Half, vec![fretted(1, 1)]),
        ],
    );
    let performance = perform(&builder.score, &GP).expect("perform");
    assert_eq!(
        shape(&heard(&performance, track)),
        vec![
            note(frac(1, 2), frac(3, 2), 60),
            note(frac(3, 2), frac(2, 1), 60),
        ]
    );
}

#[test]
fn percussion_plays_its_kit_keys_on_the_drum_channel_in_both_profiles() {
    let mut builder = Builder::new(1);
    let drums = builder.track(InstrumentKind::Percussion, &[], 0);
    builder.score.tracks[drums].midi.primary = MidiChannel::new(3).expect("channel");
    builder.voice(
        drums,
        0,
        0,
        vec![beat(NoteValue::Whole, vec![hit(0), hit(3)])],
    );
    for options in [GP, MUSICAL] {
        let performance = perform(&builder.score, &options).expect("perform");
        let notes = heard(&performance, drums);
        assert_eq!(notes.len(), 2);
        assert!(notes.iter().all(|h| h.channel == Channel::DRUMS.get()));
        assert_eq!(
            notes.iter().map(|h| h.key).collect::<Vec<_>>(),
            vec![35, 38]
        );
        assert!(
            performance
                .events
                .iter()
                .all(|event| event.channel == Channel::DRUMS)
        );
        assert_eq!(performance.tracks[drums].channel, Channel::DRUMS);
    }
}

#[test]
fn an_unreadable_kept_kit_plays_the_general_midi_keys_and_is_listed() {
    let mut builder = Builder::new(1);
    let drums = builder.track(InstrumentKind::Percussion, &[], 0);
    let id = builder.score.tracks[drums].id;
    builder.score.extensions.0.insert(
        id.0,
        vec![OpaquePayload {
            format: "gpif".to_string(),
            data: "<kept parent=\"InstrumentSet\" guard=\"drum-kit:percussion none\"><Elements/></kept>"
                .to_string(),
        }],
    );
    assert_eq!(
        builder
            .score
            .articulation_count(&builder.score.tracks[drums]),
        None
    );
    builder.voice(
        drums,
        0,
        0,
        vec![beat(NoteValue::Whole, vec![hit(1), hit(500)])],
    );
    let performance = perform(&builder.score, &GP).expect("perform");
    assert_eq!(
        heard(&performance, drums)
            .iter()
            .map(|h| h.key)
            .collect::<Vec<_>>(),
        vec![36]
    );
    assert!(performance.unplayed.iter().any(|feature| {
        feature.kind == UnplayedKind::UnreadableDrumKit && feature.track == Some(drums)
    }));
    assert!(performance.warnings.iter().any(|warning| matches!(
        warning,
        PlaybackWarning::UnknownArticulation {
            articulation: 500,
            ..
        }
    )));
}

#[test]
fn a_pitch_above_127_is_dropped_and_reported() {
    let mut builder = Builder::new(1);
    let track = builder.track(InstrumentKind::Fretted, &[120, 60], 5);
    builder.voice(
        track,
        0,
        0,
        vec![beat(NoteValue::Whole, vec![fretted(0, 10), fretted(1, 2)])],
    );
    let performance = perform(&builder.score, &GP).expect("perform");
    assert_eq!(
        heard(&performance, track)
            .iter()
            .map(|h| h.key)
            .collect::<Vec<_>>(),
        vec![67]
    );
    assert_eq!(
        performance.warnings,
        vec![PlaybackWarning::PitchOutOfRange {
            track,
            at: ScoreTime::ZERO,
            pitch: 135,
        }]
    );
}

#[test]
fn musical_gives_each_melodic_track_its_own_channel_until_the_pool_runs_out() {
    let mut builder = Builder::new(1);
    let melodic: Vec<usize> = (0..18).map(|_| builder.guitar()).collect();
    let drums: Vec<usize> = (0..2)
        .map(|_| builder.track(InstrumentKind::Percussion, &[], 0))
        .collect();
    let performance = perform(&builder.score, &MUSICAL).expect("perform");
    let channels: Vec<u8> = melodic
        .iter()
        .map(|track| performance.tracks[*track].channel.get())
        .collect();
    let first: Vec<u8> = channels[..15].to_vec();
    let mut distinct = first.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), 15);
    assert!(!first.contains(&Channel::DRUMS.get()));
    let fallbacks: Vec<usize> = performance
        .warnings
        .iter()
        .filter_map(|warning| match warning {
            PlaybackWarning::ChannelFallback { track, .. } => Some(*track),
            _ => None,
        })
        .collect();
    assert_eq!(fallbacks, melodic[15..].to_vec());
    for track in drums {
        assert_eq!(performance.tracks[track].channel, Channel::DRUMS);
    }
}

#[test]
fn musical_keeps_a_free_primary_channel_and_gp_keeps_every_primary() {
    let mut builder = Builder::new(1);
    let first = builder.guitar();
    let second = builder.guitar();
    builder.score.tracks[first].midi.primary = MidiChannel::new(4).expect("channel");
    builder.score.tracks[second].midi.primary = MidiChannel::new(4).expect("channel");
    let gp = perform(&builder.score, &GP).expect("perform");
    assert_eq!(gp.tracks[first].channel.get(), 4);
    assert_eq!(gp.tracks[second].channel.get(), 4);
    assert!(gp.warnings.is_empty());
    let musical = perform(&builder.score, &MUSICAL).expect("perform");
    assert_eq!(musical.tracks[first].channel.get(), 4);
    assert_eq!(musical.tracks[second].channel.get(), 0);
}

#[test]
fn every_track_starts_with_its_program_and_a_six_semitone_bend_range() {
    let performance = single_note(|builder, track| {
        builder.score.tracks[track].instrument.midi_program = 30;
        builder.voice(
            track,
            0,
            0,
            vec![beat(NoteValue::Whole, vec![fretted(0, 0)])],
        );
    });
    let setup: Vec<MidiMessage> = performance
        .events
        .iter()
        .filter(|event| event.at == ScoreTime::ZERO)
        .map(|event| event.message)
        .collect();
    assert_eq!(
        setup,
        vec![
            MidiMessage::Program { program: 30 },
            MidiMessage::Rpn {
                parameter: PITCH_BEND_RANGE_RPN,
                value: 6,
            },
            MidiMessage::NoteOn {
                key: 64,
                velocity: 76,
            },
        ]
    );
}

fn replay(performance: &Performance, track: usize, at: ScoreTime) -> ChannelState {
    let mut state = performance.tracks[track].channels[0].state;
    for event in &performance.events {
        if event.track != track || event.at > at {
            continue;
        }
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
    state
}

fn bent_performance() -> Performance {
    let mut builder = Builder::new(1);
    let track = builder.guitar();
    builder.score.tracks[track].midi.bank = MidiBank::new(1, 3).expect("bank");
    let bend = Bend::new(
        BendPoint::new(0, 0).expect("point"),
        BendPoint::new(50, 200).expect("point"),
        BendPoint::new(50, 200).expect("point"),
        BendPoint::new(100, 200).expect("point"),
    )
    .expect("bend");
    builder.voice(
        track,
        0,
        0,
        vec![
            beat(
                NoteValue::Half,
                vec![with(fretted(2, 2), |effects| effects.bend = Some(bend))],
            ),
            beat(NoteValue::Half, vec![fretted(2, 2)]),
        ],
    );
    perform(&builder.score, &GP).expect("perform")
}

#[test]
fn chase_restores_every_controller_and_the_bend_centre_after_a_bend() {
    let performance = bent_performance();
    let during = ScoreTime::new(frac(3, 8)).expect("time");
    let after = ScoreTime::new(frac(3, 4)).expect("time");
    for at in [ScoreTime::ZERO, during, after] {
        let chased = performance.chase(0, at);
        let messages: Vec<MidiMessage> = chased.iter().map(|event| event.message).collect();
        let expected = replay(&performance, 0, at);
        assert_eq!(messages, expected.messages().to_vec());
        assert!(
            chased
                .iter()
                .all(|event| event.at == at && event.track == 0)
        );
        assert!(matches!(messages[0], MidiMessage::Bank { .. }));
        assert!(matches!(messages[5], MidiMessage::PitchBend { .. }));
    }
    let middle = performance.chase(0, during);
    assert_ne!(
        middle[5].message,
        MidiMessage::PitchBend { value: BEND_CENTRE }
    );
    let later = performance.chase(0, after);
    assert_eq!(
        later[5].message,
        MidiMessage::PitchBend { value: BEND_CENTRE }
    );
    assert_eq!(
        later[0].message,
        MidiMessage::Bank {
            bank: MidiBank::default()
        }
    );
    assert_eq!(performance.chase(9, after), Vec::<TimedEvent>::new());
}

#[test]
fn a_bend_follows_guitar_pros_steps_and_values() {
    let performance = bent_performance();
    let at = |ticks: i64| frac(ticks, 1920);
    let rise: Vec<(Fraction, u16)> = bends(&performance)
        .into_iter()
        .filter(|(time, _)| *time < frac(1, 2))
        .collect();
    let mut expected = vec![(at(0), 8192)];
    for k in 1..=8 {
        expected.push((
            at(0) + frac(960, 1920) * frac(12 * k, 100),
            8192 + u16::try_from(k * 12 * 8191 / 300).expect("bend"),
        ));
    }
    assert_eq!(rise, expected);
    let tail: Vec<u16> = bends(&performance)
        .into_iter()
        .filter(|(time, _)| *time == frac(1, 2))
        .map(|(_, value)| value)
        .collect();
    assert_eq!(tail, vec![10922, BEND_CENTRE]);
}

#[test]
fn a_bend_and_release_rises_holds_and_falls_by_its_middle_points() {
    let bend = Bend::new(
        BendPoint::new(0, 0).expect("point"),
        BendPoint::new(25, 100).expect("point"),
        BendPoint::new(75, 100).expect("point"),
        BendPoint::new(100, 0).expect("point"),
    )
    .expect("bend");
    let performance = single_note(|builder, track| {
        builder.voice(
            track,
            0,
            0,
            vec![beat(
                NoteValue::Whole,
                vec![with(fretted(2, 2), |effects| effects.bend = Some(bend))],
            )],
        );
    });
    let values = bends(&performance);
    let peak = 8192 + 50 * 8191 / 300;
    assert_eq!(values.first(), Some(&(frac(0, 1), 8192)));
    assert!(values.contains(&(frac(1, 4), u16::try_from(peak).expect("peak"))));
    assert!(values.contains(&(frac(3, 4), u16::try_from(peak).expect("peak"))));
    assert_eq!(
        values
            .iter()
            .filter(|(time, _)| *time > frac(1, 4) && *time < frac(3, 4))
            .count(),
        0
    );
    assert_eq!(values.last(), Some(&(frac(1, 1), BEND_CENTRE)));
}

#[test]
fn a_bend_below_the_pitch_is_not_played_and_is_listed() {
    let dip = Bend::new(
        BendPoint::new(0, 0).expect("point"),
        BendPoint::new(25, -100).expect("point"),
        BendPoint::new(75, -100).expect("point"),
        BendPoint::new(100, 0).expect("point"),
    )
    .expect("bend");
    let performance = single_note(|builder, track| {
        builder.voice(
            track,
            0,
            0,
            vec![beat(
                NoteValue::Whole,
                vec![with(fretted(2, 2), |effects| effects.bend = Some(dip))],
            )],
        );
    });
    assert!(bends(&performance).is_empty());
    assert!(
        performance
            .unplayed
            .iter()
            .any(|feature| feature.kind == UnplayedKind::BendBelowPitch)
    );
}

#[test]
fn vibrato_only_centres_the_bend_at_the_start_and_end_like_guitar_pro() {
    let performance = single_note(|builder, track| {
        builder.voice(
            track,
            0,
            0,
            vec![beat(
                NoteValue::Whole,
                vec![with(fretted(2, 2), |effects| {
                    effects.vibrato = Some(opentab_model::Vibrato::Wide);
                })],
            )],
        );
    });
    assert_eq!(
        bends(&performance),
        vec![
            (frac(0, 1), BEND_CENTRE),
            (frac(1, 1), BEND_CENTRE),
            (frac(1, 1), BEND_CENTRE)
        ]
    );
}

#[test]
fn grace_notes_take_their_time_before_or_on_the_beat() {
    let grace = |kind: Grace| {
        beat_with(
            beat(NoteValue::Eighth, vec![fretted(2, 4)]),
            move |effects| {
                effects.grace = Some(kind);
            },
        )
    };
    let performance = single_note(|builder, track| {
        builder.voice(
            track,
            0,
            0,
            vec![
                beat(NoteValue::Quarter, vec![fretted(2, 2)]),
                grace(Grace::BeforeBeat),
                beat(NoteValue::Quarter, vec![fretted(2, 5)]),
                grace(Grace::OnBeat),
                beat(NoteValue::Quarter, vec![fretted(2, 5)]),
                rest(NoteValue::Quarter),
            ],
        );
    });
    assert_eq!(
        shape(&heard(&performance, 0)),
        vec![
            note(frac(0, 1), frac(1, 4), 57),
            note(frac(1, 8), frac(1, 4), 59),
            note(frac(1, 4), frac(1, 2), 60),
            note(frac(1, 2), frac(5, 8), 59),
            note(frac(5, 8), frac(3, 4), 60),
        ]
    );
}

#[test]
fn a_grace_note_at_the_very_start_plays_on_the_beat() {
    let performance = single_note(|builder, track| {
        builder.voice(
            track,
            0,
            0,
            vec![
                beat_with(beat(NoteValue::Eighth, vec![fretted(2, 4)]), |effects| {
                    effects.grace = Some(Grace::BeforeBeat);
                }),
                beat(NoteValue::Whole, vec![fretted(2, 5)]),
            ],
        );
    });
    assert_eq!(
        shape(&heard(&performance, 0)),
        vec![
            note(frac(0, 1), frac(1, 8), 59),
            note(frac(1, 8), frac(1, 1), 60)
        ]
    );
}

#[test]
fn staccato_halves_a_note_unless_it_is_accented() {
    let performance = single_note(|builder, track| {
        builder.voice(
            track,
            0,
            0,
            vec![
                beat(
                    NoteValue::Half,
                    vec![with(fretted(0, 0), |e| e.staccato = true)],
                ),
                beat(
                    NoteValue::Half,
                    vec![with(fretted(0, 0), |e| {
                        e.staccato = true;
                        e.accent = Some(opentab_model::Accent::Normal);
                    })],
                ),
            ],
        );
    });
    assert_eq!(
        shape(&heard(&performance, 0)),
        vec![
            note(frac(0, 1), frac(1, 4), 64),
            note(frac(1, 2), frac(1, 1), 64)
        ]
    );
}

#[test]
fn tremolo_picking_repeats_the_note_at_its_value() {
    let performance = single_note(|builder, track| {
        builder.voice(
            track,
            0,
            0,
            vec![
                beat_with(beat(NoteValue::Half, vec![fretted(0, 0)]), |effects| {
                    effects.tremolo_picking =
                        Some(TremoloPicking::new(NoteValue::Eighth).expect("tremolo"));
                }),
                rest(NoteValue::Half),
            ],
        );
    });
    let starts: Vec<Fraction> = heard(&performance, 0).iter().map(|h| h.start).collect();
    assert_eq!(starts, vec![frac(0, 1), frac(1, 8), frac(1, 4), frac(3, 8)]);
}

#[test]
fn a_trill_alternates_from_the_principal_at_its_speed() {
    let performance = single_note(|builder, track| {
        builder.voice(
            track,
            0,
            0,
            vec![
                beat(
                    NoteValue::Quarter,
                    vec![with(fretted(0, 5), |effects| {
                        effects.trill = Some(Trill {
                            fret: 7,
                            speed: TrillSpeed::new(NoteValue::Sixteenth).expect("speed"),
                        });
                    })],
                ),
                rest(NoteValue::Quarter),
                rest(NoteValue::Half),
            ],
        );
    });
    assert_eq!(
        shape(&heard(&performance, 0)),
        vec![
            note(frac(0, 1), frac(1, 16), 69),
            note(frac(1, 16), frac(1, 8), 71),
            note(frac(1, 8), frac(3, 16), 69),
            note(frac(3, 16), frac(1, 4), 71),
        ]
    );
}

#[test]
fn harmonics_sound_at_guitar_pros_pitches() {
    let harmonic = |kind: HarmonicKind, fret: Fraction| Harmonic { kind, fret };
    let cases = [
        (0, 12, harmonic(HarmonicKind::Natural, frac(12, 1)), 76),
        (1, 7, harmonic(HarmonicKind::Natural, frac(7, 1)), 78),
        (2, 3, harmonic(HarmonicKind::Natural, frac(16, 5)), 86),
        (3, 5, harmonic(HarmonicKind::Artificial, frac(17, 1)), 91),
        (2, 5, harmonic(HarmonicKind::Pinch, frac(12, 1)), 72),
        (2, 5, harmonic(HarmonicKind::Natural, frac(5, 1)), 79),
        (2, 5, harmonic(HarmonicKind::Tap, frac(1, 1)), 60),
    ];
    let performance = single_note(|builder, track| {
        builder.voice(
            track,
            0,
            0,
            cases
                .iter()
                .map(|(string, fret, harmonic, _)| {
                    beat(
                        NoteValue::Eighth,
                        vec![with(fretted(*string, *fret), |effects| {
                            effects.harmonic = Some(*harmonic);
                        })],
                    )
                })
                .collect(),
        );
    });
    let keys: Vec<u8> = heard(&performance, 0).iter().map(|h| h.key).collect();
    assert_eq!(keys, cases.iter().map(|case| case.3).collect::<Vec<_>>());
}

#[test]
fn a_pickup_bar_plays_its_notes_at_the_end_of_the_full_bar() {
    let mut builder = Builder::new(2);
    builder.score.anacrusis = true;
    let track = builder.guitar();
    builder.voice(
        track,
        0,
        0,
        vec![beat(NoteValue::Quarter, vec![fretted(0, 0)])],
    );
    builder.voice(
        track,
        1,
        0,
        vec![beat(NoteValue::Quarter, vec![fretted(0, 1)])],
    );
    let performance = perform(&builder.score, &GP).expect("perform");
    assert_eq!(
        shape(&heard(&performance, track)),
        vec![
            note(frac(3, 4), frac(1, 1), 64),
            note(frac(1, 1), frac(5, 4), 65)
        ]
    );
}

#[test]
fn guitar_pro_ignored_features_are_listed_and_ramps_only_under_gp() {
    let mut builder = Builder::new(1);
    let track = builder.guitar();
    builder.score.master_bars[0]
        .tempo_changes
        .push(crate::support::tempo(
            frac(0, 1),
            90.0,
            opentab_model::TempoUnit::Quarter,
            true,
        ));
    builder.voice(
        track,
        0,
        0,
        vec![beat_with(
            beat(
                NoteValue::Whole,
                vec![with(fretted(0, 0), |effects| {
                    effects.ghost = true;
                    effects.palm_mute = true;
                    effects.let_ring = true;
                })],
            ),
            |effects| effects.fade_in = true,
        )],
    );
    let kinds = |options: PlaybackOptions| -> Vec<UnplayedKind> {
        perform(&builder.score, &options)
            .expect("perform")
            .unplayed
            .iter()
            .map(|feature| feature.kind)
            .collect()
    };
    assert_eq!(
        kinds(GP),
        vec![
            UnplayedKind::TempoRamp,
            UnplayedKind::Ghost,
            UnplayedKind::PalmMute,
            UnplayedKind::LetRing,
            UnplayedKind::FadeIn,
        ]
    );
    assert!(!kinds(MUSICAL).contains(&UnplayedKind::TempoRamp));
}

#[test]
fn the_conductor_restates_tempo_and_bar_zero_signatures_after_a_jump() {
    let mut score = crate::support::score_from(3, "0 [S]; 1 [E2]");
    score.master_bars[1].time_signature = opentab_model::TimeSignature::new(3, 4).expect("time");
    let performance = perform(&score, &GP).expect("perform");
    let kinds: Vec<(Fraction, ConductorKind)> = performance
        .conductor
        .iter()
        .map(|event| (event.at.whole_notes(), event.kind))
        .collect();
    let tempo = ConductorKind::Tempo {
        micros_per_quarter: 500_000,
    };
    let four = ConductorKind::TimeSignature {
        numerator: 4,
        denominator: 4,
    };
    let three = ConductorKind::TimeSignature {
        numerator: 3,
        denominator: 4,
    };
    let key = ConductorKind::KeySignature {
        accidentals: 0,
        minor: false,
    };
    assert_eq!(
        kinds,
        vec![
            (frac(0, 1), tempo),
            (frac(0, 1), four),
            (frac(0, 1), key),
            (frac(1, 1), three),
            (frac(7, 4), tempo),
            (frac(7, 4), four),
            (frac(7, 4), key),
            (frac(11, 4), three),
            (frac(7, 2), four),
        ]
    );
    assert_eq!(performance.clicks, beat_grid(&score, &performance.order));
}
