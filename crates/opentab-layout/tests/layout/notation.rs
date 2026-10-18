use std::collections::BTreeMap;

use opentab_layout::rhythm::CUE_SIZE;
use opentab_layout::{
    DisplayList, GroupId, PathSegment, PathStyle, Primitive, STAFF_SPACE, StaffKind, StaffMode,
    Units, layout,
};
use opentab_model::{Grace, HarmonicKind, InstrumentKind, NoteKind, NoteValue, Pitch, Score};
use opentab_testkit::arb_score;
use proptest::prelude::*;

use crate::music::{
    Event, beat_id, build, glyphs, group_in, half_spaces, head_of, is_notehead, marks, measure,
    natural_midi, note_at, note_glyphs, note_id, options, overlaps, own_lines, plain, play,
    points_up, render, standard_rows, stem_of, tall, top_step, tuplet,
};
use crate::support::{dump, systems};

const G4: (u8, u8) = (2, 0);
const A4: (u8, u8) = (2, 2);
const B4: (u8, u8) = (1, 0);
const E5: (u8, u8) = (0, 0);
const E4: (u8, u8) = (3, 2);
const D4: (u8, u8) = (3, 0);
const G5: (u8, u8) = (0, 3);
const F4: (u8, u8) = (3, 3);
const F_SHARP_4: (u8, u8) = (3, 4);
const C5: (u8, u8) = (1, 1);
const C_SHARP_5: (u8, u8) = (1, 2);
const D7: (u8, u8) = (0, 22);
const G2: (u8, u8) = (5, 3);
const LOW_STRING: Pitch = Pitch(28);
const CASES: u32 = 256;

fn quarters(notes: &[(u8, u8)]) -> Vec<Event> {
    notes
        .iter()
        .map(|note| play(NoteValue::Quarter, &[*note]))
        .collect()
}

fn accidentals(list: &DisplayList, score: &Score, bar: usize) -> Vec<Vec<&'static str>> {
    let count = score.tracks[0].staves[0].bars[bar].voices[0].beats.len();
    (0..count)
        .map(|beat| {
            note_glyphs(list, note_id(score, bar, 0, beat, 0))
                .into_iter()
                .filter(|name| name.starts_with("accidental"))
                .collect()
        })
        .collect()
}

#[test]
fn accidentals_follow_the_key_carry_through_the_bar_and_reset_at_the_barline() {
    let mut score = build(&[
        measure(
            (4, 4),
            1,
            vec![(0, quarters(&[F_SHARP_4, F4, F4, F_SHARP_4]))],
        ),
        measure(
            (4, 4),
            1,
            vec![(0, quarters(&[F4, C_SHARP_5, G4, C_SHARP_5]))],
        ),
        measure(
            (4, 4),
            1,
            vec![(0, quarters(&[C_SHARP_5, C_SHARP_5, C5, G4]))],
        ),
    ]);
    note_at(&mut score, 1, 0, 3, 0).effects.tie_origin = true;
    note_at(&mut score, 2, 0, 0, 0).effects.tie_destination = true;
    let list = render(&score, &options(StaffMode::Standard));
    let natural = vec!["accidentalNatural"];
    let sharp = vec!["accidentalSharp"];
    let none: Vec<&str> = Vec::new();
    assert_eq!(
        accidentals(&list, &score, 0),
        vec![none.clone(), natural.clone(), none.clone(), sharp.clone()]
    );
    assert_eq!(
        accidentals(&list, &score, 1),
        vec![natural.clone(), sharp.clone(), none.clone(), none.clone()]
    );
    assert_eq!(
        accidentals(&list, &score, 2),
        vec![none.clone(), sharp, natural, none]
    );
}

#[test]
fn flat_keys_spell_with_flats_and_sharp_keys_with_sharps() {
    let score = build(&[
        measure((2, 4), -2, vec![(0, quarters(&[(2, 1), (2, 3)]))]),
        measure((2, 4), 2, vec![(0, quarters(&[(2, 1), (2, 3)]))]),
    ]);
    let list = render(&score, &options(StaffMode::Standard));
    let top = systems(&list)[0].staves[0].top;
    let position = |bar: usize, beat: usize| {
        half_spaces(
            head_of(&list, note_id(&score, bar, 0, beat, 0)).origin.y,
            top,
        )
    };
    assert_eq!(
        accidentals(&list, &score, 0),
        vec![vec!["accidentalFlat"], vec![]]
    );
    assert_eq!(
        accidentals(&list, &score, 1),
        vec![vec!["accidentalSharp"], vec!["accidentalSharp"]]
    );
    assert_eq!((position(0, 0), position(0, 1)), (5, 4));
    assert_eq!((position(1, 0), position(1, 1)), (6, 5));
}

fn beam_count(list: &DisplayList, score: &Score, bar: usize) -> usize {
    let id = GroupId(score.tracks[0].staves[0].bars[bar].id.0.0);
    group_in(list, id)
        .items
        .iter()
        .filter(|item| match item {
            Primitive::Shape(shape) => {
                shape.style == PathStyle::Fill
                    && shape.segments.len() == 5
                    && matches!(shape.segments[1], PathSegment::LineTo(_))
            }
            _ => false,
        })
        .count()
}

#[test]
fn eighths_are_beamed_by_the_beat_of_the_time_signature() {
    let eighths = |count: usize| vec![play(NoteValue::Eighth, &[G4]); count];
    let score = build(&[
        measure((4, 4), 0, vec![(0, eighths(8))]),
        measure((6, 8), 0, vec![(0, eighths(6))]),
        measure((7, 8), 0, vec![(0, eighths(7))]),
        measure(
            (3, 4),
            0,
            vec![(
                0,
                vec![
                    play(NoteValue::Eighth, &[G4]),
                    Event::Rest(plain(NoteValue::Eighth)),
                    play(NoteValue::Eighth, &[G4]),
                    play(NoteValue::Eighth, &[G4]),
                    play(NoteValue::Quarter, &[G4]),
                ],
            )],
        ),
    ]);
    let list = render(&score, &options(StaffMode::Standard));
    assert_eq!(beam_count(&list, &score, 0), 4);
    assert_eq!(beam_count(&list, &score, 1), 2);
    assert_eq!(beam_count(&list, &score, 2), 3);
    assert_eq!(beam_count(&list, &score, 3), 1);
    let flags = |bar: usize, beat: usize| {
        crate::music::named(&glyphs(
            &group_in(&list, GroupId(beat_id(&score, bar, 0, beat).0.0)).items,
        ))
        .contains(&"flag8thUp")
    };
    assert!(flags(3, 0));
    assert!(!flags(3, 2));
    assert!(!flags(0, 0));
}

#[test]
fn a_single_voice_stems_away_from_its_farthest_note_and_ties_point_down() {
    let score = build(&[measure(
        (4, 4),
        0,
        vec![(
            0,
            vec![
                play(NoteValue::Quarter, &[G4]),
                play(NoteValue::Quarter, &[B4]),
                play(NoteValue::Quarter, &[E5]),
                play(NoteValue::Quarter, &[D4, G5]),
            ],
        )],
    )]);
    let list = render(&score, &options(StaffMode::Standard));
    let up: Vec<bool> = (0..4)
        .map(|beat| points_up(&stem_of(&list, beat_id(&score, 0, 0, beat))))
        .collect();
    assert_eq!(up, vec![true, false, false, false]);
}

#[test]
fn with_two_voices_the_first_stems_up_and_the_second_down() {
    let score = build(&[measure(
        (2, 4),
        0,
        vec![(0, quarters(&[E5, E5])), (1, quarters(&[E4, E4]))],
    )]);
    let list = render(&score, &options(StaffMode::Standard));
    for beat in 0..2 {
        assert!(points_up(&stem_of(&list, beat_id(&score, 0, 0, beat))));
        assert!(!points_up(&stem_of(&list, beat_id(&score, 0, 1, beat))));
    }
    let crossing = build(&[measure(
        (1, 4),
        0,
        vec![(0, quarters(&[G4])), (1, quarters(&[A4]))],
    )]);
    let list = render(&crossing, &options(StaffMode::Standard));
    let upper = head_of(&list, note_id(&crossing, 0, 0, 0, 0));
    let lower = head_of(&list, note_id(&crossing, 0, 1, 0, 0));
    assert!(upper.origin.x > lower.origin.x);
}

#[test]
fn grace_notes_are_small_slashed_before_the_beat_and_plain_on_it() {
    let score = build(&[measure(
        (2, 4),
        0,
        vec![(
            0,
            vec![
                Event::Grace(Grace::BeforeBeat, NoteValue::Eighth, vec![G4]),
                play(NoteValue::Quarter, &[A4]),
                Event::Grace(Grace::OnBeat, NoteValue::Eighth, vec![(3, 4)]),
                play(NoteValue::Quarter, &[A4]),
            ],
        )],
    )]);
    let list = render(&score, &options(StaffMode::Standard));
    for (grace, main) in [(0, 1), (2, 3)] {
        let small = head_of(&list, note_id(&score, 0, 0, grace, 0));
        let large = head_of(&list, note_id(&score, 0, 0, main, 0));
        assert_eq!(small.staff_space, CUE_SIZE);
        assert_eq!(large.staff_space, STAFF_SPACE);
        assert!(small.origin.x + Units(800) < large.origin.x);
    }
    let slashes = |beat: usize| {
        own_lines(&group_in(&list, GroupId(beat_id(&score, 0, 0, beat).0.0)).items)
            .into_iter()
            .filter(|line| line.from.x != line.to.x && line.from.y != line.to.y)
            .count()
    };
    assert_eq!((slashes(0), slashes(2)), (1, 0));
    let sharp = note_glyphs(&list, note_id(&score, 0, 0, 2, 0));
    assert!(sharp.contains(&"accidentalSharp"));
}

#[test]
fn tuplets_get_a_bracket_and_their_number() {
    let triplet = Event::Play(tuplet(NoteValue::Eighth, 3, 2), vec![G4]);
    let quintuplet = Event::Play(tuplet(NoteValue::Sixteenth, 5, 4), vec![G4]);
    let mut events = vec![triplet; 3];
    events.push(play(NoteValue::Quarter, &[G4]));
    events.extend(vec![quintuplet; 5]);
    events.push(play(NoteValue::Quarter, &[G4]));
    let score = build(&[measure((4, 4), 0, vec![(0, events)])]);
    let list = render(&score, &options(StaffMode::Standard));
    let bar = group_in(&list, GroupId(score.tracks[0].staves[0].bars[0].id.0.0));
    let numbers: Vec<_> = bar
        .items
        .iter()
        .filter_map(|item| match item {
            Primitive::Glyph(glyph) => Some(crate::support::glyph_name(glyph.codepoint)),
            _ => None,
        })
        .filter(|name| name.starts_with("tuplet"))
        .collect();
    assert_eq!(numbers, vec!["tuplet3", "tuplet5"]);
    assert_eq!(beam_count(&list, &score, 0), 3);
    let top = systems(&list)[0].staves[0].top;
    let brackets = own_lines(&bar.items)
        .into_iter()
        .filter(|line| line.from.y.max(line.to.y) < top)
        .count();
    assert_eq!(brackets, 8);
    let highest = (0..3)
        .map(|beat| head_of(&list, note_id(&score, 0, 0, beat, 0)).origin.y)
        .min()
        .unwrap();
    let number = glyphs(&bar.items)
        .into_iter()
        .find(|glyph| crate::support::glyph_name(glyph.codepoint) == "tuplet3")
        .unwrap();
    assert!(number.origin.y < highest - Units(STAFF_SPACE.0 * 3));
}

fn tie_shapes(items: &[Primitive]) -> Vec<(i64, i64)> {
    items
        .iter()
        .filter_map(|item| match item {
            Primitive::Shape(shape)
                if matches!(shape.segments.get(1), Some(PathSegment::CubicTo(..))) =>
            {
                match (shape.segments.first(), shape.segments.get(1)) {
                    (Some(PathSegment::MoveTo(start)), Some(PathSegment::CubicTo(_, _, end))) => {
                        Some((start.x.0, end.x.0))
                    }
                    _ => None,
                }
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_tie_across_a_system_break_draws_two_halves() {
    let bars: Vec<_> = (0..24)
        .map(|_| measure((4, 4), 0, vec![(0, quarters(&[G4, A4, B4, G4]))]))
        .collect();
    let mut score = build(&bars);
    let plain_list = render(&score, &options(StaffMode::Standard));
    let first = systems(&plain_list)[0];
    let last = first.staves[0].bars.last().unwrap().index;
    note_at(&mut score, last, 0, 3, 0).effects.tie_origin = true;
    note_at(&mut score, last + 1, 0, 0, 0)
        .effects
        .tie_destination = true;
    note_at(&mut score, 0, 0, 3, 0).effects.tie_origin = true;
    note_at(&mut score, 1, 0, 0, 0).effects.tie_destination = true;
    let list = render(&score, &options(StaffMode::Standard));
    let all = systems(&list);
    let bar_items = |index: usize| {
        &group_in(&list, GroupId(score.tracks[0].staves[0].bars[index].id.0.0)).items
    };
    let outgoing = tie_shapes(bar_items(last));
    assert_eq!(outgoing.len(), 1);
    assert_eq!(outgoing[0].1, all[0].rect.max.x.0 - 256);
    let incoming = tie_shapes(bar_items(last + 1));
    assert_eq!(incoming.len(), 1);
    assert_eq!(incoming[0].0, all[1].staves[0].bars[0].content.0);
    let inside = tie_shapes(bar_items(1));
    assert_eq!(inside.len(), 1);
    let from = head_of(&list, note_id(&score, 0, 0, 3, 0));
    let to = head_of(&list, note_id(&score, 1, 0, 0, 0));
    assert!(inside[0].0 > from.origin.x.0 && inside[0].1 < to.origin.x.0);
}

fn rest_y(list: &DisplayList, score: &Score, voice: usize, beat: usize) -> (i64, i64) {
    let found = glyphs(&group_in(list, GroupId(beat_id(score, 0, voice, beat).0.0)).items)
        .into_iter()
        .find(|glyph| crate::support::glyph_name(glyph.codepoint).starts_with("rest"))
        .unwrap();
    (found.origin.x.0, found.origin.y.0)
}

#[test]
fn rests_rise_for_the_first_voice_and_fall_for_the_second() {
    let rest = Event::Rest(plain(NoteValue::Quarter));
    let shared = build(&[measure(
        (2, 4),
        0,
        vec![
            (0, vec![rest.clone(), play(NoteValue::Quarter, &[E5])]),
            (1, vec![rest.clone(), play(NoteValue::Quarter, &[E4])]),
        ],
    )]);
    let list = render(&shared, &options(StaffMode::Standard));
    let middle = systems(&list)[0].staves[0].middle().0;
    assert!(rest_y(&list, &shared, 0, 0).1 < middle);
    assert!(rest_y(&list, &shared, 1, 0).1 > middle);
    let alone = build(&[
        measure(
            (2, 4),
            0,
            vec![(0, vec![rest, play(NoteValue::Quarter, &[E5])])],
        ),
        measure(
            (3, 4),
            0,
            vec![(
                0,
                vec![Event::Rest(crate::music::dotted(NoteValue::Half, 1))],
            )],
        ),
    ]);
    let list = render(&alone, &options(StaffMode::Standard));
    let staff = &systems(&list)[0].staves[0];
    assert_eq!(rest_y(&list, &alone, 0, 0).1, staff.middle().0);
    let whole = glyphs(
        &group_in(
            &list,
            GroupId(alone.tracks[0].staves[0].bars[1].voices[0].beats[0].id.0.0),
        )
        .items,
    );
    assert_eq!(crate::music::named(&whole), vec!["restWhole"]);
    let bar = &staff.bars[1];
    let width = opentab_layout::glyphs::Metrics::bravura()
        .bbox("restWhole")
        .unwrap()
        .max
        .x;
    let centre = whole[0].origin.x + width.half();
    let expected = bar.content + (bar.rect.max.x - bar.content).half();
    assert!((centre - expected).0.abs() <= 1);
    assert_eq!(whole[0].origin.y, staff.line_y(1));
}

fn ledger_ys(list: &DisplayList, id: opentab_model::BeatId) -> Vec<i64> {
    let mut ys: Vec<i64> = own_lines(&group_in(list, GroupId(id.0.0)).items)
        .into_iter()
        .filter(|line| line.from.y == line.to.y)
        .map(|line| line.from.y.0)
        .collect();
    ys.sort_unstable();
    ys
}

#[test]
fn notes_six_ledger_lines_out_get_their_lines_and_push_the_system_open() {
    let inside = build(&[measure((2, 4), 0, vec![(0, quarters(&[G4, G4]))])]);
    let mut outside = build(&[measure((2, 4), 0, vec![(0, quarters(&[D7, G2]))])]);
    outside.tracks[0].tuning.strings[5] = LOW_STRING;
    let options = options(StaffMode::Both);
    let near = render(&inside, &options);
    let far = render(&outside, &options);
    let near_system = systems(&near)[0];
    let far_system = systems(&far)[0];
    let top = far_system.staves[0].top.0;
    let bottom = far_system.staves[0].bottom().0;
    let above: Vec<i64> = (1..=6).rev().map(|step| top - 1024 * step).collect();
    let below: Vec<i64> = (1..=6).map(|step| bottom + 1024 * step).collect();
    assert_eq!(ledger_ys(&far, beat_id(&outside, 0, 0, 0)), above);
    assert_eq!(ledger_ys(&far, beat_id(&outside, 0, 0, 1)), below);
    let high = head_of(&far, note_id(&outside, 0, 0, 0, 0));
    let low = head_of(&far, note_id(&outside, 0, 0, 1, 0));
    assert_eq!(high.origin.y.0, top - 1024 * 6);
    assert_eq!(low.origin.y.0, bottom + 1024 * 6);
    assert!(far_system.rect.height() > near_system.rect.height() + Units(STAFF_SPACE.0 * 4));
    let lift = far_system.staves[0].top - far_system.rect.min.y;
    let near_lift = near_system.staves[0].top - near_system.rect.min.y;
    assert!(lift > near_lift + Units(STAFF_SPACE.0 * 2));
    let gap = far_system.staves[1].top - far_system.staves[0].bottom();
    let near_gap = near_system.staves[1].top - near_system.staves[0].bottom();
    assert!(gap > near_gap + Units(STAFF_SPACE.0 * 2));
    assert!(high.origin.y - Units(STAFF_SPACE.0 / 2) > far_system.rect.min.y);
    assert!(lift <= near_lift + Units(STAFF_SPACE.0 * 4));
}

fn focused(score: &Score) -> String {
    let list = render(score, &options(StaffMode::Standard));
    dump(&list)
}

fn rhythm_score() -> Score {
    let rest = |value| Event::Rest(plain(value));
    build(&[
        measure((4, 4), 0, vec![(0, vec![play(NoteValue::Whole, &[G4])])]),
        measure(
            (4, 4),
            0,
            vec![(
                0,
                vec![
                    play(NoteValue::Half, &[G4]),
                    Event::Play(crate::music::dotted(NoteValue::Quarter, 1), vec![A4]),
                    play(NoteValue::Eighth, &[B4]),
                ],
            )],
        ),
        measure(
            (4, 4),
            0,
            vec![(
                0,
                vec![
                    play(NoteValue::Eighth, &[E4]),
                    play(NoteValue::Sixteenth, &[G4]),
                    play(NoteValue::Sixteenth, &[E5]),
                    play(NoteValue::ThirtySecond, &[B4]),
                    play(NoteValue::ThirtySecond, &[A4]),
                    play(NoteValue::Sixteenth, &[G4]),
                    play(NoteValue::Eighth, &[E4]),
                    rest(NoteValue::Quarter),
                    play(NoteValue::Quarter, &[E5]),
                ],
            )],
        ),
        measure(
            (4, 4),
            0,
            vec![(
                0,
                vec![
                    Event::Play(tuplet(NoteValue::Eighth, 3, 2), vec![G4]),
                    Event::Play(tuplet(NoteValue::Eighth, 3, 2), vec![A4]),
                    Event::Play(tuplet(NoteValue::Eighth, 3, 2), vec![B4]),
                    rest(NoteValue::Eighth),
                    play(NoteValue::SixtyFourth, &[G4]),
                ],
            )],
        ),
    ])
}

#[test]
fn rhythm_notation_matches_the_snapshot() {
    insta::assert_snapshot!(focused(&rhythm_score()));
}

fn pitch_for_export() -> Score {
    let mut score = build(&[
        measure(
            (4, 4),
            -3,
            vec![(0, quarters(&[(2, 1), (2, 2), (2, 1), (1, 0)]))],
        ),
        measure(
            (4, 4),
            4,
            vec![(
                0,
                vec![
                    play(NoteValue::Quarter, &[G4, A4, B4]),
                    play(NoteValue::Quarter, &[F4, F_SHARP_4]),
                    play(NoteValue::Half, &[D7]),
                ],
            )],
        ),
        measure(
            (4, 4),
            0,
            vec![(
                0,
                vec![
                    play(NoteValue::Half, &[G2, E4]),
                    play(NoteValue::Half, &[(3, 1), (2, 1), (1, 1), (0, 1)]),
                ],
            )],
        ),
    ]);
    score.tracks[0].tuning.strings[5] = LOW_STRING;
    score
}

#[test]
fn pitch_notation_matches_the_snapshot() {
    insta::assert_snapshot!(focused(&pitch_for_export()));
}

fn effects_for_export() -> Score {
    let mut score = build(&[
        measure(
            (4, 4),
            0,
            vec![(
                0,
                vec![
                    Event::Grace(Grace::BeforeBeat, NoteValue::Eighth, vec![(3, 4)]),
                    play(NoteValue::Quarter, &[G4]),
                    Event::Grace(Grace::OnBeat, NoteValue::Sixteenth, vec![A4]),
                    play(NoteValue::Quarter, &[G4]),
                    play(NoteValue::Quarter, &[B4]),
                    play(NoteValue::Quarter, &[E5]),
                ],
            )],
        ),
        measure(
            (4, 4),
            0,
            vec![
                (0, quarters(&[E5, E5, B4, G4])),
                (
                    1,
                    vec![
                        play(NoteValue::Half, &[E4]),
                        Event::Rest(plain(NoteValue::Half)),
                    ],
                ),
            ],
        ),
        measure((4, 4), 0, vec![(0, quarters(&[G4, G4, A4, A4]))]),
    ]);
    note_at(&mut score, 0, 0, 3, 0).effects.dead = true;
    note_at(&mut score, 0, 0, 4, 0).effects.ghost = true;
    note_at(&mut score, 0, 0, 5, 0).effects.harmonic = Some(opentab_model::Harmonic {
        kind: opentab_model::HarmonicKind::Natural,
        fret: opentab_model::Fraction::new(12, 1),
    });
    note_at(&mut score, 1, 0, 0, 0).effects.accent = Some(opentab_model::Accent::Normal);
    note_at(&mut score, 1, 0, 1, 0).effects.accent = Some(opentab_model::Accent::Heavy);
    note_at(&mut score, 1, 0, 2, 0).effects.staccato = true;
    note_at(&mut score, 1, 0, 3, 0).effects.tie_origin = true;
    note_at(&mut score, 2, 0, 0, 0).effects.tie_destination = true;
    note_at(&mut score, 2, 0, 0, 0).effects.tie_origin = true;
    note_at(&mut score, 2, 0, 1, 0).effects.tie_destination = true;
    score
}

#[test]
fn note_effects_and_voices_match_the_snapshot() {
    insta::assert_snapshot!(focused(&effects_for_export()));
}

fn expected_notes(
    score: &Score,
    track: usize,
    staff: usize,
    bar: usize,
) -> BTreeMap<u64, Option<i64>> {
    let track = &score.tracks[track];
    let mut out = BTreeMap::new();
    if track.instrument.kind != InstrumentKind::Fretted {
        return out;
    }
    let Some(model) = track.staves[staff].bars.get(bar) else {
        return out;
    };
    for voice in &model.voices {
        for beat in &voice.beats {
            for note in &beat.notes {
                if !matches!(note.kind, NoteKind::Fretted { .. }) {
                    continue;
                }
                let natural = note
                    .effects
                    .harmonic
                    .is_some_and(|harmonic| harmonic.kind == HarmonicKind::Natural);
                if let Some(pitch) = track.pitch_of(note) {
                    if natural {
                        out.insert(note.id.0.0, None);
                        continue;
                    }
                    let written = i64::from(pitch.0)
                        - i64::from(track.transpose.chromatic())
                        - 12 * i64::from(track.transpose.octave());
                    out.insert(note.id.0.0, Some(written));
                }
            }
        }
    }
    out
}

fn check_notation(score: &Score, list: &DisplayList) {
    for (_, _, row) in standard_rows(list) {
        for (bar, group) in row.staff.bars.iter().zip(&row.bars) {
            let expected = expected_notes(score, row.staff.track, row.staff.staff, bar.index);
            let found = marks(group);
            let heads: Vec<_> = found.iter().filter(|mark| mark.head).collect();
            let mut per_note: BTreeMap<u64, usize> = BTreeMap::new();
            for head in &heads {
                *per_note.entry(head.note.0).or_insert(0) += 1;
            }
            let wanted: BTreeMap<u64, usize> = expected.keys().map(|id| (*id, 1)).collect();
            assert_eq!(per_note, wanted, "noteheads of bar {}", bar.index);
            let total = glyphs(&group.items)
                .iter()
                .filter(|glyph| is_notehead(glyph))
                .count();
            assert_eq!(
                total,
                expected.len(),
                "stray noteheads in bar {}",
                bar.index
            );
            for (id, written) in expected
                .iter()
                .filter_map(|(id, written)| Some((id, (*written)?)))
            {
                let head = glyphs(&group_in(list, GroupId(*id)).items)
                    .into_iter()
                    .find(is_notehead)
                    .unwrap();
                let position = half_spaces(head.origin.y, row.staff.top);
                let step = top_step(bar.clef) - position;
                assert!(
                    (written - natural_midi(step)).abs() <= 1,
                    "note {id} written {written} drawn at step {step}"
                );
            }
            if list.crowded.contains(&bar.index) {
                continue;
            }
            for (index, first) in found.iter().enumerate() {
                for second in &found[index + 1..] {
                    let chord = first.head && second.head && first.beat == second.beat;
                    assert!(
                        chord || !overlaps(first.rect, second.rect),
                        "bar {} overlap {first:?} {second:?}",
                        bar.index
                    );
                }
            }
        }
    }
}

fn arb_mode() -> impl Strategy<Value = Option<StaffMode>> {
    proptest::option::of(prop_oneof![
        Just(StaffMode::Standard),
        Just(StaffMode::Both)
    ])
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn every_note_has_one_notehead_and_nothing_overlaps(score in arb_score(), mode in arb_mode()) {
        let options = tall(mode);
        if let Ok(list) = layout(&score, &options) {
            crate::support::check(&score, &options, &list);
            check_notation(&score, &list);
        } else {
            prop_assert!(score.master_bars.is_empty() || score.tracks.iter().all(|track| track.staves.is_empty()));
        }
    }
}

#[test]
fn every_drawn_staff_row_is_standard_or_tab() {
    let score = build(&[measure((1, 4), 0, vec![(0, quarters(&[G4]))])]);
    let list = render(&score, &options(StaffMode::Both));
    let kinds: Vec<StaffKind> = systems(&list)[0]
        .staves
        .iter()
        .map(|staff| staff.kind)
        .collect();
    assert_eq!(kinds, vec![StaffKind::Standard, StaffKind::Tab]);
    assert_eq!(standard_rows(&list).len(), 1);
}
