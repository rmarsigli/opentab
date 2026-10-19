use opentab_model::{
    Accent, Automation, AutomationValue, Balance, BeatEffects, Bend, BendPoint, Dynamic, Fraction,
    Harmonic, HarmonicKind, Level, MidiBank, NoteEffects, NoteValue, Slide, Sound, SoundRef,
    TremoloBar, Vibrato,
};
use opentab_playback::{
    BEND_CENTRE, Channel, ChannelAllocator, ChannelGrant, ChannelNeed, ChannelPool, MidiMessage,
    PAN_CONTROLLER, PITCH_BEND_RANGE_RPN, Performance, PlaybackOptions, PlaybackWarning, Profile,
    ScoreTime, UnplayedKind, VOLUME_CONTROLLER, perform, velocity_of,
};

use crate::build::{BeatSpec, Builder, beat, beat_with, fretted, with};
use crate::support::frac;

const MUSICAL: PlaybackOptions = PlaybackOptions {
    profile: Profile::Musical,
};
const GP: PlaybackOptions = PlaybackOptions {
    profile: Profile::Gp,
};
const RANGE: u8 = 12;
const MUTED_GUITAR: u8 = 28;

type Edit = fn(&mut NoteEffects);
type Knots = [(u8, i16); 4];
type Curve = Vec<(Fraction, i64)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Heard {
    pub start: Fraction,
    pub end: Fraction,
    pub key: u8,
    pub velocity: u8,
    pub channel: u8,
}

pub fn heard(performance: &Performance) -> Vec<Heard> {
    let mut open: Vec<(u8, u8, Fraction, u8)> = Vec::new();
    let mut out = Vec::new();
    for event in &performance.events {
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

fn on(performance: &Performance, channel: u8) -> Vec<(Fraction, MidiMessage)> {
    performance
        .events
        .iter()
        .filter(|event| event.channel.get() == channel)
        .map(|event| (event.at.whole_notes(), event.message))
        .collect()
}

fn bends_on(performance: &Performance, channel: u8) -> Vec<(Fraction, u16)> {
    on(performance, channel)
        .into_iter()
        .filter_map(|(at, message)| match message {
            MidiMessage::PitchBend { value } => Some((at, value)),
            _ => None,
        })
        .collect()
}

fn value(cents: i64) -> u16 {
    let offset = (cents * 8192).div_euclid(i64::from(RANGE) * 100);
    u16::try_from((8192 + offset).clamp(0, 16383)).expect("bend value")
}

fn one_bar(beats: Vec<BeatSpec>) -> Builder {
    let mut builder = Builder::new(1);
    let track = builder.guitar();
    builder.voice(track, 0, 0, beats);
    builder
}

fn musical(builder: &Builder) -> Performance {
    perform(&builder.score, &MUSICAL).expect("perform")
}

fn point(offset: u8, cents: i16) -> BendPoint {
    BendPoint::new(offset, cents).expect("point")
}

fn bend(points: Knots) -> Bend {
    Bend::new(
        point(points[0].0, points[0].1),
        point(points[1].0, points[1].1),
        point(points[2].0, points[2].1),
        point(points[3].0, points[3].1),
    )
    .expect("bend")
}

fn setup_on(performance: &Performance, channel: u8, at: Fraction) -> Vec<MidiMessage> {
    on(performance, channel)
        .into_iter()
        .filter(|(time, message)| {
            *time == at
                && !matches!(
                    message,
                    MidiMessage::NoteOn { .. } | MidiMessage::NoteOff { .. }
                )
        })
        .map(|(_, message)| message)
        .collect()
}

#[test]
fn musical_opens_every_track_with_bank_program_volume_pan_and_a_twelve_semitone_range() {
    let mut builder = one_bar(vec![beat(NoteValue::Whole, vec![fretted(0, 0)])]);
    builder.score.tracks[0].midi.bank = MidiBank::new(2, 5).expect("bank");
    builder.score.tracks[0].midi.volume = Level::new(250_000).expect("level");
    builder.score.tracks[0].midi.pan = Balance::new(1_000_000).expect("balance");
    let performance = musical(&builder);
    assert_eq!(
        setup_on(&performance, 0, frac(0, 1)),
        vec![
            MidiMessage::Bank {
                bank: MidiBank::new(2, 5).expect("bank")
            },
            MidiMessage::Program { program: 27 },
            MidiMessage::Control {
                controller: VOLUME_CONTROLLER,
                value: 50,
            },
            MidiMessage::Control {
                controller: PAN_CONTROLLER,
                value: 127,
            },
            MidiMessage::Rpn {
                parameter: PITCH_BEND_RANGE_RPN,
                value: RANGE,
            },
        ]
    );
    let opening = performance.tracks[0].channels[0].state;
    assert_eq!(
        (opening.volume, opening.pan, opening.bend_range),
        (50, 127, RANGE)
    );
}

#[test]
fn accents_ghosts_palm_mutes_and_dead_notes_move_the_velocity() {
    let cases: [(Edit, i16); 6] = [
        (|_| {}, 0),
        (|effects| effects.accent = Some(Accent::Normal), 16),
        (|effects| effects.accent = Some(Accent::Heavy), 28),
        (|effects| effects.ghost = true, -28),
        (|effects| effects.palm_mute = true, -12),
        (|effects| effects.dead = true, -8),
    ];
    for dynamic in [Dynamic::Ppp, Dynamic::Mf, Dynamic::Fff] {
        for (edit, offset) in cases {
            let builder = one_bar(vec![beat_with(
                beat(NoteValue::Whole, vec![with(fretted(0, 0), edit)]),
                |effects| effects.dynamic = dynamic,
            )]);
            let expected = (i16::from(velocity_of(dynamic)) + offset).clamp(1, 127);
            let heard = heard(&musical(&builder));
            assert_eq!(heard.len(), 1);
            assert_eq!(
                i16::from(heard[0].velocity),
                expected,
                "{dynamic:?} {offset}"
            );
            let gp = heard_gp(&builder);
            assert_eq!(gp[0].velocity, velocity_of(dynamic));
        }
    }
}

fn heard_gp(builder: &Builder) -> Vec<Heard> {
    heard(&perform(&builder.score, &GP).expect("perform"))
}

#[test]
fn staccato_palm_mute_and_dead_notes_shorten_the_note() {
    let cases: [(Edit, Fraction); 5] = [
        (|_| {}, frac(1, 2)),
        (|effects| effects.staccato = true, frac(1, 4)),
        (
            |effects| {
                effects.staccato = true;
                effects.accent = Some(Accent::Normal);
            },
            frac(1, 4),
        ),
        (|effects| effects.palm_mute = true, frac(3, 8)),
        (|effects| effects.dead = true, frac(1, 32)),
    ];
    for (edit, length) in cases {
        let builder = one_bar(vec![
            beat(NoteValue::Half, vec![with(fretted(1, 0), edit)]),
            beat(NoteValue::Half, vec![fretted(0, 0)]),
        ]);
        let heard = heard(&musical(&builder));
        assert_eq!(heard[0].end - heard[0].start, length);
    }
}

#[test]
fn palm_muted_and_dead_guitar_notes_share_a_muted_guitar_channel() {
    let muted = |note| with(note, |effects| effects.palm_mute = true);
    let mut builder = one_bar(vec![
        beat(
            NoteValue::Quarter,
            vec![muted(fretted(5, 0)), muted(fretted(4, 2))],
        ),
        beat(
            NoteValue::Quarter,
            vec![with(fretted(5, 0), |effects| effects.dead = true)],
        ),
        beat(NoteValue::Half, vec![fretted(0, 0)]),
    ]);
    let performance = musical(&builder);
    let heard = heard(&performance);
    let channels: Vec<u8> = heard.iter().map(|note| note.channel).collect();
    assert_eq!(channels, vec![1, 1, 1, 0]);
    assert_eq!(
        setup_on(&performance, 1, frac(0, 1)),
        vec![
            MidiMessage::Bank {
                bank: MidiBank::default()
            },
            MidiMessage::Program {
                program: MUTED_GUITAR
            },
            MidiMessage::Control {
                controller: VOLUME_CONTROLLER,
                value: 100,
            },
            MidiMessage::Control {
                controller: PAN_CONTROLLER,
                value: 64,
            },
            MidiMessage::Rpn {
                parameter: PITCH_BEND_RANGE_RPN,
                value: RANGE,
            },
            MidiMessage::PitchBend { value: BEND_CENTRE },
        ]
    );
    builder.score.tracks[0].instrument.midi_program = 33;
    let bass = heard_musical(&builder);
    assert!(bass.iter().all(|note| note.channel == 0));
}

fn heard_musical(builder: &Builder) -> Vec<Heard> {
    heard(&musical(builder))
}

#[test]
fn a_hammer_on_chain_sounds_once_and_moves_the_pitch_without_a_new_attack() {
    let hammer = |fret| {
        with(fretted(2, fret), |effects| {
            effects.hammer_on_pull_off = true
        })
    };
    let builder = one_bar(vec![
        beat(NoteValue::Quarter, vec![hammer(5)]),
        beat(NoteValue::Quarter, vec![hammer(7)]),
        beat(NoteValue::Quarter, vec![fretted(2, 5)]),
        beat(NoteValue::Quarter, vec![fretted(2, 9)]),
    ]);
    let performance = musical(&builder);
    let heard = heard(&performance);
    assert_eq!(
        heard
            .iter()
            .map(|note| (note.start, note.end, note.key))
            .collect::<Vec<_>>(),
        vec![(frac(0, 1), frac(3, 4), 60), (frac(3, 4), frac(1, 1), 64)]
    );
    let chain = heard[0].channel;
    assert_ne!(chain, 0);
    assert_eq!(heard[1].channel, 0);
    assert_eq!(
        bends_on(&performance, chain),
        vec![
            (frac(0, 1), BEND_CENTRE),
            (frac(1, 4), value(200)),
            (frac(1, 2), value(0)),
        ]
    );
    assert!(
        performance
            .unplayed
            .iter()
            .all(|feature| feature.kind != UnplayedKind::HammerOnPullOff)
    );
}

#[test]
fn a_legato_slide_glides_into_the_next_note_and_a_shift_slide_attacks_it_again() {
    for (slide, attacks) in [(Slide::Legato, 1), (Slide::Shift, 2)] {
        let builder = one_bar(vec![
            beat(
                NoteValue::Half,
                vec![with(fretted(2, 5), |effects| effects.slide = Some(slide))],
            ),
            beat(NoteValue::Half, vec![fretted(2, 9)]),
        ]);
        let performance = musical(&builder);
        let heard = heard(&performance);
        assert_eq!(heard.len(), attacks, "{slide:?}");
        let channel = heard[0].channel;
        assert_ne!(channel, 0);
        let glide = bends_on(&performance, channel);
        let rising: Vec<(Fraction, u16)> = glide
            .iter()
            .copied()
            .filter(|(at, _)| *at >= frac(7, 16) && *at < frac(1, 2))
            .collect();
        assert_eq!(
            rising.first(),
            Some(&(frac(7, 16), BEND_CENTRE)),
            "{slide:?}"
        );
        assert!(rising.windows(2).all(|pair| pair[0].1 < pair[1].1));
        assert_eq!(rising.len(), 20, "{slide:?} {rising:?}");
        if slide == Slide::Legato {
            assert_eq!(heard[0].end, frac(1, 1));
            assert_eq!(glide.last(), Some(&(frac(1, 2), value(400))));
        } else {
            assert_eq!(heard[1].start, frac(1, 2));
            assert_eq!(heard[1].key, 64);
            assert!(glide.iter().all(|(at, _)| *at < frac(1, 2)));
        }
    }
}

#[test]
fn slides_into_and_out_of_a_note_glide_in_score_time() {
    let cases: [(Slide, i64, i64, Fraction, Fraction); 4] = [
        (Slide::IntoFromBelow, -300, 0, frac(0, 1), frac(1, 16)),
        (Slide::IntoFromAbove, 300, 0, frac(0, 1), frac(1, 16)),
        (Slide::OutDownwards, 0, -500, frac(3, 8), frac(1, 2)),
        (Slide::OutUpwards, 0, 500, frac(3, 8), frac(1, 2)),
    ];
    for (slide, from, to, start, end) in cases {
        let builder = one_bar(vec![
            beat(
                NoteValue::Half,
                vec![with(fretted(2, 5), |effects| effects.slide = Some(slide))],
            ),
            beat(NoteValue::Half, vec![fretted(1, 5)]),
        ]);
        let performance = musical(&builder);
        let channel = heard(&performance)[0].channel;
        let curve: Vec<(Fraction, u16)> = bends_on(&performance, channel)
            .into_iter()
            .skip(1)
            .filter(|(at, _)| *at >= start)
            .collect();
        assert_eq!(
            curve.first().map(|(_, bent)| *bent),
            Some(value(from)),
            "{slide:?}"
        );
        let steps = usize::try_from((to - from).abs() / 20).expect("steps");
        let expected_last = if to == 0 { Some((end, value(0))) } else { None };
        if let Some(last) = expected_last {
            assert_eq!(curve.last(), Some(&last), "{slide:?}");
            assert_eq!(curve.len(), steps + 1, "{slide:?}");
        } else {
            assert_eq!(curve.len(), steps, "{slide:?}");
            assert!(curve.iter().all(|(at, _)| *at < end));
        }
    }
}

#[test]
fn a_bend_plays_its_four_points_including_below_the_pitch() {
    let shapes: [(Knots, Curve); 2] = [
        (
            [(0, 0), (50, 100), (50, 100), (100, 100)],
            vec![
                (frac(0, 1), 0),
                (frac(1, 20), 20),
                (frac(1, 10), 40),
                (frac(3, 20), 60),
                (frac(1, 5), 80),
                (frac(1, 4), 100),
            ],
        ),
        (
            [(0, -40), (20, -40), (20, -40), (60, 0)],
            vec![
                (frac(0, 1), -40),
                (frac(1, 10) + frac(1, 10), -20),
                (frac(3, 10), 0),
            ],
        ),
    ];
    for (points, expected) in shapes {
        let builder = one_bar(vec![
            beat(
                NoteValue::Half,
                vec![with(fretted(2, 5), |effects| {
                    effects.bend = Some(bend(points))
                })],
            ),
            beat(NoteValue::Half, vec![fretted(1, 5)]),
        ]);
        let performance = musical(&builder);
        let channel = heard(&performance)[0].channel;
        let expected: Vec<(Fraction, u16)> = expected
            .into_iter()
            .map(|(at, cents)| (at, value(cents)))
            .collect();
        let found = bends_on(&performance, channel);
        assert_eq!(found[0], (frac(0, 1), BEND_CENTRE));
        assert_eq!(found[1..].to_vec(), expected, "{points:?}");
    }
}

#[test]
fn vibrato_swings_around_the_pitch_eight_times_per_sixteenth_in_score_time() {
    let builder = one_bar(vec![
        beat(
            NoteValue::Quarter,
            vec![with(fretted(2, 5), |effects| {
                effects.vibrato = Some(Vibrato::Wide)
            })],
        ),
        beat(NoteValue::Quarter, vec![fretted(1, 5)]),
        beat(NoteValue::Half, vec![fretted(1, 5)]),
    ]);
    let performance = musical(&builder);
    let channel = heard(&performance)[0].channel;
    let found = bends_on(&performance, channel);
    let sine = [0, 707, 1000, 707, 0, -707, -1000, -707];
    let expected: Vec<(Fraction, u16)> = (0..32)
        .map(|index: i64| {
            let cents = 50 * sine[usize::try_from(index % 8).expect("index")] / 1000;
            (frac(index, 128), value(cents))
        })
        .collect();
    assert_eq!(found[0], (frac(0, 1), BEND_CENTRE));
    assert_eq!(found[1..].to_vec(), expected);
    let mut slow = builder.score.clone();
    slow.master_bars[0].tempo_changes = vec![crate::support::tempo(
        frac(0, 1),
        40.0,
        opentab_model::TempoUnit::Quarter,
        false,
    )];
    let slowed = perform(&slow, &MUSICAL).expect("perform");
    assert_eq!(bends_on(&slowed, channel), found);
}

#[test]
fn a_tremolo_bar_bends_every_note_of_its_beat_on_a_channel_of_its_own() {
    let dive = TremoloBar::new(
        point(0, 0),
        point(50, -1200),
        point(50, -1200),
        point(100, 0),
    )
    .expect("bar");
    let builder = one_bar(vec![
        BeatSpec {
            value: NoteValue::Half,
            notes: vec![fretted(0, 0), fretted(1, 0)],
            effects: BeatEffects {
                tremolo_bar: Some(dive),
                ..BeatEffects::default()
            },
        },
        beat(NoteValue::Half, vec![fretted(0, 0)]),
    ]);
    let performance = musical(&builder);
    let heard = heard(&performance);
    let dived: Vec<u8> = heard
        .iter()
        .filter(|note| note.start == frac(0, 1))
        .map(|note| note.channel)
        .collect();
    assert_eq!(dived.len(), 2);
    assert_ne!(dived[0], dived[1]);
    for channel in dived {
        let values: Vec<u16> = bends_on(&performance, channel)
            .into_iter()
            .map(|(_, bent)| bent)
            .collect();
        assert_eq!(values.iter().min().copied(), Some(0));
    }
    assert!(
        performance
            .unplayed
            .iter()
            .all(|feature| feature.kind != UnplayedKind::TremoloBar)
    );
}

#[test]
fn let_ring_holds_a_note_until_its_string_plays_again_or_a_whole_note_later() {
    let ring = |note| with(note, |effects| effects.let_ring = true);
    let mut builder = Builder::new(3);
    let track = builder.guitar();
    builder.voice(
        track,
        0,
        0,
        vec![
            beat(NoteValue::Quarter, vec![ring(fretted(1, 0))]),
            beat(NoteValue::Quarter, vec![ring(fretted(2, 0))]),
            beat(NoteValue::Half, vec![fretted(1, 2)]),
        ],
    );
    let heard = heard(&musical(&builder));
    let lengths: Vec<(u8, Fraction, Fraction)> = heard
        .iter()
        .map(|note| (note.key, note.start, note.end))
        .collect();
    assert_eq!(
        lengths,
        vec![
            (59, frac(0, 1), frac(1, 2)),
            (55, frac(1, 4), frac(3, 2)),
            (61, frac(1, 2), frac(1, 1)),
        ]
    );
}

#[test]
fn musical_harmonics_sound_by_physics_where_guitar_pro_plays_the_fret() {
    let half_fret = Fraction::new(231, 100);
    let builder = one_bar(vec![beat(
        NoteValue::Whole,
        vec![with(fretted(5, 0), |effects| {
            effects.harmonic = Some(Harmonic {
                kind: HarmonicKind::Natural,
                fret: half_fret,
            });
        })],
    )]);
    assert_eq!(heard_musical(&builder)[0].key, 40 + 36);
    let gp = perform(&builder.score, &GP).expect("perform");
    assert_eq!(heard(&gp)[0].key, 40);
    assert!(
        gp.unplayed
            .iter()
            .any(|feature| feature.kind == UnplayedKind::OffNodeHarmonic)
    );
}

fn bent_guitar(builder: &mut Builder) -> usize {
    let track = builder.guitar();
    builder.voice(
        track,
        0,
        0,
        vec![beat(
            NoteValue::Whole,
            vec![with(fretted(2, 5), |effects| {
                effects.bend = Some(bend([(0, 0), (50, 200), (50, 200), (100, 200)]));
            })],
        )],
    );
    track
}

#[test]
fn a_bent_note_channel_gets_the_tracks_sound_and_later_automations() {
    let mut builder = Builder::new(2);
    let track = bent_guitar(&mut builder);
    let id = builder.score.master_bars[0].id;
    let midi = &mut builder.score.tracks[track].midi;
    midi.bank = MidiBank::new(1, 2).expect("bank");
    midi.volume = Level::new(400_000).expect("level");
    midi.pan = Balance::new(250_000).expect("balance");
    builder.score.tracks[track].sounds = vec![Sound {
        name: "Steel".to_string(),
        program: 25,
        bank: MidiBank::new(3, 0).expect("bank"),
    }];
    builder.score.tracks[track].automations = vec![
        Automation {
            master_bar: id,
            position: frac(1, 2),
            linear: false,
            visible: true,
            value: AutomationValue::Volume(Level::new(300_000).expect("level")),
        },
        Automation {
            master_bar: id,
            position: frac(3, 4),
            linear: false,
            visible: true,
            value: AutomationValue::Sound(SoundRef(0)),
        },
    ];
    let performance = musical(&builder);
    let channel = heard(&performance)[0].channel;
    assert_ne!(channel, 0);
    assert_eq!(
        setup_on(&performance, channel, frac(0, 1)),
        vec![
            MidiMessage::Bank {
                bank: MidiBank::new(1, 2).expect("bank")
            },
            MidiMessage::Program { program: 27 },
            MidiMessage::Control {
                controller: VOLUME_CONTROLLER,
                value: 80,
            },
            MidiMessage::Control {
                controller: PAN_CONTROLLER,
                value: 32,
            },
            MidiMessage::Rpn {
                parameter: PITCH_BEND_RANGE_RPN,
                value: RANGE,
            },
            MidiMessage::PitchBend { value: BEND_CENTRE },
            MidiMessage::PitchBend { value: BEND_CENTRE },
        ]
    );
    for target in [0, channel] {
        assert!(on(&performance, target).contains(&(
            frac(1, 2),
            MidiMessage::Control {
                controller: VOLUME_CONTROLLER,
                value: 60,
            }
        )));
        assert!(
            on(&performance, target).contains(&(frac(3, 4), MidiMessage::Program { program: 25 }))
        );
        assert!(on(&performance, target).contains(&(
            frac(3, 4),
            MidiMessage::Bank {
                bank: MidiBank::new(3, 0).expect("bank")
            }
        )));
    }
    let chased = performance.chase(track, ScoreTime::new(frac(7, 8)).expect("time"));
    let program = chased
        .iter()
        .find(|event| {
            event.channel == Channel::new(channel).expect("channel")
                && matches!(event.message, MidiMessage::Program { .. })
        })
        .map(|event| event.message);
    assert_eq!(program, Some(MidiMessage::Program { program: 25 }));
}

#[test]
fn sixteen_guitars_bending_together_exhaust_the_pool_and_report_the_fallback() {
    let mut builder = Builder::new(1);
    for _ in 0..16 {
        bent_guitar(&mut builder);
    }
    let performance = musical(&builder);
    let fallbacks: Vec<(usize, ScoreTime)> = performance
        .warnings
        .iter()
        .filter_map(|warning| match warning {
            PlaybackWarning::ChannelFallback { track, at } => Some((*track, *at)),
            _ => None,
        })
        .collect();
    let expected: Vec<(usize, ScoreTime)> = std::iter::once(15)
        .chain(0..16)
        .map(|track| (track, ScoreTime::ZERO))
        .collect();
    assert_eq!(fallbacks, expected);
    let heard = heard(&performance);
    assert_eq!(heard.len(), 16);
}

#[test]
fn the_allocator_shares_muted_groups_reuses_released_channels_and_falls_back() {
    let time = |numer, denom| ScoreTime::new(frac(numer, denom)).expect("time");
    let mut pool = ChannelPool::new();
    for _ in 0..13 {
        pool.acquire(None);
    }
    let mut allocator = ChannelAllocator::new(pool);
    let exclusive = ChannelNeed::Exclusive { muted: false };
    assert_eq!(
        allocator.request(0, time(0, 1), time(1, 4), ChannelNeed::Shared),
        ChannelGrant::Shared
    );
    let first = allocator.request(0, time(0, 1), time(1, 2), exclusive);
    let ChannelGrant::Allocated {
        channel,
        fresh: true,
    } = first
    else {
        panic!("{first:?}");
    };
    let muted = allocator.request(0, time(0, 1), time(1, 4), ChannelNeed::Muted);
    let ChannelGrant::Allocated {
        channel: group,
        fresh: true,
    } = muted
    else {
        panic!("{muted:?}");
    };
    assert_eq!(
        allocator.request(0, time(1, 8), time(3, 8), ChannelNeed::Muted),
        ChannelGrant::Allocated {
            channel: group,
            fresh: false
        }
    );
    assert_eq!(
        allocator.request(1, time(1, 8), time(1, 4), exclusive),
        ChannelGrant::Fallback
    );
    assert_eq!(
        allocator.request(1, time(1, 2), time(1, 1), exclusive),
        ChannelGrant::Allocated {
            channel: channel.min(group),
            fresh: true
        }
    );
    assert_eq!(allocator.pool().free(), 1);
}

#[test]
fn trills_and_tremolo_picking_play_their_pieces_under_musical() {
    let builder = one_bar(vec![beat_with(
        beat(NoteValue::Quarter, vec![fretted(0, 0)]),
        |effects| {
            effects.tremolo_picking =
                Some(opentab_model::TremoloPicking::new(NoteValue::Sixteenth).expect("tremolo"));
        },
    )]);
    let heard = heard_musical(&builder);
    assert_eq!(heard.len(), 4);
    assert!(heard.iter().all(|note| note.channel == 0));
}

#[test]
fn musical_lists_only_what_it_cannot_play() {
    let builder = one_bar(vec![beat(
        NoteValue::Whole,
        vec![with(fretted(0, 0), |effects| {
            effects.ghost = true;
            effects.palm_mute = true;
            effects.let_ring = true;
            effects.vibrato = Some(Vibrato::Slight);
            effects.slide = Some(Slide::OutDownwards);
            effects.tap = Some(opentab_model::Tap::Right);
        })],
    )]);
    let kinds: Vec<UnplayedKind> = musical(&builder)
        .unplayed
        .iter()
        .map(|feature| feature.kind)
        .collect();
    assert_eq!(kinds, vec![UnplayedKind::Tap]);
}
