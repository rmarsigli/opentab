use opentab_gp::{GpError, GpifProblem, read_gp};
use opentab_model::{from_json, to_json};

use crate::support::{Doc, SCORE_ENTRY, archive, gp, read_gpif_error};

const MUTATION_ROUNDS: usize = 3000;
const REPLACEMENTS: &[u8] = b"<>/=\"'&;!?-[]0123456789 xA\n";

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next()).unwrap_or_default() % bound.max(1)
    }
}

fn rich_document() -> String {
    Doc {
        master_track: "<Automations><Automation><Type>Tempo</Type><Bar>0</Bar><Position>0.5</Position><Value>90 2</Value></Automation></Automations>".to_string(),
        master_bar: "<Repeat start=\"true\" end=\"true\" count=\"2\"/><Section><Letter><![CDATA[A]]></Letter></Section>".to_string(),
        beat: "<Dynamic>F</Dynamic><Properties><Property name=\"Brush\"><Direction>Up</Direction></Property></Properties>".to_string(),
        note: "<Tie origin=\"true\" destination=\"false\"/><Accent>9</Accent>".to_string(),
        note_properties: "<Property name=\"Slide\"><Flags>2</Flags></Property><Property name=\"Bended\"><Enable/></Property>".to_string(),
        rhythm: "<PrimaryTuplet num=\"3\" den=\"2\"/>".to_string(),
        ..Doc::default()
    }
    .render()
}

#[test]
fn truncated_documents_never_panic() {
    let gpif = rich_document();
    for end in 0..gpif.len() {
        if gpif.is_char_boundary(end)
            && let Ok(score) = read_gp(&gp(&gpif[..end]))
        {
            let json = to_json(&score).expect("valid model");
            assert_eq!(from_json(&json).expect("json back"), score);
        }
    }
}

#[test]
fn mutated_documents_never_panic() {
    let gpif = rich_document().into_bytes();
    let mut random = Lcg(0x5eed);
    let mut accepted = 0usize;
    for _ in 0..MUTATION_ROUNDS {
        let mut mutated = gpif.clone();
        for _ in 0..=random.below(3) {
            let at = random.below(mutated.len());
            match random.below(3) {
                0 => mutated[at] = REPLACEMENTS[random.below(REPLACEMENTS.len())],
                1 => {
                    mutated.remove(at);
                }
                _ => {
                    let end = (at + random.below(40)).min(mutated.len());
                    let copy = mutated[at..end].to_vec();
                    let target = random.below(mutated.len());
                    mutated.splice(target..target, copy);
                }
            }
        }
        if let Ok(score) = read_gp(&archive(&[(SCORE_ENTRY, &mutated)])) {
            accepted += 1;
            let json = to_json(&score).expect("valid model");
            assert_eq!(from_json(&json).expect("json back"), score);
        }
    }
    assert!(accepted > 0);
}

#[test]
fn truncated_archives_never_panic() {
    let bytes = gp(&rich_document());
    for end in 0..bytes.len() {
        let _ = read_gp(&bytes[..end]);
    }
}

#[test]
fn deep_nesting_is_rejected() {
    let deep = format!("{}{}", "<a>".repeat(300), "</a>".repeat(300));
    let gpif = Doc::default()
        .render()
        .replace("<GPRevision>", &format!("{deep}<GPRevision>"));
    assert!(matches!(
        read_gpif_error(&gpif),
        GpError::Gpif {
            problem: GpifProblem::DepthLimit { limit: 256 },
            ..
        }
    ));
}

#[test]
fn malformed_xml_is_a_typed_error() {
    let cases: [&[u8]; 6] = [
        b"",
        b"\xff\xfe<GPIF/>",
        b"<!DOCTYPE GPIF [<!ENTITY x \"y\">]><GPIF/>",
        b"<GPIF><Score></GPIF>",
        b"<GPIF>&bogus;</GPIF>",
        b"<GPIF/><GPIF/>",
    ];
    for case in cases {
        let error = read_gp(&archive(&[(SCORE_ENTRY, case)])).expect_err("malformed");
        assert!(matches!(error, GpError::Gpif { .. }), "{error}");
    }
    let error = read_gp(&archive(&[(SCORE_ENTRY, b"<Score/>")])).expect_err("root");
    assert!(matches!(
        error,
        GpError::Gpif {
            problem: GpifProblem::UnexpectedRoot { .. },
            ..
        }
    ));
}

#[test]
fn reference_expansion_is_bounded() {
    let beats = vec!["0"; 1_100_000].join(" ");
    let gpif = Doc::default()
        .render()
        .replace("<Beats>0</Beats>", &format!("<Beats>{beats}</Beats>"));
    assert!(matches!(
        read_gpif_error(&gpif),
        GpError::Gpif {
            problem: GpifProblem::ExpansionLimit { limit: 1_000_000 },
            ..
        }
    ));
}

#[test]
fn opaque_copies_of_shared_elements_are_bounded() {
    let large = "x".repeat(1024 * 1024);
    let references = vec!["0"; 10_000].join(" ");
    let doc = Doc {
        note: format!("<Ornament>{large}</Ornament>"),
        ..Doc::default()
    };
    let gpif = doc
        .render()
        .replace("<Notes>0</Notes>", &format!("<Notes>{references}</Notes>"));
    assert!(matches!(
        read_gpif_error(&gpif),
        GpError::Gpif {
            problem: GpifProblem::OpaqueLimit { limit: 67_108_864 },
            ..
        }
    ));
}
