use opentab_model::{
    Accent, BeatEffects, Bend, BendPoint, Fraction, Harmonic, HarmonicKind, NoteEffects, NoteValue,
    Slide, Stroke, StrokeDirection, Vibrato,
};
use proptest::collection::vec;
use proptest::option;
use proptest::prelude::*;
use proptest::sample::select;

use crate::text::arb_text;

const MAX_BEND_POINTS: usize = 4;

pub fn arb_note_value() -> impl Strategy<Value = NoteValue> {
    select(vec![
        NoteValue::Whole,
        NoteValue::Half,
        NoteValue::Quarter,
        NoteValue::Eighth,
        NoteValue::Sixteenth,
        NoteValue::ThirtySecond,
        NoteValue::SixtyFourth,
        NoteValue::OneTwentyEighth,
    ])
}

fn arb_stroke() -> impl Strategy<Value = Stroke> {
    (
        select(vec![StrokeDirection::Up, StrokeDirection::Down]),
        arb_note_value(),
    )
        .prop_map(|(direction, duration)| Stroke {
            direction,
            duration,
        })
}

pub fn arb_beat_effects() -> impl Strategy<Value = BeatEffects> {
    (
        option::of(arb_note_value()),
        option::of(arb_stroke()),
        option::of(arb_text(12)),
        option::of(arb_text(24)),
        (any::<bool>(), any::<bool>(), any::<bool>(), any::<bool>()),
    )
        .prop_map(
            |(tremolo_picking, stroke, chord_name, text, (fade_in, tap, slap, pop))| BeatEffects {
                tremolo_picking,
                stroke,
                chord_name,
                text,
                fade_in,
                tap,
                slap,
                pop,
            },
        )
}

fn arb_harmonic() -> impl Strategy<Value = Harmonic> {
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
        1i64..=12,
    )
        .prop_map(|(kind, numer, denom)| Harmonic {
            kind,
            fret: Fraction::new(numer, denom),
        })
}

fn arb_bend() -> impl Strategy<Value = Bend> {
    vec(
        (0u8..=100, -48i16..=48).prop_map(|(offset, value)| BendPoint { offset, value }),
        0..=MAX_BEND_POINTS,
    )
    .prop_map(|points| Bend { points })
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

pub fn arb_note_effects() -> impl Strategy<Value = NoteEffects> {
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
            option::of(arb_harmonic()),
            option::of(arb_bend()),
            option::of(any::<u8>()),
            option::of(any::<u8>()),
        ),
    )
        .prop_map(
            |(
                flags,
                (accent, vibrato, slide),
                (harmonic, bend, left_hand_finger, right_hand_finger),
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
                left_hand_finger,
                right_hand_finger,
            },
        )
}
