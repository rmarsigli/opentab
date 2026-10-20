use opentab_gp::{GpError, Refusal, write_gp};
use opentab_model::{Diagnostic, DrumKit, Marker, Score, Sound, diagnose, gm_drum_kit};

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
    let drums = Doc {
        instrument: "drumKit".to_string(),
        tuning: "0 0 0 0 0 0".to_string(),
        note: "<InstrumentArticulation>3</InstrumentArticulation>".to_string(),
        ..Doc::default()
    }
    .read();
    assert_eq!(check(&drums, &KIT_FIELDS), KIT_FIELDS.len() * escaped);
}
