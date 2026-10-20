use opentab_model::{
    DIRECTION_JUMPS, DIRECTION_TARGETS, Diagnostic, DirectionJump, DirectionTarget, Directions,
    JumpDestination, MasterBarId, Score, diagnose, from_json, source_order, to_json,
};
use serde_json::{Value, json};

use crate::support::{four_four, master_bar, quarters};

const SUBSETS: u32 = 32;

struct Expected {
    jump: DirectionJump,
    name: &'static str,
    destination: JumpDestination,
    releases: Option<DirectionTarget>,
}

const fn expected(
    jump: DirectionJump,
    name: &'static str,
    destination: JumpDestination,
    releases: Option<DirectionTarget>,
) -> Expected {
    Expected {
        jump,
        name,
        destination,
        releases,
    }
}

const START: JumpDestination = JumpDestination::Start;
const SEGNO: JumpDestination = JumpDestination::Target(DirectionTarget::Segno);
const SEGNO_SEGNO: JumpDestination = JumpDestination::Target(DirectionTarget::SegnoSegno);
const FINE: Option<DirectionTarget> = Some(DirectionTarget::Fine);
const CODA: Option<DirectionTarget> = Some(DirectionTarget::Coda);
const DOUBLE_CODA: Option<DirectionTarget> = Some(DirectionTarget::DoubleCoda);

const JUMPS: [Expected; 14] = [
    expected(DirectionJump::DaCapo, "da_capo", START, None),
    expected(DirectionJump::DaCapoAlFine, "da_capo_al_fine", START, FINE),
    expected(DirectionJump::DaCapoAlCoda, "da_capo_al_coda", START, CODA),
    expected(
        DirectionJump::DaCapoAlDoubleCoda,
        "da_capo_al_double_coda",
        START,
        DOUBLE_CODA,
    ),
    expected(DirectionJump::DaSegno, "da_segno", SEGNO, None),
    expected(
        DirectionJump::DaSegnoAlFine,
        "da_segno_al_fine",
        SEGNO,
        FINE,
    ),
    expected(
        DirectionJump::DaSegnoAlCoda,
        "da_segno_al_coda",
        SEGNO,
        CODA,
    ),
    expected(
        DirectionJump::DaSegnoAlDoubleCoda,
        "da_segno_al_double_coda",
        SEGNO,
        DOUBLE_CODA,
    ),
    expected(
        DirectionJump::DaSegnoSegno,
        "da_segno_segno",
        SEGNO_SEGNO,
        None,
    ),
    expected(
        DirectionJump::DaSegnoSegnoAlFine,
        "da_segno_segno_al_fine",
        SEGNO_SEGNO,
        FINE,
    ),
    expected(
        DirectionJump::DaSegnoSegnoAlCoda,
        "da_segno_segno_al_coda",
        SEGNO_SEGNO,
        CODA,
    ),
    expected(
        DirectionJump::DaSegnoSegnoAlDoubleCoda,
        "da_segno_segno_al_double_coda",
        SEGNO_SEGNO,
        DOUBLE_CODA,
    ),
    expected(
        DirectionJump::DaCoda,
        "da_coda",
        JumpDestination::Target(DirectionTarget::Coda),
        None,
    ),
    expected(
        DirectionJump::DaDoubleCoda,
        "da_double_coda",
        JumpDestination::Target(DirectionTarget::DoubleCoda),
        None,
    ),
];

struct ToCoda {
    jump: DirectionJump,
    target: DirectionTarget,
}

const TO_CODA: [ToCoda; 2] = [
    ToCoda {
        jump: DirectionJump::DaCoda,
        target: DirectionTarget::Coda,
    },
    ToCoda {
        jump: DirectionJump::DaDoubleCoda,
        target: DirectionTarget::DoubleCoda,
    },
];

struct TargetName {
    target: DirectionTarget,
    name: &'static str,
}

const fn target_name(target: DirectionTarget, name: &'static str) -> TargetName {
    TargetName { target, name }
}

const TARGET_NAMES: [TargetName; 5] = [
    target_name(DirectionTarget::Segno, "segno"),
    target_name(DirectionTarget::SegnoSegno, "segno_segno"),
    target_name(DirectionTarget::Coda, "coda"),
    target_name(DirectionTarget::DoubleCoda, "double_coda"),
    target_name(DirectionTarget::Fine, "fine"),
];

fn bars(count: usize) -> Score {
    let mut score = Score::new();
    for _ in 0..count {
        let master = master_bar(&mut score, 4, 4);
        score.master_bars.push(master);
    }
    score
}

fn subset(mask: u32) -> Vec<DirectionTarget> {
    DIRECTION_TARGETS
        .iter()
        .enumerate()
        .filter(|(bit, _)| mask & (1 << bit) != 0)
        .map(|(_, target)| *target)
        .collect()
}

fn needed(entry: &Expected) -> Vec<DirectionTarget> {
    let mut needs = Vec::new();
    if let JumpDestination::Target(target) = entry.destination {
        needs.push(target);
    }
    needs.extend(entry.releases);
    needs
}

#[test]
fn every_jump_names_its_destination_and_the_target_it_releases() {
    assert_eq!(JUMPS.len(), DIRECTION_JUMPS.len());
    for (entry, listed) in JUMPS.iter().zip(DIRECTION_JUMPS) {
        assert_eq!(entry.jump, listed);
        assert_eq!(
            entry.jump.destination(),
            entry.destination,
            "{}",
            entry.name
        );
        assert_eq!(entry.jump.releases(), entry.releases, "{}", entry.name);
        assert_eq!(entry.jump.needs(), needed(entry), "{}", entry.name);
        let to_coda = matches!(
            entry.jump,
            DirectionJump::DaCoda | DirectionJump::DaDoubleCoda
        );
        assert_eq!(entry.jump.is_return(), !to_coda, "{}", entry.name);
        assert_eq!(serde_json::to_value(entry.jump).unwrap(), json!(entry.name));
        assert_eq!(
            serde_json::from_value::<DirectionJump>(json!(entry.name)).unwrap(),
            entry.jump
        );
    }
    for (named, listed) in TARGET_NAMES.iter().zip(DIRECTION_TARGETS) {
        assert_eq!(named.target, listed);
        assert_eq!(
            serde_json::to_value(named.target).unwrap(),
            json!(named.name)
        );
    }
    assert!(serde_json::from_value::<DirectionJump>(json!("da_fine")).is_err());
    assert!(serde_json::from_value::<DirectionTarget>(json!("Segno")).is_err());
}

#[test]
fn a_jump_is_dangling_exactly_when_a_target_it_needs_is_missing() {
    for entry in &JUMPS {
        for mask in 0..SUBSETS {
            for jump_bar in 0..2usize {
                let mut score = bars(2);
                let targets = subset(mask);
                score.master_bars[1 - jump_bar].directions.targets = targets.clone();
                score.master_bars[jump_bar].directions.jumps = vec![entry.jump];
                let at = score.master_bars[jump_bar].id;
                let mut expected: Vec<Diagnostic> = needed(entry)
                    .into_iter()
                    .filter(|target| !targets.contains(target))
                    .map(|target| Diagnostic::DanglingJump {
                        master_bar: at,
                        jump: entry.jump,
                        target,
                    })
                    .collect();
                if let Some(ToCoda { target, .. }) =
                    TO_CODA.iter().find(|to_coda| to_coda.jump == entry.jump)
                    && targets.contains(target)
                    && jump_bar == 1
                {
                    expected.push(Diagnostic::CodaNotAhead {
                        master_bar: at,
                        jump: entry.jump,
                        target: *target,
                    });
                }
                assert_eq!(diagnose(&score), expected, "{} with {mask:05b}", entry.name);
            }
        }
    }
}

#[test]
fn a_to_coda_is_flagged_exactly_when_no_copy_of_its_target_lies_after_its_bar() {
    for ToCoda { jump, target } in TO_CODA {
        for jump_bar in 0..3usize {
            for mask in 0u32..8 {
                let mut score = bars(3);
                score.master_bars[jump_bar].directions.jumps = vec![jump];
                let places: Vec<usize> = (0..3).filter(|bar| mask & (1 << bar) != 0).collect();
                for place in &places {
                    score.master_bars[*place].directions.targets.push(target);
                }
                let ids: Vec<MasterBarId> = score.master_bars.iter().map(|bar| bar.id).collect();
                let mut expected: Vec<Diagnostic> = places
                    .iter()
                    .take(places.len().saturating_sub(1))
                    .map(|place| Diagnostic::DuplicateTarget {
                        master_bar: ids[*place],
                        target,
                    })
                    .collect();
                if places.is_empty() {
                    expected.push(Diagnostic::DanglingJump {
                        master_bar: ids[jump_bar],
                        jump,
                        target,
                    });
                } else if places.iter().all(|place| *place <= jump_bar) {
                    expected.push(Diagnostic::CodaNotAhead {
                        master_bar: ids[jump_bar],
                        jump,
                        target,
                    });
                }
                assert_eq!(
                    diagnose(&score),
                    expected,
                    "{jump:?} at {jump_bar}, {places:?}"
                );
            }
        }
    }
    let mut forward = bars(3);
    forward.master_bars[0].directions.jumps = vec![DirectionJump::DaSegno];
    forward.master_bars[2].directions.targets = vec![DirectionTarget::Segno];
    assert_eq!(diagnose(&forward), Vec::new());
}

#[test]
fn a_target_in_the_jumps_own_bar_counts() {
    let mut score = bars(1);
    score.master_bars[0].directions = Directions {
        targets: vec![DirectionTarget::Fine, DirectionTarget::Segno],
        jumps: vec![DirectionJump::DaSegnoAlFine],
    };
    assert_eq!(diagnose(&score), Vec::new());
}

#[test]
fn every_copy_of_a_target_but_the_last_is_a_duplicate() {
    let placements: [&[usize]; 5] = [&[0], &[0, 1], &[0, 0], &[1, 2], &[0, 1, 2]];
    for target in DIRECTION_TARGETS {
        for places in placements {
            let mut score = bars(3);
            for place in places {
                score.master_bars[*place].directions.targets.push(target);
            }
            let ids: Vec<MasterBarId> = score.master_bars.iter().map(|bar| bar.id).collect();
            let expected: Vec<Diagnostic> = places
                .iter()
                .take(places.len() - 1)
                .map(|place| Diagnostic::DuplicateTarget {
                    master_bar: ids[*place],
                    target,
                })
                .collect();
            assert_eq!(diagnose(&score), expected, "{target:?} at {places:?}");
        }
    }
}

#[test]
fn directions_round_trip_through_json() {
    let mut score = four_four(&quarters(4));
    score.master_bars[0].directions = Directions {
        targets: vec![DirectionTarget::Coda, DirectionTarget::Segno],
        jumps: vec![DirectionJump::DaSegnoAlCoda, DirectionJump::DaCoda],
    };
    let text = to_json(&score).unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        value["master_bars"][0]["directions"],
        json!({"targets": ["coda", "segno"], "jumps": ["da_segno_al_coda", "da_coda"]})
    );
    assert_eq!(from_json(&text).unwrap(), score);
}

#[test]
fn json_directions_default_to_none_and_are_closed() {
    let score = four_four(&quarters(4));
    let text = to_json(&score).unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    let mut missing = value.clone();
    missing["master_bars"][0]
        .as_object_mut()
        .unwrap()
        .remove("directions");
    assert_eq!(from_json(&missing.to_string()).unwrap(), score);
    let mut extra = value.clone();
    extra["master_bars"][0]["directions"]["labels"] = json!([]);
    assert!(from_json(&extra.to_string()).is_err());
    let mut unknown = value;
    unknown["master_bars"][0]["directions"]["jumps"] = json!(["to_the_end"]);
    assert!(from_json(&unknown.to_string()).is_err());
}

#[test]
fn direction_diagnostics_round_trip_through_json() {
    let diagnostics = [
        Diagnostic::DanglingJump {
            master_bar: bars(1).master_bars[0].id,
            jump: DirectionJump::DaSegnoAlCoda,
            target: DirectionTarget::Coda,
        },
        Diagnostic::DuplicateTarget {
            master_bar: bars(2).master_bars[1].id,
            target: DirectionTarget::Fine,
        },
        Diagnostic::CodaNotAhead {
            master_bar: bars(1).master_bars[0].id,
            jump: DirectionJump::DaDoubleCoda,
            target: DirectionTarget::DoubleCoda,
        },
    ];
    for diagnostic in diagnostics {
        let text = serde_json::to_string(&diagnostic).unwrap();
        assert_eq!(
            serde_json::from_str::<Diagnostic>(&text).unwrap(),
            diagnostic
        );
    }
    let at = bars(1).master_bars[0].id;
    let value = serde_json::to_value(Diagnostic::DanglingJump {
        master_bar: at,
        jump: DirectionJump::DaSegno,
        target: DirectionTarget::Segno,
    })
    .unwrap();
    assert_eq!(
        value,
        json!({"kind": "dangling_jump", "master_bar": at.0.0, "jump": "da_segno", "target": "segno"})
    );
}

#[test]
fn every_copy_of_a_jump_but_the_last_is_a_duplicate() {
    let placements: [&[usize]; 5] = [&[0], &[0, 1], &[1, 1], &[0, 2], &[0, 1, 2]];
    for jump in DIRECTION_JUMPS {
        for places in placements {
            let mut score = bars(3);
            for place in places {
                score.master_bars[*place].directions.jumps.push(jump);
            }
            let ids: Vec<MasterBarId> = score.master_bars.iter().map(|bar| bar.id).collect();
            let expected: Vec<Diagnostic> = places
                .iter()
                .take(places.len() - 1)
                .map(|place| Diagnostic::DuplicateJump {
                    master_bar: ids[*place],
                    jump,
                })
                .collect();
            let found: Vec<Diagnostic> = diagnose(&score)
                .into_iter()
                .filter(|diagnostic| matches!(diagnostic, Diagnostic::DuplicateJump { .. }))
                .collect();
            assert_eq!(found, expected, "{jump:?} at {places:?}");
        }
    }
    let json = serde_json::to_string(&Diagnostic::DuplicateJump {
        master_bar: bars(1).master_bars[0].id,
        jump: DirectionJump::DaCapo,
    })
    .unwrap();
    assert!(json.contains(r#""kind":"duplicate_jump""#), "{json}");
    assert!(json.contains(r#""jump":"da_capo""#), "{json}");
    assert_eq!(
        serde_json::from_str::<Diagnostic>(&json).unwrap(),
        Diagnostic::DuplicateJump {
            master_bar: bars(1).master_bars[0].id,
            jump: DirectionJump::DaCapo,
        }
    );
}

#[test]
fn distinct_names_in_one_bar_are_not_duplicates() {
    let mut score = bars(1);
    score.master_bars[0].directions = Directions {
        targets: vec![
            DirectionTarget::Segno,
            DirectionTarget::Coda,
            DirectionTarget::Fine,
        ],
        jumps: vec![DirectionJump::DaSegnoAlCoda, DirectionJump::DaCoda],
    };
    assert!(diagnose(&score).iter().all(|diagnostic| !matches!(
        diagnostic,
        Diagnostic::DuplicateJump { .. } | Diagnostic::DuplicateTarget { .. }
    )));
}

fn played_sources(directions: &[Directions]) -> Vec<usize> {
    let mut score = Score::new();
    for placed in directions {
        let mut bar = master_bar(&mut score, 4, 4);
        bar.directions = placed.clone();
        score.master_bars.push(bar);
    }
    let order = source_order(&score.master_bars);
    assert!(!order.truncated);
    order.bars.iter().map(|bar| bar.source).collect()
}

fn placed(targets: &[DirectionTarget], jumps: &[DirectionJump]) -> Directions {
    Directions {
        targets: targets.to_vec(),
        jumps: jumps.to_vec(),
    }
}

#[test]
fn the_source_order_follows_a_da_segno_al_coda() {
    let none = Directions::default();
    let order = played_sources(&[
        none.clone(),
        placed(&[DirectionTarget::Segno], &[]),
        placed(&[], &[DirectionJump::DaCoda]),
        placed(&[], &[DirectionJump::DaSegnoAlCoda]),
        placed(&[DirectionTarget::Coda], &[]),
        none,
    ]);
    assert_eq!(order, vec![0, 1, 2, 3, 1, 2, 4, 5]);
    let order = source_order(&[]);
    assert!(order.bars.is_empty() && !order.truncated);
}
