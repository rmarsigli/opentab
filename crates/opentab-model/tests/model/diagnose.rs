use opentab_model::{
    Diagnostic, Duration, ElementId, Fraction, NoteValue, Score, TimeSignature, Tuplet, diagnose,
};

use crate::support::{bar, four_four, guitar, master_bar, quarters};

fn first_voice(score: &Score) -> opentab_model::VoiceId {
    score.tracks[0].staves[0].bars[0].voices[0].id
}

#[test]
fn exactly_filled_bar_is_clean() {
    assert_eq!(diagnose(&four_four(&quarters(4))), Vec::new());
}

#[test]
fn exactly_filled_triplet_bar_is_clean() {
    let triplet = Duration::new(NoteValue::Quarter, 0, Some(Tuplet { num: 3, den: 2 })).unwrap();
    assert_eq!(diagnose(&four_four(&[triplet; 6])), Vec::new());
}

#[test]
fn incomplete_bar_is_reported() {
    let score = four_four(&quarters(3));
    assert_eq!(
        diagnose(&score),
        vec![Diagnostic::IncompleteBar {
            voice: first_voice(&score),
            filled: Fraction::new(3, 4),
            expected: Fraction::new(1, 1),
        }]
    );
}

#[test]
fn overfull_bar_is_reported() {
    let score = four_four(&quarters(5));
    assert_eq!(
        diagnose(&score),
        vec![Diagnostic::OverfullBar {
            voice: first_voice(&score),
            filled: Fraction::new(5, 4),
            expected: Fraction::new(1, 1),
        }]
    );
}

#[test]
fn empty_voice_is_not_reported() {
    assert_eq!(diagnose(&four_four(&[])), Vec::new());
}

#[test]
fn staff_with_extra_bar_is_reported() {
    let mut score = four_four(&quarters(4));
    let extra = bar(&mut score, &quarters(4));
    score.tracks[0].staves[0].bars.push(extra);
    assert_eq!(
        diagnose(&score),
        vec![Diagnostic::StaffBarCountMismatch {
            staff: score.tracks[0].staves[0].id,
            bars: 2,
            master_bars: 1,
        }]
    );
}

#[test]
fn zero_master_bars_with_bars_is_a_mismatch() {
    let mut score = four_four(&quarters(3));
    score.master_bars.clear();
    assert_eq!(
        diagnose(&score),
        vec![Diagnostic::StaffBarCountMismatch {
            staff: score.tracks[0].staves[0].id,
            bars: 1,
            master_bars: 0,
        }]
    );
}

#[test]
fn staff_with_missing_bar_is_reported() {
    let mut score = four_four(&quarters(4));
    let master = master_bar(&mut score, 3, 4);
    score.master_bars.push(master);
    assert_eq!(
        diagnose(&score),
        vec![Diagnostic::StaffBarCountMismatch {
            staff: score.tracks[0].staves[0].id,
            bars: 1,
            master_bars: 2,
        }]
    );
}

#[test]
fn zero_tracks_and_zero_master_bars_are_clean() {
    assert_eq!(diagnose(&Score::new()), Vec::new());
    let mut score = Score::new();
    let master = master_bar(&mut score, 4, 4);
    score.master_bars.push(master);
    assert_eq!(diagnose(&score), Vec::new());
}

#[test]
fn string_out_of_range_is_reported() {
    let mut score = four_four(&quarters(4));
    let beats = &mut score.tracks[0].staves[0].bars[0].voices[0].beats;
    beats[1].notes[0].kind = opentab_model::NoteKind::Fretted { string: 5, fret: 0 };
    beats[2].notes[0].kind = opentab_model::NoteKind::Fretted { string: 6, fret: 0 };
    let note = beats[2].notes[0].id;
    assert_eq!(
        diagnose(&score),
        vec![Diagnostic::StringOutOfRange {
            note,
            string: 6,
            strings: 6,
        }]
    );
}

#[test]
fn duplicate_id_is_reported_once() {
    let mut score = four_four(&quarters(4));
    let voice = first_voice(&score);
    let beats = &mut score.tracks[0].staves[0].bars[0].voices[0].beats;
    beats[0].id.0 = voice.0;
    beats[1].id.0 = voice.0;
    assert_eq!(
        diagnose(&score),
        vec![Diagnostic::DuplicateId { id: voice.0 }]
    );
}

#[test]
fn id_at_or_above_allocator_is_reported() {
    let mut score = four_four(&quarters(4));
    let next = score.ids.peek();
    let beats = &mut score.tracks[0].staves[0].bars[0].voices[0].beats;
    beats[0].notes[0].id.0 = next;
    beats[1].notes[0].id.0 = ElementId(next.0 + 100);
    assert_eq!(
        diagnose(&score),
        vec![
            Diagnostic::IdAboveAllocator { id: next },
            Diagnostic::IdAboveAllocator {
                id: ElementId(next.0 + 100)
            },
        ]
    );
}

#[test]
fn id_just_below_allocator_is_clean() {
    let mut score = four_four(&quarters(4));
    let below = ElementId(score.ids.peek().0 - 1);
    let last = score.tracks[0].staves[0].bars[0].voices[0].beats[3].id;
    assert_eq!(last.0, below);
    assert_eq!(diagnose(&score), Vec::new());
    score.ids = serde_json::from_str(&format!(r#"{{"next": {}}}"#, below.0)).unwrap();
    assert_eq!(
        diagnose(&score),
        vec![Diagnostic::IdAboveAllocator { id: below }]
    );
}

#[test]
fn diagnostics_follow_track_staff_bar_voice_beat_order() {
    let mut score = four_four(&quarters(3));
    let second_master = master_bar(&mut score, 4, 4);
    score.master_bars.push(second_master);
    let extra = bar(&mut score, &quarters(5));
    score.tracks[0].staves[0].bars.push(extra);
    let second = guitar(&mut score, vec![quarters(2), quarters(4), quarters(1)]);
    score.tracks.push(second);
    score.tracks[0].staves[0].bars[0].voices[0].beats[2].notes[0].kind =
        opentab_model::NoteKind::Fretted { string: 9, fret: 0 };
    let first = &score.tracks[0].staves[0];
    let other = &score.tracks[1].staves[0];
    let expected = vec![
        Diagnostic::StringOutOfRange {
            note: first.bars[0].voices[0].beats[2].notes[0].id,
            string: 9,
            strings: 6,
        },
        Diagnostic::IncompleteBar {
            voice: first.bars[0].voices[0].id,
            filled: Fraction::new(3, 4),
            expected: Fraction::new(1, 1),
        },
        Diagnostic::OverfullBar {
            voice: first.bars[1].voices[0].id,
            filled: Fraction::new(5, 4),
            expected: Fraction::new(1, 1),
        },
        Diagnostic::StaffBarCountMismatch {
            staff: other.id,
            bars: 3,
            master_bars: 2,
        },
        Diagnostic::IncompleteBar {
            voice: other.bars[0].voices[0].id,
            filled: Fraction::new(1, 2),
            expected: Fraction::new(1, 1),
        },
    ];
    assert_eq!(diagnose(&score), expected);
}

#[test]
fn zero_denominator_time_signature_does_not_panic() {
    let mut score = four_four(&quarters(4));
    score.master_bars[0].time_signature = TimeSignature {
        numerator: 4,
        denominator: 0,
    };
    assert_eq!(
        diagnose(&score),
        vec![Diagnostic::OverfullBar {
            voice: first_voice(&score),
            filled: Fraction::new(1, 1),
            expected: Fraction::from_integer(0),
        }]
    );
}

#[test]
fn fill_beyond_i64_is_skipped_without_panic() {
    let primes = [
        2u8, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61,
    ];
    let durations: Vec<Duration> = primes
        .iter()
        .map(|p| {
            Duration::new(
                NoteValue::OneTwentyEighth,
                3,
                Some(Tuplet { num: *p, den: 1 }),
            )
            .unwrap()
        })
        .collect();
    let score = four_four(&durations);
    let diagnostics = diagnose(&score);
    assert!(diagnostics.is_empty());
}

#[test]
fn diagnostic_serialises_with_kind_tag() {
    let diagnostic = Diagnostic::IncompleteBar {
        voice: opentab_model::VoiceId(ElementId(3)),
        filled: Fraction::new(3, 4),
        expected: Fraction::new(1, 1),
    };
    let json = serde_json::to_string(&diagnostic).unwrap();
    assert_eq!(
        json,
        r#"{"kind":"incomplete_bar","voice":3,"filled":[3,4],"expected":[1,1]}"#
    );
    let back: Diagnostic = serde_json::from_str(&json).unwrap();
    assert_eq!(back, diagnostic);
}
