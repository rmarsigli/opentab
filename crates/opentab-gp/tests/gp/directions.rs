use opentab_command::{Command, apply};
use opentab_gp::{read_gp, write_gp};
use opentab_model::{
    DIRECTION_JUMPS, DIRECTION_TARGETS, Diagnostic, DirectionJump, DirectionTarget, Directions,
    Marker, Score, diagnose,
};

use crate::anchors::gpif_text;
use crate::inventory::master_bar_children;
use crate::support::{Doc, master_bar, payloads};

struct Named<T> {
    name: &'static str,
    value: T,
}

const fn named<T>(name: &'static str, value: T) -> Named<T> {
    Named { name, value }
}

const TARGET_NAMES: [Named<DirectionTarget>; 5] = [
    named("Segno", DirectionTarget::Segno),
    named("SegnoSegno", DirectionTarget::SegnoSegno),
    named("Coda", DirectionTarget::Coda),
    named("DoubleCoda", DirectionTarget::DoubleCoda),
    named("Fine", DirectionTarget::Fine),
];

const JUMP_NAMES: [Named<DirectionJump>; 14] = [
    named("DaCapo", DirectionJump::DaCapo),
    named("DaCapoAlFine", DirectionJump::DaCapoAlFine),
    named("DaCapoAlCoda", DirectionJump::DaCapoAlCoda),
    named("DaCapoAlDoubleCoda", DirectionJump::DaCapoAlDoubleCoda),
    named("DaSegno", DirectionJump::DaSegno),
    named("DaSegnoAlFine", DirectionJump::DaSegnoAlFine),
    named("DaSegnoAlCoda", DirectionJump::DaSegnoAlCoda),
    named("DaSegnoAlDoubleCoda", DirectionJump::DaSegnoAlDoubleCoda),
    named("DaSegnoSegno", DirectionJump::DaSegnoSegno),
    named("DaSegnoSegnoAlFine", DirectionJump::DaSegnoSegnoAlFine),
    named("DaSegnoSegnoAlCoda", DirectionJump::DaSegnoSegnoAlCoda),
    named(
        "DaSegnoSegnoAlDoubleCoda",
        DirectionJump::DaSegnoSegnoAlDoubleCoda,
    ),
    named("DaCoda", DirectionJump::DaCoda),
    named("DaDoubleCoda", DirectionJump::DaDoubleCoda),
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
    for Named {
        name,
        value: target,
    } in TARGET_NAMES
    {
        let element = format!("<Directions><Target>{name}</Target></Directions>");
        let score = with_directions(&element).read();
        let bar = master_bar(&score);
        assert_eq!(bar.directions.targets, vec![target], "{name}");
        assert!(bar.directions.jumps.is_empty(), "{name}");
        assert!(payloads(&score, bar.id.0).is_empty(), "{name}");
        assert_eq!(directions_of(&written(&score)), element, "{name}");
    }
    for Named { name, value: jump } in JUMP_NAMES {
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
fn the_duplicates_fixture_reports_each_copy_guitar_pro_drops() {
    let score = fixture("directions-duplicates.gp");
    let id = |bar: usize| score.master_bars[bar].id;
    assert_eq!(
        diagnose(&score),
        vec![
            Diagnostic::DuplicateTarget {
                master_bar: id(1),
                target: DirectionTarget::Segno,
            },
            Diagnostic::DuplicateTarget {
                master_bar: id(2),
                target: DirectionTarget::Fine,
            },
        ]
    );
}

#[test]
fn fixtures_with_a_repeated_jump_report_the_copy_guitar_pro_drops() {
    for (name, bar, jump) in [
        ("directions-endings.gp", 1, DirectionJump::DaCapo),
        ("directions-coda-free.gp", 1, DirectionJump::DaCoda),
    ] {
        let score = fixture(name);
        let duplicates: Vec<Diagnostic> = diagnose(&score)
            .into_iter()
            .filter(|diagnostic| matches!(diagnostic, Diagnostic::DuplicateJump { .. }))
            .collect();
        assert_eq!(
            duplicates,
            vec![Diagnostic::DuplicateJump {
                master_bar: score.master_bars[bar].id,
                jump,
            }],
            "{name}"
        );
    }
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

const TRIPLET_FEEL: &str = "<TripletFeel>Triplet8th</TripletFeel>";
const BARS_THEN_PROPERTIES: &str = "<Bars>0</Bars><XProperties><XProperty id=\"1124139010\"><Int>7</Int></XProperty></XProperties></MasterBar>";

fn with_kept_children() -> Score {
    let gpif = with_directions(TRIPLET_FEEL)
        .render()
        .replace("<Bars>0</Bars></MasterBar>", BARS_THEN_PROPERTIES);
    crate::support::read_gpif(&gpif)
}

fn children(score: &Score) -> Vec<String> {
    let text = gpif_text(&write_gp(score).expect("write"));
    master_bar_children(&text).remove(0)
}

#[test]
fn edits_that_add_a_master_bar_child_keep_the_documented_child_order() {
    let score = with_kept_children();
    assert_eq!(
        children(&score),
        ["Key", "Time", "TripletFeel", "Bars", "XProperties"]
    );
    let mut directed = score.clone();
    apply(
        &mut directed,
        &Command::SetDirections {
            master_bar: 0,
            directions: Directions {
                targets: vec![DirectionTarget::Segno],
                jumps: Vec::new(),
            },
        },
    )
    .expect("set_directions");
    assert_eq!(
        children(&directed),
        [
            "Key",
            "Time",
            "TripletFeel",
            "Directions",
            "Bars",
            "XProperties"
        ]
    );
    let mut marked = score.clone();
    marked.master_bars[0].marker = Some(Marker {
        text: "Verse".to_string(),
        letter: "A".to_string(),
    });
    marked.master_bars[0].double_bar = true;
    marked.master_bars[0].repeat_start = true;
    marked.master_bars[0].alternate_endings = 1;
    assert_eq!(
        children(&marked),
        [
            "Key",
            "Time",
            "Repeat",
            "Section",
            "AlternateEndings",
            "TripletFeel",
            "DoubleBar",
            "Bars",
            "XProperties"
        ]
    );
    for edited in [&directed, &marked] {
        let again = read_gp(&write_gp(edited).expect("write")).expect("read");
        assert_eq!(children(&again), children(edited));
    }
}
