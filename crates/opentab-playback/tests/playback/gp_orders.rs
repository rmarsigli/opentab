use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use opentab_gp::read_gp;
use opentab_playback::play_order;

use crate::reference::reference_order;
use crate::support::{numbers, score_from};

const GP8_ORDERS: &str = include_str!("../data/gp8-play-orders.txt");
const FIXTURE_ORDERS: &str = include_str!("../../../../xtask/src/fixtures/play-orders.json");
const GP8_EXPERIMENTS: usize = 54;
const T4_EXPERIMENTS: usize = 2;
const DIRECTION_FIXTURES: usize = 12;
const REFERENCE_LIMIT: usize = 1 << 16;

pub struct Experiment {
    pub name: String,
    pub bars: usize,
    pub input: String,
    pub played: Vec<usize>,
}

pub fn experiments() -> Vec<Experiment> {
    GP8_ORDERS
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            let [name, bars, input, played] = cells.as_slice() else {
                panic!("malformed row {line}");
            };
            Experiment {
                name: (*name).to_string(),
                bars: bars.parse().expect("bar count"),
                input: (*input).to_string(),
                played: numbers(played),
            }
        })
        .collect()
}

#[test]
fn the_data_holds_every_gp8_experiment() {
    let all = experiments();
    let t1 = all
        .iter()
        .filter(|row| row.name.starts_with("m3-po"))
        .count();
    let t4 = all
        .iter()
        .filter(|row| row.name.starts_with("m3-t4-"))
        .count();
    assert_eq!((t1, t4, all.len()), (GP8_EXPERIMENTS, T4_EXPERIMENTS, 56));
}

#[test]
fn every_gp8_experiment_plays_in_guitar_pros_order() {
    let mut matched = 0;
    for experiment in experiments() {
        let score = score_from(experiment.bars, &experiment.input);
        let order = play_order(&score);
        assert_eq!(order.sources(), experiment.played, "{}", experiment.name);
        let reference: Vec<usize> = reference_order(&score.master_bars, REFERENCE_LIMIT)
            .iter()
            .map(|visit| visit.bar)
            .collect();
        assert_eq!(
            reference, experiment.played,
            "reference {}",
            experiment.name
        );
        matched += 1;
    }
    assert_eq!(matched, GP8_EXPERIMENTS + T4_EXPERIMENTS);
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn every_directions_fixture_plays_in_guitar_pros_order() {
    let predicted: BTreeMap<String, Vec<Vec<usize>>> =
        serde_json::from_str(FIXTURE_ORDERS).expect("play orders parse");
    assert_eq!(predicted.len(), DIRECTION_FIXTURES);
    for (name, orders) in &predicted {
        let path = root().join("fixtures/gp").join(name);
        let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{name}: {error}"));
        let score = read_gp(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        let order = play_order(&score);
        assert_eq!(vec![order.sources()], *orders, "{name}");
        let reference: Vec<usize> = reference_order(&score.master_bars, REFERENCE_LIMIT)
            .iter()
            .map(|visit| visit.bar)
            .collect();
        assert_eq!(vec![reference], *orders, "reference {name}");
    }
}
