use std::cell::Cell;

use opentab_gp::{GpError, Refusal, write_gp};
use opentab_model::{Beat, Diagnostic, DrumKit, Marker, Score, Sound, diagnose, gm_drum_kit};
use opentab_testkit::{ScoreGenConfig, arb_any_text, arb_score_with};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

use crate::support::Doc;

struct Field {
    name: &'static str,
    edit: fn(&mut Score, String),
}

fn beat_text(score: &mut Score, text: String) {
    score.tracks[0].staves[0].bars[0].voices[0].beats[0]
        .effects
        .text = Some(text);
}

fn sound_name(score: &mut Score, text: String) {
    let track = &mut score.tracks[0];
    track.sounds = vec![Sound {
        name: text,
        program: track.instrument.midi_program,
        bank: track.midi.bank,
    }];
}

fn kit_with(edit: impl FnOnce(&mut DrumKit) -> DrumKit) -> DrumKit {
    let mut kit = gm_drum_kit();
    edit(&mut kit)
}

fn kit_element(score: &mut Score, text: String) {
    score.tracks[0].drum_kit = Some(kit_with(|kit| {
        let mut elements = kit.elements().to_vec();
        elements[0].soundbank = text;
        DrumKit::new(elements, kit.articulations().to_vec()).expect("kit")
    }));
}

fn kit_articulation(score: &mut Score, text: String) {
    score.tracks[0].drum_kit = Some(kit_with(|kit| {
        let mut articulations = kit.articulations().to_vec();
        articulations[1].technique_symbol = text;
        DrumKit::new(kit.elements().to_vec(), articulations).expect("kit")
    }));
}

const FIELDS: [Field; 8] = [
    Field {
        name: "title",
        edit: |score, text| score.meta.title = text,
    },
    Field {
        name: "notices",
        edit: |score, text| score.meta.notices = text,
    },
    Field {
        name: "marker",
        edit: |score, text| {
            score.master_bars[0].marker = Some(Marker {
                text,
                letter: "A".to_string(),
            })
        },
    },
    Field {
        name: "letter",
        edit: |score, letter| {
            score.master_bars[0].marker = Some(Marker {
                text: "Verse".to_string(),
                letter,
            })
        },
    },
    Field {
        name: "beat",
        edit: beat_text,
    },
    Field {
        name: "track",
        edit: |score, text| score.tracks[0].name = text,
    },
    Field {
        name: "short name",
        edit: |score, text| score.tracks[0].short_name = text,
    },
    Field {
        name: "sound",
        edit: sound_name,
    },
];

fn chord_name(score: &mut Score, text: String) {
    score.tracks[0].staves[0].bars[0].voices[0].beats[0]
        .effects
        .chord_name = Some(text);
}

const ESCAPED_FIELDS: [Field; 1] = [Field {
    name: "chord name",
    edit: chord_name,
}];

const KIT_FIELDS: [Field; 2] = [
    Field {
        name: "kit element",
        edit: kit_element,
    },
    Field {
        name: "kit articulation",
        edit: kit_articulation,
    },
];

fn probes() -> Vec<char> {
    let mut probes: Vec<char> = ('\u{0}'..='\u{7F}').collect();
    probes.extend([
        '\u{D7FF}',
        '\u{E000}',
        '\u{FFFD}',
        '\u{FFFE}',
        '\u{FFFF}',
        '\u{10000}',
        '\u{10FFFF}',
        'é',
    ]);
    probes
}

fn refused(score: &Score, probe: char) -> bool {
    match write_gp(score) {
        Ok(_) => false,
        Err(GpError::Unsupported {
            reason: Refusal::Character { code },
            ..
        }) => code == u32::from(probe),
        Err(other) => panic!("unexpected error {other}"),
    }
}

fn diagnosed(score: &Score, probe: char) -> bool {
    diagnose(score).iter().any(|diagnostic| {
        matches!(diagnostic, Diagnostic::UnwritableText { code, .. } if *code == u32::from(probe))
    })
}

fn check(base: &Score, fields: &[Field]) -> usize {
    assert!(diagnose(base).is_empty());
    let mut refusals = 0usize;
    for field in fields {
        for probe in probes() {
            let mut score = base.clone();
            (field.edit)(&mut score, format!("a{probe}b"));
            let refused = refused(&score, probe);
            assert_eq!(
                refused,
                diagnosed(&score, probe),
                "{}: {probe:?}",
                field.name
            );
            refusals += usize::from(refused);
        }
    }
    refusals
}

#[test]
fn text_the_writer_refuses_is_exactly_the_text_diagnose_reports() {
    let literal = 32;
    let escaped = literal - 1;
    assert_eq!(
        check(&Doc::default().read(), &FIELDS),
        FIELDS.len() * literal
    );
    assert_eq!(
        check(&Doc::default().read(), &ESCAPED_FIELDS),
        ESCAPED_FIELDS.len() * escaped
    );
    let drums = Doc {
        instrument: "drumKit".to_string(),
        tuning: "0 0 0 0 0 0".to_string(),
        note: "<InstrumentArticulation>3</InstrumentArticulation>".to_string(),
        ..Doc::default()
    }
    .read();
    assert_eq!(check(&drums, &KIT_FIELDS), KIT_FIELDS.len() * escaped);
}

const ARBITRARY_CASES: u32 = 256;
const MAX_ARBITRARY_CHARS: usize = 16;
const MIN_OUTCOMES: usize = 16;
const PLACED_FIELDS: [&str; 9] = [
    "title",
    "notices",
    "marker",
    "letter",
    "beat",
    "track",
    "short name",
    "sound",
    "chord name",
];

fn first_beat(score: &mut Score) -> Option<&mut Beat> {
    score
        .tracks
        .first_mut()?
        .staves
        .first_mut()?
        .bars
        .first_mut()?
        .voices
        .first_mut()?
        .beats
        .first_mut()
}

fn place(score: &mut Score, field: usize, text: String) -> bool {
    match PLACED_FIELDS[field % PLACED_FIELDS.len()] {
        "beat" => match first_beat(score) {
            Some(beat) => {
                beat.effects.text = Some(text);
                true
            }
            None => false,
        },
        "chord name" => match first_beat(score) {
            Some(beat) => {
                beat.effects.chord_name = Some(text);
                true
            }
            None => false,
        },
        "sound" => match score.tracks[0].sounds.first_mut() {
            Some(sound) => {
                sound.name = text;
                true
            }
            None => false,
        },
        name => match FIELDS.iter().find(|known| known.name == name) {
            Some(known) => {
                (known.edit)(score, text);
                true
            }
            None => false,
        },
    }
}

fn unwritable_codes(score: &Score) -> Vec<u32> {
    diagnose(score)
        .iter()
        .filter_map(|diagnostic| match diagnostic {
            Diagnostic::UnwritableText { code, .. } => Some(*code),
            _ => None,
        })
        .collect()
}

#[derive(Default)]
struct Outcomes {
    refused: Cell<usize>,
    written: Cell<usize>,
}

#[test]
fn arbitrary_text_is_diagnosed_exactly_when_the_writer_refuses_it() {
    let strategy = (
        arb_score_with(ScoreGenConfig {
            gp_writable: true,
            ..ScoreGenConfig::default()
        }),
        any::<usize>(),
        arb_any_text(MAX_ARBITRARY_CHARS),
    );
    let outcomes = Outcomes::default();
    let mut runner = TestRunner::new_with_rng(
        Config {
            cases: ARBITRARY_CASES,
            failure_persistence: None,
            ..Config::default()
        },
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    );
    let result = runner.run(&strategy, |(base, field, text)| {
        prop_assume!(write_gp(&base).is_ok());
        let mut score = base.clone();
        prop_assume!(place(&mut score, field, text.clone()));
        let codes = unwritable_codes(&score);
        match write_gp(&score) {
            Ok(_) => {
                prop_assert_eq!(codes, Vec::<u32>::new(), "{:?}", text);
                outcomes.written.set(outcomes.written.get() + 1);
            }
            Err(GpError::Unsupported {
                reason: Refusal::Character { code },
                ..
            }) => {
                prop_assert_eq!(codes, vec![code], "{:?}", text);
                outcomes.refused.set(outcomes.refused.get() + 1);
            }
            Err(other) => return Err(TestCaseError::fail(format!("{other} for {text:?}"))),
        }
        Ok(())
    });
    if let Err(error) = result {
        panic!("{error}");
    }
    assert!(
        outcomes.refused.get() >= MIN_OUTCOMES,
        "{}",
        outcomes.refused.get()
    );
    assert!(
        outcomes.written.get() >= MIN_OUTCOMES,
        "{}",
        outcomes.written.get()
    );
}
