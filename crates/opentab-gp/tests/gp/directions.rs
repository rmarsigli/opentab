use opentab_gp::{read_gp, write_gp};
use opentab_model::{
    DIRECTION_JUMPS, DIRECTION_TARGETS, Diagnostic, DirectionJump, DirectionTarget, Directions,
    Score, diagnose,
};

use crate::anchors::gpif_text;
use crate::support::{Doc, master_bar, payloads};

const TARGET_NAMES: [(&str, DirectionTarget); 5] = [
    ("Segno", DirectionTarget::Segno),
    ("SegnoSegno", DirectionTarget::SegnoSegno),
    ("Coda", DirectionTarget::Coda),
    ("DoubleCoda", DirectionTarget::DoubleCoda),
    ("Fine", DirectionTarget::Fine),
];

const JUMP_NAMES: [(&str, DirectionJump); 14] = [
    ("DaCapo", DirectionJump::DaCapo),
    ("DaCapoAlFine", DirectionJump::DaCapoAlFine),
    ("DaCapoAlCoda", DirectionJump::DaCapoAlCoda),
    ("DaCapoAlDoubleCoda", DirectionJump::DaCapoAlDoubleCoda),
    ("DaSegno", DirectionJump::DaSegno),
    ("DaSegnoAlFine", DirectionJump::DaSegnoAlFine),
    ("DaSegnoAlCoda", DirectionJump::DaSegnoAlCoda),
    ("DaSegnoAlDoubleCoda", DirectionJump::DaSegnoAlDoubleCoda),
    ("DaSegnoSegno", DirectionJump::DaSegnoSegno),
    ("DaSegnoSegnoAlFine", DirectionJump::DaSegnoSegnoAlFine),
    ("DaSegnoSegnoAlCoda", DirectionJump::DaSegnoSegnoAlCoda),
    (
        "DaSegnoSegnoAlDoubleCoda",
        DirectionJump::DaSegnoSegnoAlDoubleCoda,
    ),
    ("DaCoda", DirectionJump::DaCoda),
    ("DaDoubleCoda", DirectionJump::DaDoubleCoda),
];

fn with_directions(directions: &str) -> Doc {
    Doc {
        master_bar: directions.to_string(),
        ..Doc::default()
    }
}

fn written(score: &Score) -> String {
    gpif_text(&write_gp(score).expect("write")).replace('\n', "")
}

fn directions_of(gpif: &str) -> String {
    let start = gpif.find("<Directions").expect("directions written");
    let end = gpif[start..]
        .find("</Directions>")
        .map(|at| start + at + "</Directions>".len())
        .expect("directions closed");
    gpif[start..end].to_string()
}

#[test]
fn every_target_and_jump_name_maps_to_its_own_value_and_back() {
    assert_eq!(TARGET_NAMES.len(), DIRECTION_TARGETS.len());
    assert_eq!(JUMP_NAMES.len(), DIRECTION_JUMPS.len());
    for (name, target) in TARGET_NAMES {
        let element = format!("<Directions><Target>{name}</Target></Directions>");
        let score = with_directions(&element).read();
        let bar = master_bar(&score);
        assert_eq!(bar.directions.targets, vec![target], "{name}");
        assert!(bar.directions.jumps.is_empty(), "{name}");
        assert!(payloads(&score, bar.id.0).is_empty(), "{name}");
        assert_eq!(directions_of(&written(&score)), element, "{name}");
    }
    for (name, jump) in JUMP_NAMES {
        let element = format!("<Directions><Jump>{name}</Jump></Directions>");
        let score = with_directions(&element).read();
        let bar = master_bar(&score);
        assert_eq!(bar.directions.jumps, vec![jump], "{name}");
        assert!(bar.directions.targets.is_empty(), "{name}");
        assert!(payloads(&score, bar.id.0).is_empty(), "{name}");
        assert_eq!(directions_of(&written(&score)), element, "{name}");
    }
}

#[test]
fn several_directions_keep_their_order_and_sit_before_bars() {
    let element = concat!(
        "<Directions><Target>Fine</Target><Target>Segno</Target>",
        "<Jump>DaSegnoAlCoda</Jump><Jump>DaCoda</Jump></Directions>"
    );
    let score = with_directions(&format!("<DoubleBar/>{element}")).read();
    let bar = master_bar(&score);
    assert_eq!(
        bar.directions,
        Directions {
            targets: vec![DirectionTarget::Fine, DirectionTarget::Segno],
            jumps: vec![DirectionJump::DaSegnoAlCoda, DirectionJump::DaCoda],
        }
    );
    assert!(payloads(&score, bar.id.0).is_empty());
    let text = written(&score);
    assert!(
        text.contains(&format!("<DoubleBar/>{element}<Bars>")),
        "{text}"
    );
}

#[test]
fn a_bar_without_directions_writes_none() {
    let score = Doc::default().read();
    assert!(master_bar(&score).directions.is_empty());
    assert!(!written(&score).contains("Directions"));
}

#[test]
fn an_uncanonical_directions_element_is_replayed_until_the_directions_change() {
    let cases = [
        (
            "<Directions><Jump>DaCoda</Jump><Target>Coda</Target></Directions>",
            Directions {
                targets: vec![DirectionTarget::Coda],
                jumps: vec![DirectionJump::DaCoda],
            },
        ),
        (
            "<Directions><Target>Segno</Target><Target>Ritornello</Target></Directions>",
            Directions {
                targets: vec![DirectionTarget::Segno],
                jumps: Vec::new(),
            },
        ),
        (
            "<Directions><Jump>ToTheEnd</Jump></Directions>",
            Directions::default(),
        ),
        ("<Directions/>", Directions::default()),
        (
            "<Directions kind=\"x\"><Target>Fine</Target></Directions>",
            Directions {
                targets: vec![DirectionTarget::Fine],
                jumps: Vec::new(),
            },
        ),
        (
            "<Directions><Target> Coda </Target></Directions>",
            Directions {
                targets: vec![DirectionTarget::Coda],
                jumps: Vec::new(),
            },
        ),
    ];
    for (element, expected) in cases {
        let score = with_directions(element).read();
        let bar = master_bar(&score);
        assert_eq!(bar.directions, expected, "{element}");
        let kept = payloads(&score, bar.id.0);
        assert_eq!(kept.len(), 1, "{element}");
        assert!(kept[0].contains("guard=\"directions:"), "{kept:?}");
        let text = written(&score);
        assert!(text.contains(&format!("{element}<Bars>")), "{text}");
        assert_eq!(
            read_gp(&write_gp(&score).expect("write")),
            Ok(score.clone())
        );
        let mut edited = score.clone();
        edited.master_bars[0].directions = Directions {
            targets: vec![DirectionTarget::SegnoSegno],
            jumps: vec![DirectionJump::DaSegnoSegno],
        };
        let text = written(&edited);
        assert!(
            text.contains(concat!(
                "<Directions><Target>SegnoSegno</Target>",
                "<Jump>DaSegnoSegno</Jump></Directions><Bars>"
            )),
            "{element}: {text}"
        );
        assert_eq!(text.matches("<Directions").count(), 1, "{text}");
        let mut cleared = score.clone();
        cleared.master_bars[0].directions = Directions::default();
        if !expected.is_empty() {
            assert!(!written(&cleared).contains("Directions"), "{element}");
        }
    }
}

#[test]
fn a_second_directions_element_is_refused() {
    let doubled = "<Directions><Target>Coda</Target></Directions><Directions/>";
    let error = with_directions(doubled).error();
    assert!(error.to_string().contains("Directions"), "{error}");
}

fn fixture(name: &str) -> Score {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/gp")
        .join(name);
    read_gp(&std::fs::read(path).expect("fixture")).expect("fixture reads")
}

#[test]
fn the_dangling_fixture_reports_each_missing_target() {
    let score = fixture("directions-dangling.gp");
    let id = |bar: usize| score.master_bars[bar].id;
    assert_eq!(
        diagnose(&score),
        vec![
            Diagnostic::DanglingJump {
                master_bar: id(1),
                jump: DirectionJump::DaCoda,
                target: DirectionTarget::Coda,
            },
            Diagnostic::DanglingJump {
                master_bar: id(2),
                jump: DirectionJump::DaSegno,
                target: DirectionTarget::Segno,
            },
            Diagnostic::DanglingJump {
                master_bar: id(3),
                jump: DirectionJump::DaCapoAlCoda,
                target: DirectionTarget::Coda,
            },
        ]
    );
}

#[test]
fn the_duplicates_fixture_reports_each_later_target() {
    let score = fixture("directions-duplicates.gp");
    let id = |bar: usize| score.master_bars[bar].id;
    assert_eq!(
        diagnose(&score),
        vec![
            Diagnostic::DuplicateTarget {
                master_bar: id(4),
                target: DirectionTarget::Segno,
            },
            Diagnostic::DuplicateTarget {
                master_bar: id(5),
                target: DirectionTarget::Fine,
            },
        ]
    );
}

#[test]
fn the_ds_coda_fixture_holds_two_jumps_in_gp_order() {
    let score = fixture("directions-ds-coda.gp");
    assert_eq!(
        score.master_bars[3].directions.jumps,
        vec![DirectionJump::DaSegnoAlCoda, DirectionJump::DaCoda]
    );
    assert!(
        written(&score)
            .contains("<Directions><Jump>DaSegnoAlCoda</Jump><Jump>DaCoda</Jump></Directions>")
    );
}
