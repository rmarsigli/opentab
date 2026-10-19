use opentab_model::{
    BarId, Diagnostic, Duration, ElementId, Fraction, Instrument, InstrumentKind, NoteId, NoteKind,
    NoteValue, OpaquePayload, Score, TrackId, Tuning, Tuplet, VoiceSlot, diagnose,
};

use crate::support::{bar, beat, duration, four_four, guitar, master_bar, quarters, voice};

fn first_voice(score: &Score) -> opentab_model::VoiceId {
    score.tracks[0].staves[0].bars[0].voices[0].id
}

#[test]
fn exactly_filled_bar_is_clean() {
    assert_eq!(diagnose(&four_four(&quarters(4))), Vec::new());
}

#[test]
fn exactly_filled_triplet_bar_is_clean() {
    let triplet = Duration::new(NoteValue::Quarter, 0, Some(Tuplet::new(3, 2).unwrap())).unwrap();
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
fn id_u64_max_against_exhausted_allocator_is_above_allocator() {
    let mut score = four_four(&quarters(4));
    score.ids = serde_json::from_str(r#"{"next": 18446744073709551615}"#).unwrap();
    let max = ElementId(u64::MAX);
    score.tracks[0].staves[0].bars[0].voices[0].beats[0].notes[0]
        .id
        .0 = max;
    let found: Vec<Diagnostic> = diagnose(&score)
        .into_iter()
        .filter(|d| matches!(d, Diagnostic::IdAboveAllocator { id } if *id == max))
        .collect();
    assert_eq!(found, vec![Diagnostic::IdAboveAllocator { id: max }]);
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
fn fill_beyond_i64_is_reported_as_not_computable() {
    let primes = [
        2u8, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61,
    ];
    let durations: Vec<Duration> = primes
        .iter()
        .map(|p| {
            Duration::new(
                NoteValue::OneTwentyEighth,
                3,
                Some(Tuplet::new(*p, 1).unwrap()),
            )
            .unwrap()
        })
        .collect();
    let score = four_four(&durations);
    assert_eq!(
        diagnose(&score),
        vec![Diagnostic::FillNotComputable {
            voice: first_voice(&score)
        }]
    );
}

#[test]
fn long_voice_of_fine_durations_is_still_checked() {
    let triple_dotted = Duration::new(NoteValue::OneTwentyEighth, 3, None).unwrap();
    let score = four_four(&[triple_dotted; 16]);
    assert_eq!(
        diagnose(&score),
        vec![Diagnostic::IncompleteBar {
            voice: first_voice(&score),
            filled: Fraction::new(15, 64),
            expected: Fraction::new(1, 1),
        }]
    );
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

fn payload() -> Vec<OpaquePayload> {
    vec![OpaquePayload {
        format: "gp8:xml".to_string(),
        data: "<x/>".to_string(),
    }]
}

fn percussion_score() -> Score {
    let mut score = four_four(&quarters(4));
    let track = &mut score.tracks[0];
    track.instrument = Instrument {
        kind: InstrumentKind::Percussion,
        midi_program: 0,
        set: None,
    };
    track.tuning = Tuning {
        strings: Vec::new(),
    };
    for beat in &mut track.staves[0].bars[0].voices[0].beats {
        beat.notes[0].kind = NoteKind::Percussion { articulation: 42 };
    }
    score
}

#[test]
fn extension_on_every_kind_of_element_is_not_an_orphan() {
    let mut score = four_four(&quarters(4));
    let track = &score.tracks[0];
    let staff = &track.staves[0];
    let bar = &staff.bars[0];
    let voice = &bar.voices[0];
    let beat = &voice.beats[0];
    let ids = [
        score.master_bars[0].id.0,
        track.id.0,
        staff.id.0,
        bar.id.0,
        voice.id.0,
        beat.id.0,
        beat.notes[0].id.0,
    ];
    for id in ids {
        score.extensions.0.insert(id, payload());
    }
    assert_eq!(diagnose(&score), Vec::new());
}

#[test]
fn orphan_extensions_are_reported_last_and_sorted_by_id() {
    let mut score = four_four(&quarters(3));
    for id in [900, 5000, 700] {
        score.extensions.0.insert(ElementId(id), payload());
    }
    let expected = vec![
        Diagnostic::IncompleteBar {
            voice: first_voice(&score),
            filled: Fraction::new(3, 4),
            expected: Fraction::new(1, 1),
        },
        Diagnostic::OrphanExtension { id: ElementId(700) },
        Diagnostic::OrphanExtension { id: ElementId(900) },
        Diagnostic::OrphanExtension {
            id: ElementId(5000),
        },
    ];
    assert_eq!(diagnose(&score), expected);
}

#[test]
fn extension_on_a_removed_element_becomes_an_orphan() {
    let mut score = four_four(&quarters(4));
    let beat = score.tracks[0].staves[0].bars[0].voices[0].beats.remove(3);
    score.extensions.0.insert(beat.id.0, payload());
    let note = beat.notes[0].id.0;
    score.extensions.0.insert(note, payload());
    let mut ids = vec![beat.id.0, note];
    ids.sort();
    let mut found = Vec::new();
    for diagnostic in diagnose(&score) {
        if let Diagnostic::OrphanExtension { id } = diagnostic {
            found.push(id);
        }
    }
    assert_eq!(found, ids);
}

#[test]
fn fretted_note_on_percussion_track_is_a_kind_mismatch_only() {
    let mut score = percussion_score();
    score.tracks[0].staves[0].bars[0].voices[0].beats[2].notes[0].kind =
        NoteKind::Fretted { string: 0, fret: 3 };
    let note = score.tracks[0].staves[0].bars[0].voices[0].beats[2].notes[0].id;
    assert_eq!(
        diagnose(&score),
        vec![Diagnostic::NoteKindMismatch {
            note,
            track: score.tracks[0].id
        }]
    );
}

#[test]
fn percussion_note_on_fretted_track_is_a_kind_mismatch() {
    let mut score = four_four(&quarters(4));
    let beats = &mut score.tracks[0].staves[0].bars[0].voices[0].beats;
    beats[1].notes[0].kind = NoteKind::Percussion { articulation: 38 };
    beats[3].notes[0].kind = NoteKind::Percussion { articulation: 36 };
    let track: TrackId = score.tracks[0].id;
    let beats = &score.tracks[0].staves[0].bars[0].voices[0].beats;
    let notes: Vec<NoteId> = vec![beats[1].notes[0].id, beats[3].notes[0].id];
    assert_eq!(
        diagnose(&score),
        notes
            .into_iter()
            .map(|note| Diagnostic::NoteKindMismatch { note, track })
            .collect::<Vec<_>>()
    );
}

#[test]
fn matching_kinds_on_both_instruments_are_clean() {
    assert_eq!(diagnose(&percussion_score()), Vec::new());
}

#[test]
fn a_bar_without_voices_is_a_normal_empty_bar() {
    let mut score = four_four(&quarters(4));
    let extra = bar(&mut score, &quarters(4));
    let second_master = master_bar(&mut score, 4, 4);
    score.master_bars.push(second_master);
    score.tracks[0].staves[0].bars.push(extra);
    score.tracks[0].staves[0].bars[1].voices.clear();
    assert_eq!(diagnose(&score), Vec::new());
}

#[test]
fn fill_not_computable_is_reported_even_without_a_master_bar() {
    let primes = [
        2u8, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61,
    ];
    let durations: Vec<Duration> = primes
        .iter()
        .map(|p| {
            Duration::new(
                NoteValue::OneTwentyEighth,
                3,
                Some(Tuplet::new(*p, 1).unwrap()),
            )
            .unwrap()
        })
        .collect();
    let mut score = four_four(&durations);
    score.master_bars.clear();
    let staff = score.tracks[0].staves[0].id;
    assert_eq!(
        diagnose(&score),
        vec![
            Diagnostic::StaffBarCountMismatch {
                staff,
                bars: 1,
                master_bars: 0,
            },
            Diagnostic::FillNotComputable {
                voice: first_voice(&score)
            },
        ]
    );
}

#[test]
fn new_diagnostics_round_trip_through_json() {
    let cases = [
        (
            Diagnostic::FillNotComputable {
                voice: opentab_model::VoiceId(ElementId(4)),
            },
            r#"{"kind":"fill_not_computable","voice":4}"#,
        ),
        (
            Diagnostic::OrphanExtension { id: ElementId(9) },
            r#"{"kind":"orphan_extension","id":9}"#,
        ),
        (
            Diagnostic::NoteKindMismatch {
                note: NoteId(ElementId(6)),
                track: TrackId(ElementId(2)),
            },
            r#"{"kind":"note_kind_mismatch","note":6,"track":2}"#,
        ),
    ];
    for (diagnostic, expected) in cases {
        let json = serde_json::to_string(&diagnostic).unwrap();
        assert_eq!(json, expected);
        assert_eq!(
            serde_json::from_str::<Diagnostic>(&json).unwrap(),
            diagnostic
        );
    }
}

#[test]
fn diagnostic_fractions_are_validated_on_deserialise() {
    for fraction in ["[1,0]", "[-3,4]", "[3,-4]", "[0,0]"] {
        for kind in ["incomplete_bar", "overfull_bar"] {
            for field in ["filled", "expected"] {
                let (filled, expected) = if field == "filled" {
                    (fraction, "[1,1]")
                } else {
                    ("[1,1]", fraction)
                };
                let text = format!(
                    r#"{{"kind":"{kind}","voice":3,"filled":{filled},"expected":{expected}}}"#
                );
                let error = serde_json::from_str::<Diagnostic>(&text).unwrap_err();
                assert!(error.to_string().contains("fraction"), "{text}: {error}");
            }
        }
    }
    let ok = r#"{"kind":"overfull_bar","voice":3,"filled":[6,8],"expected":[1,1]}"#;
    assert_eq!(
        serde_json::from_str::<Diagnostic>(ok).unwrap(),
        Diagnostic::OverfullBar {
            voice: opentab_model::VoiceId(ElementId(3)),
            filled: Fraction::new(3, 4),
            expected: Fraction::new(1, 1),
        }
    );
}

fn tempo_at(numer: i64, denom: i64) -> opentab_model::TempoChange {
    opentab_model::TempoChange {
        position: Fraction::new(numer, denom),
        bpm: opentab_model::Tempo::new(100.0).unwrap(),
        unit: opentab_model::TempoUnit::Quarter,
    }
}

#[test]
fn tempo_changes_inside_the_bar_are_clean() {
    let mut score = four_four(&quarters(4));
    score.master_bars[0].tempo_changes = vec![tempo_at(0, 1), tempo_at(63, 64)];
    assert_eq!(diagnose(&score), Vec::new());
}

#[test]
fn tempo_changes_at_or_past_the_bar_end_are_reported_in_order() {
    let mut score = four_four(&quarters(3));
    let second = master_bar(&mut score, 3, 8);
    score.master_bars.push(second);
    let first_id = score.master_bars[0].id;
    let second_id = score.master_bars[1].id;
    score.master_bars[0].tempo_changes = vec![tempo_at(5, 4), tempo_at(1, 2), tempo_at(1, 1)];
    score.master_bars[1].tempo_changes = vec![tempo_at(3, 8)];
    let diagnostics = diagnose(&score);
    assert_eq!(
        diagnostics[..3],
        [
            Diagnostic::TempoChangeOutsideBar {
                master_bar: first_id,
                position: Fraction::new(5, 4),
            },
            Diagnostic::TempoChangeOutsideBar {
                master_bar: first_id,
                position: Fraction::new(1, 1),
            },
            Diagnostic::TempoChangeOutsideBar {
                master_bar: second_id,
                position: Fraction::new(3, 8),
            },
        ]
    );
    assert!(matches!(
        diagnostics[3],
        Diagnostic::StaffBarCountMismatch { .. }
    ));
    assert!(matches!(diagnostics[4], Diagnostic::IncompleteBar { .. }));
    assert_eq!(diagnostics.len(), 5);
}

#[test]
fn tempo_change_outside_bar_round_trips_and_validates_its_position() {
    let diagnostic = Diagnostic::TempoChangeOutsideBar {
        master_bar: opentab_model::MasterBarId(ElementId(3)),
        position: Fraction::new(5, 4),
    };
    let json = serde_json::to_string(&diagnostic).unwrap();
    assert_eq!(
        json,
        r#"{"kind":"tempo_change_outside_bar","master_bar":3,"position":[5,4]}"#
    );
    assert_eq!(
        serde_json::from_str::<Diagnostic>(&json).unwrap(),
        diagnostic
    );
    let negative = r#"{"kind":"tempo_change_outside_bar","master_bar":3,"position":[-1,4]}"#;
    assert!(serde_json::from_str::<Diagnostic>(negative).is_err());
}

#[test]
fn duplicate_strings_in_a_beat_are_reported_once_per_string_in_order() {
    let mut score = four_four(&quarters(4));
    let extra = [(0, 1), (2, 3), (0, 5), (2, 7), (0, 9)];
    let notes: Vec<opentab_model::Note> = extra
        .iter()
        .map(|(string, fret)| crate::support::note(&mut score, *string, *fret))
        .collect();
    let first = score.tracks[0].staves[0].bars[0].voices[0].beats[0].id;
    let second = score.tracks[0].staves[0].bars[0].voices[0].beats[1].id;
    score.tracks[0].staves[0].bars[0].voices[0].beats[0].notes = notes[..4].to_vec();
    score.tracks[0].staves[0].bars[0].voices[0].beats[1].notes = notes[4..].to_vec();
    assert_eq!(
        diagnose(&score),
        vec![
            Diagnostic::DuplicateString {
                beat: first,
                string: 0
            },
            Diagnostic::DuplicateString {
                beat: first,
                string: 2
            },
        ]
    );
    let repeat = crate::support::note(&mut score, 0, 2);
    score.tracks[0].staves[0].bars[0].voices[0].beats[1]
        .notes
        .push(repeat);
    assert_eq!(
        diagnose(&score)[2],
        Diagnostic::DuplicateString {
            beat: second,
            string: 0
        }
    );
    let json = serde_json::to_string(&diagnose(&score)[2]).unwrap();
    assert_eq!(
        json,
        format!(
            r#"{{"kind":"duplicate_string","beat":{},"string":0}}"#,
            second.0.0
        )
    );
}

#[test]
fn distinct_strings_are_not_duplicates() {
    let mut score = four_four(&quarters(4));
    let notes = vec![
        crate::support::note(&mut score, 0, 1),
        crate::support::note(&mut score, 1, 1),
    ];
    score.tracks[0].staves[0].bars[0].voices[0].beats[0].notes = notes;
    assert_eq!(diagnose(&score), Vec::new());
}

fn grace_beat(score: &mut Score, grace: opentab_model::Grace) -> opentab_model::Beat {
    let mut grace_beat = beat(score, duration(NoteValue::Sixteenth), Vec::new());
    grace_beat.effects.grace = Some(grace);
    grace_beat
}

#[test]
fn grace_beats_do_not_count_toward_the_fill() {
    for grace in [
        opentab_model::Grace::BeforeBeat,
        opentab_model::Grace::OnBeat,
    ] {
        let mut score = four_four(&quarters(4));
        let extra = grace_beat(&mut score, grace);
        score.tracks[0].staves[0].bars[0].voices[0]
            .beats
            .insert(2, extra);
        assert_eq!(diagnose(&score), Vec::new());
    }
}

#[test]
fn grace_beats_do_not_hide_a_short_bar() {
    let mut score = four_four(&quarters(3));
    let extra = grace_beat(&mut score, opentab_model::Grace::BeforeBeat);
    score.tracks[0].staves[0].bars[0].voices[0]
        .beats
        .push(extra);
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
fn a_voice_of_only_grace_beats_counts_as_empty() {
    let mut score = four_four(&[]);
    let extra = grace_beat(&mut score, opentab_model::Grace::OnBeat);
    score.tracks[0].staves[0].bars[0].voices[0]
        .beats
        .push(extra);
    assert_eq!(diagnose(&score), Vec::new());
}

#[test]
fn anacrusis_allows_an_underfull_first_bar_only() {
    let mut score = four_four(&quarters(1));
    let second = bar(&mut score, &quarters(2));
    score.tracks[0].staves[0].bars.push(second);
    let master = master_bar(&mut score, 4, 4);
    score.master_bars.push(master);
    let second_voice = score.tracks[0].staves[0].bars[1].voices[0].id;
    assert_eq!(diagnose(&score).len(), 2);
    score.anacrusis = true;
    assert_eq!(
        diagnose(&score),
        vec![Diagnostic::IncompleteBar {
            voice: second_voice,
            filled: Fraction::new(1, 2),
            expected: Fraction::new(1, 1),
        }]
    );
}

#[test]
fn anacrusis_does_not_excuse_an_overfull_first_bar() {
    let mut score = four_four(&quarters(5));
    score.anacrusis = true;
    assert_eq!(
        diagnose(&score),
        vec![Diagnostic::OverfullBar {
            voice: first_voice(&score),
            filled: Fraction::new(5, 4),
            expected: Fraction::new(1, 1),
        }]
    );
}

fn two_voice_score(first: u8, second: u8) -> Score {
    let mut score = four_four(&quarters(4));
    let mut extra = voice(&mut score, &quarters(4));
    let bar = &mut score.tracks[0].staves[0].bars[0];
    bar.voices[0].slot = VoiceSlot::new(first).unwrap();
    extra.slot = VoiceSlot::new(second).unwrap();
    bar.voices.push(extra);
    score
}

#[test]
fn strictly_increasing_voice_slots_are_clean() {
    for (first, second) in [(0, 1), (0, 3), (2, 3), (1, 2)] {
        assert_eq!(diagnose(&two_voice_score(first, second)), Vec::new());
    }
}

#[test]
fn equal_or_decreasing_voice_slots_are_reported_once_per_bar() {
    for (first, second) in [(0, 0), (3, 3), (1, 0), (3, 2)] {
        let score = two_voice_score(first, second);
        assert_eq!(
            diagnose(&score),
            vec![Diagnostic::VoiceSlotOrder {
                bar: score.tracks[0].staves[0].bars[0].id
            }]
        );
    }
}

#[test]
fn voice_slot_order_round_trips_through_json() {
    let diagnostic = Diagnostic::VoiceSlotOrder {
        bar: BarId(ElementId(9)),
    };
    let text = serde_json::to_string(&diagnostic).unwrap();
    assert_eq!(text, r#"{"kind":"voice_slot_order","bar":9}"#);
    assert_eq!(
        serde_json::from_str::<Diagnostic>(&text).unwrap(),
        diagnostic
    );
}
