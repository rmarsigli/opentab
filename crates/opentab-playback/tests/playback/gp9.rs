use opentab_model::{
    Accent, Bend, BendPoint, Dynamic, Fraction, Harmonic, HarmonicKind, NoteValue, TempoUnit,
};
use opentab_playback::{
    BEND_CENTRE, ConductorKind, MidiMessage, Performance, PlaybackOptions, Profile, ScoreTime,
    UnplayedKind, export_midi, perform,
};

use crate::build::{Builder, NoteSpec, beat, beat_with, fretted, with};
use crate::smf::{SmfKind, read_smf};
use crate::support::{frac, numbers, score_from, tempo};

const GP: PlaybackOptions = PlaybackOptions {
    profile: Profile::Gp,
};
const TICKS_PER_WHOLE: i64 = 1920;
const MICROS_PER_MINUTE: i64 = 60_000_000;
const BEND_REACH: i32 = 8191;
const RANGE_UNITS: i32 = 300;
const MIDDLE_START: u8 = 20;
const MIDDLE_END: u8 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Exported {
    Silent,
    Line,
    Peak,
}

#[derive(Debug, Clone, Copy)]
struct BendCase {
    origin: i16,
    middle: i16,
    destination: i16,
    exported: Exported,
}

const fn bend_case(origin: i16, middle: i16, destination: i16, exported: Exported) -> BendCase {
    BendCase {
        origin,
        middle,
        destination,
        exported,
    }
}

const M3_GP9_01_GRID: [BendCase; 36] = [
    bend_case(0, -100, 0, Exported::Silent),
    bend_case(0, -100, 100, Exported::Silent),
    bend_case(0, -100, 200, Exported::Silent),
    bend_case(0, 0, 0, Exported::Line),
    bend_case(0, 0, 100, Exported::Line),
    bend_case(0, 0, 200, Exported::Line),
    bend_case(0, 100, 0, Exported::Peak),
    bend_case(0, 100, 100, Exported::Line),
    bend_case(0, 100, 200, Exported::Line),
    bend_case(0, 200, 0, Exported::Peak),
    bend_case(0, 200, 100, Exported::Peak),
    bend_case(0, 200, 200, Exported::Line),
    bend_case(100, -100, 0, Exported::Silent),
    bend_case(100, -100, 100, Exported::Silent),
    bend_case(100, -100, 200, Exported::Silent),
    bend_case(100, 0, 0, Exported::Line),
    bend_case(100, 0, 100, Exported::Silent),
    bend_case(100, 0, 200, Exported::Silent),
    bend_case(100, 100, 0, Exported::Line),
    bend_case(100, 100, 100, Exported::Line),
    bend_case(100, 100, 200, Exported::Line),
    bend_case(100, 200, 0, Exported::Peak),
    bend_case(100, 200, 100, Exported::Peak),
    bend_case(100, 200, 200, Exported::Line),
    bend_case(200, -100, 0, Exported::Silent),
    bend_case(200, -100, 100, Exported::Silent),
    bend_case(200, -100, 200, Exported::Silent),
    bend_case(200, 0, 0, Exported::Line),
    bend_case(200, 0, 100, Exported::Silent),
    bend_case(200, 0, 200, Exported::Silent),
    bend_case(200, 100, 0, Exported::Line),
    bend_case(200, 100, 100, Exported::Line),
    bend_case(200, 100, 200, Exported::Silent),
    bend_case(200, 200, 0, Exported::Line),
    bend_case(200, 200, 100, Exported::Line),
    bend_case(200, 200, 200, Exported::Line),
];

const M3_GP9_01_BELOW: [BendCase; 4] = [
    bend_case(-100, 200, 200, Exported::Line),
    bend_case(0, 200, -100, Exported::Peak),
    bend_case(-200, -100, -200, Exported::Silent),
    bend_case(0, -100, -100, Exported::Silent),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BendAt {
    at: Fraction,
    value: u16,
}

fn value_of(cents: i16) -> u16 {
    let units = i32::from(cents.max(0)) / 2;
    u16::try_from(i32::from(BEND_CENTRE) + units * BEND_REACH / RANGE_UNITS).expect("bend")
}

fn whole_note(note: NoteSpec) -> Performance {
    let mut builder = Builder::new(1);
    let track = builder.guitar();
    builder.voice(track, 0, 0, vec![beat(NoteValue::Whole, vec![note])]);
    perform(&builder.score, &GP).expect("perform")
}

fn bent(case: BendCase) -> Performance {
    let point = |offset: u8, value: i16| BendPoint::new(offset, value).expect("point");
    let bend = Bend::new(
        point(0, case.origin),
        point(MIDDLE_START, case.middle),
        point(MIDDLE_END, case.middle),
        point(100, case.destination),
    )
    .expect("bend");
    whole_note(with(fretted(1, 5), |effects| effects.bend = Some(bend)))
}

fn bends(performance: &Performance) -> Vec<BendAt> {
    performance
        .events
        .iter()
        .filter_map(|event| match event.message {
            MidiMessage::PitchBend { value } => Some(BendAt {
                at: event.at.whole_notes(),
                value,
            }),
            _ => None,
        })
        .collect()
}

fn monotonic(values: &[u16]) -> bool {
    values.windows(2).all(|pair| pair[0] <= pair[1])
        || values.windows(2).all(|pair| pair[0] >= pair[1])
}

fn check_bend(case: BendCase) {
    let performance = bent(case);
    let events = bends(&performance);
    let label = format!("{case:?}");
    let dip = performance
        .unplayed
        .iter()
        .any(|feature| feature.kind == UnplayedKind::DipBend);
    assert_eq!(dip, case.exported == Exported::Silent, "{label}");
    if case.exported == Exported::Silent {
        assert!(events.is_empty(), "{label}: {events:?}");
        return;
    }
    let (shape, centre) = events.split_at(events.len() - 1);
    assert_eq!(
        centre,
        [BendAt {
            at: frac(1, 1),
            value: BEND_CENTRE
        }],
        "{label}"
    );
    assert_eq!(
        shape.first(),
        Some(&BendAt {
            at: frac(0, 1),
            value: value_of(case.origin)
        }),
        "{label}"
    );
    assert_eq!(
        shape.last(),
        Some(&BendAt {
            at: frac(1, 1),
            value: value_of(case.destination)
        }),
        "{label}"
    );
    let start = frac(i64::from(MIDDLE_START), 100);
    let end = frac(i64::from(MIDDLE_END), 100);
    let inside = shape
        .iter()
        .filter(|event| event.at > start && event.at < end)
        .count();
    let values: Vec<u16> = shape.iter().map(|event| event.value).collect();
    match case.exported {
        Exported::Peak => {
            assert_eq!(
                inside, 0,
                "{label}: the peak holds between the middle points"
            );
            assert!(
                shape.contains(&BendAt {
                    at: end,
                    value: value_of(case.middle)
                }),
                "{label}"
            );
            assert_eq!(values.iter().max(), Some(&value_of(case.middle)), "{label}");
        }
        Exported::Line => {
            assert!(monotonic(&values), "{label}: {values:?}");
            if case.origin.max(0) != case.destination.max(0) {
                assert!(inside > 0, "{label}: a line ignores the middle offsets");
            }
        }
        Exported::Silent => {}
    }
}

#[test]
fn m3_gp9_01_bends_clamp_their_ends_drop_dips_and_peak_above_both_ends() {
    for case in M3_GP9_01_GRID.iter().chain(&M3_GP9_01_BELOW) {
        check_bend(*case);
    }
    let fall = bends(&bent(M3_GP9_01_BELOW[1]));
    let after_peak: Vec<&BendAt> = fall
        .iter()
        .filter(|event| event.at > frac(i64::from(MIDDLE_END), 100))
        .collect();
    assert_eq!(after_peak.len(), 9 + 1, "nine fall events, then the centre");
}

#[derive(Debug, Clone, Copy)]
struct LengthCase {
    value: NoteValue,
    staccato: bool,
    accent: Option<Accent>,
    ticks: i64,
}

const fn length_case(
    value: NoteValue,
    staccato: bool,
    accent: Option<Accent>,
    ticks: i64,
) -> LengthCase {
    LengthCase {
        value,
        staccato,
        accent,
        ticks,
    }
}

const M3_GP9_02_LENGTHS: [LengthCase; 12] = [
    length_case(NoteValue::Quarter, false, None, 480),
    length_case(NoteValue::Quarter, true, None, 240),
    length_case(NoteValue::Quarter, true, Some(Accent::Normal), 480),
    length_case(NoteValue::Quarter, true, Some(Accent::Heavy), 480),
    length_case(NoteValue::Quarter, false, Some(Accent::Heavy), 480),
    length_case(NoteValue::Quarter, false, Some(Accent::Normal), 480),
    length_case(NoteValue::Eighth, true, None, 120),
    length_case(NoteValue::Eighth, true, Some(Accent::Heavy), 240),
    length_case(NoteValue::Eighth, true, Some(Accent::Normal), 240),
    length_case(NoteValue::Eighth, false, None, 240),
    length_case(NoteValue::Half, true, Some(Accent::Heavy), 960),
    length_case(NoteValue::Half, true, None, 480),
];

const M3_GP9_02_VELOCITIES: [(Dynamic, u8); 3] =
    [(Dynamic::Mf, 76), (Dynamic::F, 89), (Dynamic::P, 51)];

fn sounded(performance: &Performance) -> Vec<(i64, u8)> {
    let mut start = None;
    let mut out = Vec::new();
    for event in &performance.events {
        match event.message {
            MidiMessage::NoteOn { velocity, .. } => start = Some((event.at, velocity)),
            MidiMessage::NoteOff { .. } => {
                let (on, velocity) = start.take().expect("a note-on first");
                let length = (event.at.whole_notes() - on.whole_notes()) * TICKS_PER_WHOLE;
                out.push((length.to_integer(), velocity));
            }
            _ => {}
        }
    }
    out
}

#[test]
fn m3_gp9_02_heavy_accent_keeps_a_staccato_note_whole_and_velocity_is_the_dynamic() {
    for case in M3_GP9_02_LENGTHS {
        for (dynamic, velocity) in M3_GP9_02_VELOCITIES {
            let mut builder = Builder::new(1);
            let track = builder.guitar();
            let note = with(fretted(0, 0), |effects| {
                effects.staccato = case.staccato;
                effects.accent = case.accent;
            });
            builder.voice(
                track,
                0,
                0,
                vec![beat_with(beat(case.value, vec![note]), |effects| {
                    effects.dynamic = dynamic;
                })],
            );
            let performance = perform(&builder.score, &GP).expect("perform");
            assert_eq!(
                sounded(&performance),
                vec![(case.ticks, velocity)],
                "{case:?}"
            );
        }
    }
}

const M3_GP9_03_FRETS: [(i64, i64, u8); 17] = [
    (12, 5, 36),
    (27, 10, 34),
    (16, 5, 31),
    (4, 1, 28),
    (5, 1, 24),
    (29, 5, 34),
    (7, 1, 19),
    (41, 5, 36),
    (9, 1, 28),
    (48, 5, 34),
    (12, 1, 12),
    (147, 10, 34),
    (16, 1, 28),
    (17, 1, 36),
    (19, 1, 19),
    (217, 10, 34),
    (24, 1, 24),
];

const M3_GP9_03_FOREIGN: [(i64, i64); 18] = [
    (51, 25),
    (91, 50),
    (351, 25),
    (521, 25),
    (28, 25),
    (12001, 1000),
    (231, 100),
    (5, 2),
    (7, 2),
    (6, 1),
    (2786, 100),
    (28, 1),
    (31, 1),
    (337, 10),
    (36, 1),
    (40, 1),
    (25, 1),
    (2411, 100),
];

const HARMONIC_KINDS: [HarmonicKind; 6] = [
    HarmonicKind::Natural,
    HarmonicKind::Artificial,
    HarmonicKind::Pinch,
    HarmonicKind::Semi,
    HarmonicKind::Tap,
    HarmonicKind::Feedback,
];

const LOW_E: u8 = 40;

fn harmonic_key(kind: HarmonicKind, string: u8, fret: u8, at: Fraction) -> (u8, bool) {
    let performance = whole_note(with(fretted(string, fret), |effects| {
        effects.harmonic = Some(Harmonic { kind, fret: at });
    }));
    let key = performance
        .events
        .iter()
        .find_map(|event| match event.message {
            MidiMessage::NoteOn { key, .. } => Some(key),
            _ => None,
        })
        .expect("a note");
    let listed = performance
        .unplayed
        .iter()
        .any(|feature| feature.kind == UnplayedKind::OffNodeHarmonic);
    (key, listed)
}

#[test]
fn m3_gp9_03_harmonics_sound_only_at_guitar_pros_seventeen_frets() {
    for kind in HARMONIC_KINDS {
        let base = if kind == HarmonicKind::Natural {
            LOW_E
        } else {
            LOW_E + 3
        };
        for (numer, denom, semitones) in M3_GP9_03_FRETS {
            assert_eq!(
                harmonic_key(kind, 5, 3, Fraction::new(numer, denom)),
                (base + semitones, false),
                "{kind:?} at {numer}/{denom}"
            );
        }
        for (numer, denom) in M3_GP9_03_FOREIGN {
            assert_eq!(
                harmonic_key(kind, 5, 3, Fraction::new(numer, denom)),
                (LOW_E + 3, true),
                "{kind:?} at {numer}/{denom} plays its plain pitch"
            );
        }
    }
    let natural = |fret: u8, at: i64| harmonic_key(HarmonicKind::Natural, 1, fret, frac(at, 1)).0;
    assert_eq!((natural(3, 12), natural(3, 7)), (71, 78));
    assert_eq!(harmonic_key(HarmonicKind::Natural, 0, 0, frac(12, 1)).0, 76);
    let artificial = |at: Fraction| harmonic_key(HarmonicKind::Artificial, 4, 7, at).0;
    assert_eq!(
        (
            artificial(frac(217, 10)),
            artificial(frac(24, 1)),
            artificial(frac(17, 1))
        ),
        (86, 76, 88)
    );
}

#[derive(Debug, Clone, Copy)]
struct TempoMark {
    bar: usize,
    position: (i64, i64),
    bpm: u16,
}

#[derive(Debug, Clone, Copy)]
struct TempoCase {
    name: &'static str,
    bars: usize,
    structure: &'static str,
    marks: &'static [TempoMark],
    order: &'static str,
    events: &'static [(i64, u16)],
}

const fn mark(bar: usize, position: (i64, i64), bpm: u16) -> TempoMark {
    TempoMark { bar, position, bpm }
}

const M3_GP9_04: [TempoCase; 6] = [
    TempoCase {
        name: "04a-repeat",
        bars: 5,
        structure: "1 [S]; 3 [E3]",
        marks: &[
            mark(0, (0, 1), 120),
            mark(2, (0, 1), 90),
            mark(3, (1, 2), 150),
        ],
        order: "0 1 2 3 1 2 3 1 2 3 4",
        events: &[
            (0, 120),
            (3840, 90),
            (6720, 150),
            (7680, 120),
            (9600, 90),
            (12480, 150),
            (13440, 120),
            (15360, 90),
            (18240, 150),
        ],
    },
    TempoCase {
        name: "04b-dc",
        bars: 5,
        structure: "3 DaCapo",
        marks: &[
            mark(0, (0, 1), 100),
            mark(1, (0, 1), 140),
            mark(2, (1, 2), 80),
        ],
        order: "0 1 2 3 0 1 2 3 4",
        events: &[
            (0, 100),
            (1920, 140),
            (4800, 80),
            (7680, 100),
            (9600, 140),
            (12480, 80),
        ],
    },
    TempoCase {
        name: "04c-ds-no-tempo",
        bars: 5,
        structure: "1 Segno; 3 DaSegno",
        marks: &[
            mark(0, (0, 1), 100),
            mark(2, (0, 1), 140),
            mark(3, (1, 4), 60),
        ],
        order: "0 1 2 3 1 2 3 4",
        events: &[
            (0, 100),
            (3840, 140),
            (6240, 60),
            (7680, 100),
            (9600, 140),
            (12000, 60),
        ],
    },
    TempoCase {
        name: "04d-self-repeats",
        bars: 5,
        structure: "1 [S][E3]; 2 [S][E3]",
        marks: &[
            mark(0, (0, 1), 120),
            mark(1, (1, 2), 90),
            mark(3, (0, 1), 140),
        ],
        order: "0 1 1 1 2 2 2 3 4",
        events: &[(0, 120), (2880, 90), (4800, 90), (6720, 90), (13440, 140)],
    },
    TempoCase {
        name: "04e-two-bar-repeat",
        bars: 4,
        structure: "1 [S]; 2 [E2]",
        marks: &[mark(0, (0, 1), 120), mark(2, (1, 2), 90)],
        order: "0 1 2 1 2 3",
        events: &[(0, 120), (4800, 90), (5760, 120), (8640, 90)],
    },
    TempoCase {
        name: "04f-forward-jump",
        bars: 6,
        structure: "1 Segno; 2 DaCoda; 3 DaSegnoAlCoda; 4 Coda",
        marks: &[
            mark(0, (0, 1), 100),
            mark(2, (0, 1), 140),
            mark(3, (0, 1), 70),
        ],
        order: "0 1 2 3 1 2 4 5",
        events: &[(0, 100), (3840, 140), (5760, 70), (7680, 100), (9600, 140)],
    },
];

fn micros_of(bpm: u16) -> u32 {
    let bpm = i64::from(bpm);
    u32::try_from((2 * MICROS_PER_MINUTE + bpm) / (2 * bpm)).expect("micros")
}

fn tick_of(at: ScoreTime) -> i64 {
    (at.whole_notes() * TICKS_PER_WHOLE).to_integer()
}

#[test]
fn m3_gp9_04_tempo_is_restated_at_backward_jumps_only() {
    for case in M3_GP9_04 {
        let mut score = score_from(case.bars, case.structure);
        for mark in case.marks {
            score.master_bars[mark.bar].tempo_changes.push(tempo(
                frac(mark.position.0, mark.position.1),
                f64::from(mark.bpm),
                TempoUnit::Quarter,
                false,
            ));
        }
        let performance = perform(&score, &GP).expect("perform");
        assert_eq!(
            performance.order.sources(),
            numbers(case.order),
            "{}",
            case.name
        );
        let expected: Vec<(i64, u32)> = case
            .events
            .iter()
            .map(|(tick, bpm)| (*tick, micros_of(*bpm)))
            .collect();
        let ours: Vec<(i64, u32)> = performance
            .conductor
            .iter()
            .filter_map(|event| match event.kind {
                ConductorKind::Tempo { micros_per_quarter } => {
                    Some((tick_of(event.at), micros_per_quarter))
                }
                _ => None,
            })
            .collect();
        assert_eq!(ours, expected, "{}", case.name);
        let changes: Vec<i64> = performance
            .tempo
            .segments()
            .iter()
            .map(|segment| tick_of(segment.start))
            .collect();
        assert!(
            changes
                .iter()
                .all(|tick| expected.iter().any(|(at, _)| at == tick)),
            "{}: the held tempo changes only at an exported event: {changes:?}",
            case.name
        );
        let written: Vec<(i64, u32)> = read_smf(&export_midi(&score, Profile::Gp).expect("export"))
            .events
            .iter()
            .filter_map(|event| match event.kind {
                SmfKind::Tempo { micros } => {
                    Some((i64::try_from(event.tick).expect("tick"), micros))
                }
                _ => None,
            })
            .collect();
        assert_eq!(written, expected, "{}: the exported file", case.name);
    }
}

const M3_GP9_01_PEAK_UNEQUAL_ENDS: [((i64, i64), u16); 17] = [
    ((0, 1), 8192),
    ((3, 125), 8519),
    ((6, 125), 8847),
    ((9, 125), 9174),
    ((12, 125), 9502),
    ((3, 25), 9830),
    ((18, 125), 10157),
    ((21, 125), 10485),
    ((24, 125), 10813),
    ((1, 5), 10922),
    ((3, 5), 10922),
    ((87, 125), 10594),
    ((99, 125), 10267),
    ((111, 125), 9939),
    ((123, 125), 9611),
    ((1, 1), 9557),
    ((1, 1), 8192),
];

#[test]
fn m3_gp9_01_a_peak_with_unequal_ends_rises_holds_and_falls_event_by_event() {
    let expected: Vec<BendAt> = M3_GP9_01_PEAK_UNEQUAL_ENDS
        .iter()
        .map(|((numer, denom), value)| BendAt {
            at: frac(*numer, *denom),
            value: *value,
        })
        .collect();
    assert_eq!(
        bends(&bent(bend_case(0, 200, 100, Exported::Peak))),
        expected
    );
}

#[derive(Debug, Clone, Copy)]
struct ChordNote {
    staccato: bool,
    accent: Option<Accent>,
    ticks: i64,
}

const fn chord_note(staccato: bool, accent: Option<Accent>, ticks: i64) -> ChordNote {
    ChordNote {
        staccato,
        accent,
        ticks,
    }
}

const M3_GP9_02_CHORDS: [[ChordNote; 2]; 4] = [
    [
        chord_note(true, Some(Accent::Heavy), 480),
        chord_note(false, None, 480),
    ],
    [
        chord_note(true, None, 240),
        chord_note(false, Some(Accent::Heavy), 480),
    ],
    [
        chord_note(true, Some(Accent::Normal), 480),
        chord_note(true, None, 240),
    ],
    [
        chord_note(true, Some(Accent::Heavy), 480),
        chord_note(true, Some(Accent::Heavy), 480),
    ],
];

fn lengths_by_key(performance: &Performance) -> Vec<(u8, i64)> {
    let mut started: Vec<(u8, Fraction)> = Vec::new();
    let mut out = Vec::new();
    for event in &performance.events {
        match event.message {
            MidiMessage::NoteOn { key, .. } => started.push((key, event.at.whole_notes())),
            MidiMessage::NoteOff { key, .. } => {
                let index = started
                    .iter()
                    .position(|(candidate, _)| *candidate == key)
                    .expect("a note-on first");
                let (_, on) = started.remove(index);
                out.push((
                    key,
                    ((event.at.whole_notes() - on) * TICKS_PER_WHOLE).to_integer(),
                ));
            }
            _ => {}
        }
    }
    out.sort_unstable();
    out
}

#[test]
fn m3_gp9_02_in_a_chord_staccato_and_accent_act_per_note() {
    for chord in M3_GP9_02_CHORDS {
        let mut builder = Builder::new(1);
        let track = builder.guitar();
        let notes = chord
            .iter()
            .enumerate()
            .map(|(string, spec)| {
                with(
                    fretted(u8::try_from(string).expect("string"), 0),
                    |effects| {
                        effects.staccato = spec.staccato;
                        effects.accent = spec.accent;
                    },
                )
            })
            .collect();
        builder.voice(track, 0, 0, vec![beat(NoteValue::Quarter, notes)]);
        let performance = perform(&builder.score, &GP).expect("perform");
        let mut expected = vec![(59, chord[1].ticks), (64, chord[0].ticks)];
        expected.sort_unstable();
        assert_eq!(lengths_by_key(&performance), expected, "{chord:?}");
    }
}
