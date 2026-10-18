use opentab_model::{Diagnostic, NoteKind, Score, diagnose, from_json, to_json};
use opentab_testkit::{ScoreGenConfig, arb_score, arb_score_with, min_beats_for_bar};
use proptest::prelude::*;
use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::{Config, RngAlgorithm, TestError, TestRng, TestRunner};

const CASES: u32 = 256;
const COVERAGE_SAMPLES: usize = 256;
const SHRINK_SEED: [u8; 32] = [7; 32];
const SHRINK_ITERS: u32 = 100_000;
const LONG_TEXT_CHARS: usize = 10_000;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn p1_generated_scores_have_no_diagnostics(score in arb_score()) {
        prop_assert_eq!(diagnose(&score), Vec::new());
    }

    #[test]
    fn p2_json_round_trips(score in arb_score()) {
        let json = to_json(&score).expect("serialises");
        let back = from_json(&json).expect("generated score must parse");
        prop_assert_eq!(back, score);
    }

    #[test]
    fn json_text_is_a_fixed_point(score in arb_score()) {
        let json = to_json(&score).expect("serialises");
        let back = from_json(&json).expect("generated score must parse");
        prop_assert_eq!(to_json(&back).expect("serialises"), json);
    }

    #[test]
    fn round_tripped_scores_still_diagnose_clean(score in arb_score()) {
        let back = from_json(&to_json(&score).expect("serialises")).expect("generated score must parse");
        prop_assert_eq!(diagnose(&back), Vec::new());
    }

    #[test]
    fn lookups_resolve_every_element(score in arb_score()) {
        for (track_index, track) in score.tracks.iter().enumerate() {
            let found = score.find_track(track.id).expect("track is found");
            prop_assert_eq!(found.id, track.id);
            for (staff_index, staff) in track.staves.iter().enumerate() {
                for (bar_index, bar) in staff.bars.iter().enumerate() {
                    for (voice_index, voice) in bar.voices.iter().enumerate() {
                        let path = score.find_voice(voice.id).expect("voice is found");
                        prop_assert_eq!(path.track, track_index);
                        prop_assert_eq!(path.staff, staff_index);
                        prop_assert_eq!(path.bar, bar_index);
                        prop_assert_eq!(path.voice, voice_index);
                        for (beat_index, beat) in voice.beats.iter().enumerate() {
                            let beat_path = score.find_beat(beat.id).expect("beat is found");
                            prop_assert_eq!(beat_path.voice, voice_index);
                            prop_assert_eq!(beat_path.beat, beat_index);
                            for (note_index, note) in beat.notes.iter().enumerate() {
                                let note_path = score.find_note(note.id).expect("note is found");
                                prop_assert_eq!(note_path.beat, beat_path);
                                prop_assert_eq!(note_path.note, note_index);
                            }
                        }
                    }
                }
            }
        }
        for (index, master_bar) in score.master_bars.iter().enumerate() {
            prop_assert_eq!(score.find_master_bar_index(master_bar.id), Some(index));
        }
    }

    #[test]
    fn dropping_a_beat_is_always_diagnosed(score in arb_score(), pick in any::<usize>()) {
        let mut broken = score.clone();
        let voices: Vec<(usize, usize, usize, usize)> = broken
            .tracks
            .iter()
            .enumerate()
            .flat_map(|(t, track)| {
                track.staves.iter().enumerate().flat_map(move |(s, staff)| {
                    staff.bars.iter().enumerate().flat_map(move |(b, bar)| {
                        bar.voices
                            .iter()
                            .enumerate()
                            .filter(|(_, voice)| voice.beats.len() >= 2)
                            .map(move |(v, _)| (t, s, b, v))
                    })
                })
            })
            .collect();
        prop_assume!(!voices.is_empty());
        let (t, s, b, v) = voices[pick % voices.len()];
        let voice = &mut broken.tracks[t].staves[s].bars[b].voices[v];
        voice.beats.pop();
        let id = voice.id;
        let diagnostics = diagnose(&broken);
        let flagged = diagnostics
            .iter()
            .any(|d| matches!(d, Diagnostic::IncompleteBar { voice, .. } if *voice == id));
        prop_assert!(flagged);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn max_beats_per_voice_is_a_soft_cap(
        (cap, score) in (0usize..=8).prop_flat_map(|cap| {
            arb_score_with(ScoreGenConfig {
                max_beats_per_voice: cap,
                ..ScoreGenConfig::default()
            })
            .prop_map(move |score| (cap, score))
        })
    ) {
        for track in &score.tracks {
            for staff in &track.staves {
                for (bar, master_bar) in staff.bars.iter().zip(&score.master_bars) {
                    let limit = cap.max(min_beats_for_bar(master_bar.time_signature));
                    for voice in &bar.voices {
                        prop_assert!(voice.beats.len() <= limit);
                    }
                }
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn small_configs_hold_the_same_properties(
        score in arb_score_with(ScoreGenConfig {
            max_tracks: 1,
            max_master_bars: 2,
            max_beats_per_voice: 1,
            max_notes_per_beat: 1,
        })
    ) {
        prop_assert_eq!(diagnose(&score), Vec::new());
        prop_assert_eq!(from_json(&to_json(&score).expect("serialises")).expect("parses"), score);
    }
}

proptest! {
    #![proptest_config(Config {
        cases: CASES,
        max_shrink_time: 0,
        max_shrink_iters: SHRINK_ITERS,
        failure_persistence: None,
        ..Config::default()
    })]

    #[test]
    #[should_panic(expected = "minimal failing input")]
    fn shrinking_failure_panics_with_a_minimal_input(score in arb_score()) {
        prop_assert!(score.tracks.is_empty());
    }
}

fn seeded_runner() -> TestRunner {
    TestRunner::new_with_rng(
        Config {
            cases: CASES,
            max_shrink_time: 0,
            max_shrink_iters: SHRINK_ITERS,
            failure_persistence: None,
            ..Config::default()
        },
        TestRng::from_seed(RngAlgorithm::ChaCha, &SHRINK_SEED),
    )
}

#[test]
fn shrinking_reaches_a_minimal_score() {
    let mut runner = seeded_runner();
    let result = runner.run(&arb_score(), |score| {
        prop_assert!(score.tracks.is_empty());
        Ok(())
    });
    let Err(TestError::Fail(_, minimal)) = result else {
        panic!("property must fail on a score with tracks");
    };
    assert_eq!(minimal.tracks.len(), 1);
    assert!(minimal.master_bars.is_empty());
    assert!(minimal.meta.title.is_empty());
    assert!(minimal.extensions.0.is_empty());
}

#[derive(Default)]
struct Coverage {
    fretted: bool,
    percussion: bool,
    tuplet: bool,
    dotted: bool,
    multi_voice: bool,
    multi_staff: bool,
    non_empty_extensions: bool,
    non_ascii_meta: bool,
    emoji_meta: bool,
    long_meta: bool,
    rest_beat: bool,
    chord_beat: bool,
    zero_tracks: bool,
    zero_bars: bool,
    beat_tremolo: bool,
    beat_stroke: bool,
    beat_chord_name: bool,
    beat_text: bool,
    beat_fade_in: bool,
    beat_tap: bool,
    beat_slap: bool,
    beat_pop: bool,
    note_tie_origin: bool,
    note_tie_destination: bool,
    note_ghost: bool,
    note_dead: bool,
    note_accent: bool,
    note_palm_mute: bool,
    note_let_ring: bool,
    note_staccato: bool,
    note_vibrato: bool,
    note_hammer_on_pull_off: bool,
    note_slide: bool,
    note_harmonic: bool,
    note_bend: bool,
    note_left_hand_finger: bool,
    note_right_hand_finger: bool,
    marker: bool,
    repeats: bool,
    tempo_changes: bool,
}

fn observe(coverage: &mut Coverage, score: &Score) {
    let meta = &score.meta;
    let texts = [
        &meta.title,
        &meta.subtitle,
        &meta.artist,
        &meta.album,
        &meta.words,
        &meta.music,
        &meta.copyright,
        &meta.tab_author,
        &meta.instructions,
        &meta.notices,
    ];
    for text in texts {
        coverage.non_ascii_meta |= !text.is_ascii();
        coverage.emoji_meta |= text
            .chars()
            .any(|c| ('\u{1F300}'..='\u{1FAFF}').contains(&c));
        coverage.long_meta |= text.chars().count() >= LONG_TEXT_CHARS;
    }
    coverage.non_empty_extensions |= !score.extensions.0.is_empty();
    coverage.zero_tracks |= score.tracks.is_empty();
    coverage.zero_bars |= score.master_bars.is_empty();
    for master_bar in &score.master_bars {
        coverage.marker |= master_bar.marker.is_some();
        coverage.repeats |= master_bar.repeat_start || master_bar.repeat_end.is_some();
        coverage.tempo_changes |= !master_bar.tempo_changes.is_empty();
    }
    for track in &score.tracks {
        coverage.multi_staff |= track.staves.len() > 1;
        for staff in &track.staves {
            for bar in &staff.bars {
                coverage.multi_voice |= bar.voices.len() > 1;
                for voice in &bar.voices {
                    for beat in &voice.beats {
                        observe_beat(coverage, beat);
                    }
                }
            }
        }
    }
}

fn observe_beat(coverage: &mut Coverage, beat: &opentab_model::Beat) {
    coverage.tuplet |= beat.duration.tuplet().is_some();
    coverage.dotted |= beat.duration.dots() > 0;
    coverage.rest_beat |= beat.notes.is_empty();
    coverage.chord_beat |= beat.notes.len() > 1;
    let effects = &beat.effects;
    coverage.beat_tremolo |= effects.tremolo_picking.is_some();
    coverage.beat_stroke |= effects.stroke.is_some();
    coverage.beat_chord_name |= effects.chord_name.is_some();
    coverage.beat_text |= effects.text.is_some();
    coverage.beat_fade_in |= effects.fade_in;
    coverage.beat_tap |= effects.tap;
    coverage.beat_slap |= effects.slap;
    coverage.beat_pop |= effects.pop;
    for note in &beat.notes {
        match note.kind {
            NoteKind::Fretted { .. } => coverage.fretted = true,
            NoteKind::Percussion { .. } => coverage.percussion = true,
        }
        let effects = &note.effects;
        coverage.note_tie_origin |= effects.tie_origin;
        coverage.note_tie_destination |= effects.tie_destination;
        coverage.note_ghost |= effects.ghost;
        coverage.note_dead |= effects.dead;
        coverage.note_accent |= effects.accent.is_some();
        coverage.note_palm_mute |= effects.palm_mute;
        coverage.note_let_ring |= effects.let_ring;
        coverage.note_staccato |= effects.staccato;
        coverage.note_vibrato |= effects.vibrato.is_some();
        coverage.note_hammer_on_pull_off |= effects.hammer_on_pull_off;
        coverage.note_slide |= effects.slide.is_some();
        coverage.note_harmonic |= effects.harmonic.is_some();
        coverage.note_bend |= effects.bend.is_some();
        coverage.note_left_hand_finger |= effects.left_hand_finger.is_some();
        coverage.note_right_hand_finger |= effects.right_hand_finger.is_some();
    }
}

#[test]
fn generator_covers_every_feature() {
    let mut runner = TestRunner::deterministic();
    let strategy = arb_score();
    let mut coverage = Coverage::default();
    for _ in 0..COVERAGE_SAMPLES {
        let tree = strategy.new_tree(&mut runner).expect("tree");
        observe(&mut coverage, &tree.current());
    }
    let checks = [
        ("fretted", coverage.fretted),
        ("percussion", coverage.percussion),
        ("tuplet", coverage.tuplet),
        ("dotted", coverage.dotted),
        ("multi_voice", coverage.multi_voice),
        ("multi_staff", coverage.multi_staff),
        ("non_empty_extensions", coverage.non_empty_extensions),
        ("non_ascii_meta", coverage.non_ascii_meta),
        ("emoji_meta", coverage.emoji_meta),
        ("long_meta", coverage.long_meta),
        ("rest_beat", coverage.rest_beat),
        ("chord_beat", coverage.chord_beat),
        ("zero_tracks", coverage.zero_tracks),
        ("zero_bars", coverage.zero_bars),
        ("beat_tremolo", coverage.beat_tremolo),
        ("beat_stroke", coverage.beat_stroke),
        ("beat_chord_name", coverage.beat_chord_name),
        ("beat_text", coverage.beat_text),
        ("beat_fade_in", coverage.beat_fade_in),
        ("beat_tap", coverage.beat_tap),
        ("beat_slap", coverage.beat_slap),
        ("beat_pop", coverage.beat_pop),
        ("note_tie_origin", coverage.note_tie_origin),
        ("note_tie_destination", coverage.note_tie_destination),
        ("note_ghost", coverage.note_ghost),
        ("note_dead", coverage.note_dead),
        ("note_accent", coverage.note_accent),
        ("note_palm_mute", coverage.note_palm_mute),
        ("note_let_ring", coverage.note_let_ring),
        ("note_staccato", coverage.note_staccato),
        ("note_vibrato", coverage.note_vibrato),
        ("note_hammer_on_pull_off", coverage.note_hammer_on_pull_off),
        ("note_slide", coverage.note_slide),
        ("note_harmonic", coverage.note_harmonic),
        ("note_bend", coverage.note_bend),
        ("note_left_hand_finger", coverage.note_left_hand_finger),
        ("note_right_hand_finger", coverage.note_right_hand_finger),
        ("marker", coverage.marker),
        ("repeats", coverage.repeats),
        ("tempo_changes", coverage.tempo_changes),
    ];
    let missing: Vec<&str> = checks
        .iter()
        .filter(|(_, seen)| !seen)
        .map(|(name, _)| *name)
        .collect();
    assert!(missing.is_empty(), "never generated: {missing:?}");
}

#[test]
fn shrinking_a_structural_failure_isolates_one_voice() {
    let mut runner = seeded_runner();
    let result = runner.run(&arb_score(), |score| {
        let has_tuplet = score
            .tracks
            .iter()
            .flat_map(|track| &track.staves)
            .flat_map(|staff| &staff.bars)
            .flat_map(|bar| &bar.voices)
            .flat_map(|voice| &voice.beats)
            .any(|beat| beat.duration.tuplet().is_some());
        prop_assert!(!has_tuplet);
        Ok(())
    });
    let Err(TestError::Fail(_, minimal)) = result else {
        panic!("property must fail on a score with a tuplet");
    };
    assert_eq!(minimal.tracks.len(), 1);
    assert_eq!(minimal.master_bars.len(), 1);
    assert_eq!(minimal.tracks[0].staves.len(), 1);
    assert_eq!(minimal.tracks[0].staves[0].bars[0].voices.len(), 1);
    assert!(minimal.tracks[0].staves[0].bars[0].voices[0].beats.len() <= 7);
}
