use opentab_model::{
    Accent, BeatEffects, Bend, BendPoint, Dynamic, Finger, Fraction, Harmonic, HarmonicKind,
    NoteEffects, NoteValue, Slide, Stroke, StrokeDirection, StrokeTicks, Tap, TremoloPicking,
    Vibrato,
};
use proptest::option;
use proptest::prelude::*;
use proptest::sample::select;

use crate::text::arb_text_for;

const MAX_BEND_VALUE: i16 = 1200;
const MIN_STROKE_TICKS: u16 = 30;
const MAX_STROKE_TICKS: u16 = 480;
const TERMINATING_DENOMINATORS: [i64; 6] = [1, 2, 4, 5, 8, 10];

pub fn arb_dynamic() -> impl Strategy<Value = Dynamic> {
    select(vec![
        Dynamic::Ppp,
        Dynamic::Pp,
        Dynamic::P,
        Dynamic::Mp,
        Dynamic::Mf,
        Dynamic::F,
        Dynamic::Ff,
        Dynamic::Fff,
    ])
}

fn arb_stroke() -> impl Strategy<Value = Stroke> {
    (
        select(vec![StrokeDirection::Up, StrokeDirection::Down]),
        MIN_STROKE_TICKS..=MAX_STROKE_TICKS,
    )
        .prop_map(|(direction, ticks)| Stroke {
            direction,
            duration: StrokeTicks::new(ticks).expect("ticks are within range"),
        })
}

fn arb_tremolo() -> impl Strategy<Value = TremoloPicking> {
    select(vec![
        NoteValue::Eighth,
        NoteValue::Sixteenth,
        NoteValue::ThirtySecond,
    ])
    .prop_map(|value| TremoloPicking::new(value).expect("value is a tremolo value"))
}

fn arb_chord_name(gp_writable: bool) -> impl Strategy<Value = Option<String>> {
    option::of(arb_text_for(12, gp_writable))
}

pub fn arb_beat_effects(gp_writable: bool) -> impl Strategy<Value = BeatEffects> {
    (
        option::of(arb_tremolo()),
        option::of(arb_stroke()),
        arb_chord_name(gp_writable),
        option::of(arb_text_for(24, gp_writable)),
        (any::<bool>(), any::<bool>(), any::<bool>()),
        arb_dynamic(),
    )
        .prop_map(
            |(tremolo_picking, stroke, chord_name, text, (fade_in, slap, pop), dynamic)| {
                BeatEffects {
                    tremolo_picking,
                    stroke,
                    chord_name,
                    text,
                    fade_in,
                    slap,
                    pop,
                    grace: None,
                    dynamic,
                }
            },
        )
}

fn arb_harmonic_denominator(gp_writable: bool) -> BoxedStrategy<i64> {
    if gp_writable {
        select(TERMINATING_DENOMINATORS.to_vec()).boxed()
    } else {
        (1i64..=12).boxed()
    }
}

fn arb_harmonic(gp_writable: bool) -> impl Strategy<Value = Harmonic> {
    (
        select(vec![
            HarmonicKind::Natural,
            HarmonicKind::Artificial,
            HarmonicKind::Pinch,
            HarmonicKind::Tap,
            HarmonicKind::Semi,
            HarmonicKind::Feedback,
        ]),
        0i64..=48,
        arb_harmonic_denominator(gp_writable),
    )
        .prop_map(|(kind, numer, denom)| Harmonic {
            kind,
            fret: Fraction::new(numer, denom),
        })
}

fn arb_bend_point(value: i16) -> impl Strategy<Value = BendPoint> {
    (0u8..=100).prop_map(move |offset| {
        BendPoint::new(offset, value).expect("offset and value are within range")
    })
}

fn arb_bend() -> impl Strategy<Value = Bend> {
    (-MAX_BEND_VALUE..=MAX_BEND_VALUE).prop_flat_map(|middle| {
        (
            (-MAX_BEND_VALUE..=MAX_BEND_VALUE).prop_flat_map(arb_bend_point),
            arb_bend_point(middle),
            arb_bend_point(middle),
            (-MAX_BEND_VALUE..=MAX_BEND_VALUE).prop_flat_map(arb_bend_point),
        )
            .prop_map(|(origin, middle_start, middle_end, destination)| {
                Bend::new(origin, middle_start, middle_end, destination)
                    .expect("middle points share one value")
            })
    })
}

fn arb_finger() -> impl Strategy<Value = Finger> {
    select(vec![
        Finger::Open,
        Finger::Thumb,
        Finger::Index,
        Finger::Middle,
        Finger::Ring,
        Finger::Little,
    ])
}

fn arb_right_finger(gp_writable: bool) -> BoxedStrategy<Finger> {
    if gp_writable {
        select(vec![
            Finger::Thumb,
            Finger::Index,
            Finger::Middle,
            Finger::Ring,
            Finger::Little,
        ])
        .boxed()
    } else {
        arb_finger().boxed()
    }
}

#[derive(Debug, Clone)]
struct NoteFlags {
    tie_origin: bool,
    tie_destination: bool,
    ghost: bool,
    dead: bool,
    palm_mute: bool,
    let_ring: bool,
    staccato: bool,
    hammer_on_pull_off: bool,
}

fn arb_note_flags() -> impl Strategy<Value = NoteFlags> {
    (
        (any::<bool>(), any::<bool>(), any::<bool>(), any::<bool>()),
        (any::<bool>(), any::<bool>(), any::<bool>(), any::<bool>()),
    )
        .prop_map(
            |(
                (tie_origin, tie_destination, ghost, dead),
                (palm_mute, let_ring, staccato, hammer_on_pull_off),
            )| NoteFlags {
                tie_origin,
                tie_destination,
                ghost,
                dead,
                palm_mute,
                let_ring,
                staccato,
                hammer_on_pull_off,
            },
        )
}

pub fn arb_note_effects(gp_writable: bool) -> impl Strategy<Value = NoteEffects> {
    (
        arb_note_flags(),
        (
            option::of(select(vec![Accent::Normal, Accent::Heavy])),
            option::of(select(vec![Vibrato::Slight, Vibrato::Wide])),
            option::of(select(vec![
                Slide::Shift,
                Slide::Legato,
                Slide::IntoFromBelow,
                Slide::IntoFromAbove,
                Slide::OutDownwards,
                Slide::OutUpwards,
            ])),
        ),
        (
            option::of(arb_harmonic(gp_writable)),
            option::of(arb_bend()),
            option::of(select(vec![Tap::Right, Tap::Left])),
            (
                option::of(arb_finger()),
                option::of(arb_right_finger(gp_writable)),
            ),
        ),
    )
        .prop_map(
            |(
                flags,
                (accent, vibrato, slide),
                (harmonic, bend, tap, (left_hand_finger, right_hand_finger)),
            )| NoteEffects {
                tie_origin: flags.tie_origin,
                tie_destination: flags.tie_destination,
                ghost: flags.ghost,
                dead: flags.dead,
                accent,
                palm_mute: flags.palm_mute,
                let_ring: flags.let_ring,
                staccato: flags.staccato,
                vibrato,
                hammer_on_pull_off: flags.hammer_on_pull_off,
                slide,
                harmonic,
                bend,
                tap,
                left_hand_finger,
                right_hand_finger,
            },
        )
}
