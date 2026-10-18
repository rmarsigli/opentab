use serde_json::json;

use crate::support::{file_names, new_score, path_str, read_json, run, temp};

#[test]
fn new_writes_the_briefed_score() {
    let dir = temp();
    let path = dir.path().join("a.opentab.json");
    let result = run(&["new", "--out", path_str(&path), "--title", "Demo"]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    let score = read_json(&path);
    assert_eq!(score["opentab_json"], 1);
    assert_eq!(score["meta"]["title"], "Demo");
    assert_eq!(score["master_bars"].as_array().unwrap().len(), 1);
    let master_bar = &score["master_bars"][0];
    assert_eq!(
        master_bar["time_signature"],
        json!({"numerator": 4, "denominator": 4})
    );
    assert_eq!(master_bar["tempo_changes"][0]["bpm"], 120.0);
    let tracks = score["tracks"].as_array().unwrap();
    assert_eq!(tracks.len(), 1);
    let track = &tracks[0];
    assert_eq!(track["name"], "Guitar");
    assert_eq!(
        track["instrument"],
        json!({"kind": "fretted", "midi_program": 25})
    );
    assert_eq!(track["tuning"]["strings"], json!([64, 59, 55, 50, 45, 40]));
    assert_eq!(track["capo"], 0);
    assert_eq!(track["staves"].as_array().unwrap().len(), 1);
    let voices = track["staves"][0]["bars"][0]["voices"].as_array().unwrap();
    assert_eq!(voices.len(), 1);
    let beats = voices[0]["beats"].as_array().unwrap();
    assert_eq!(beats.len(), 1);
    assert_eq!(beats[0]["duration"]["value"], "whole");
    assert_eq!(beats[0]["notes"], json!([]));
}

#[test]
fn new_reports_json_and_leaves_no_temp_files() {
    let dir = temp();
    let path = dir.path().join("a.opentab.json");
    let result = run(&["new", "--out", path_str(&path), "--json"]);
    assert_eq!(result.code, 0);
    assert_eq!(result.json(), json!({"created": path_str(&path)}));
    assert_eq!(file_names(dir.path()), vec!["a.opentab.json".to_string()]);
}

#[test]
fn new_without_title_has_an_empty_title() {
    let dir = temp();
    let path = new_score(dir.path(), "a.opentab.json");
    assert_eq!(read_json(&path)["meta"]["title"], "");
}

#[test]
fn new_never_overwrites_an_existing_file() {
    let dir = temp();
    let path = dir.path().join("a.opentab.json");
    std::fs::write(&path, "precious").unwrap();
    let result = run(&["new", "--out", path_str(&path)]);
    assert_eq!(result.code, 2);
    assert!(
        result.stderr.contains("already exists"),
        "{}",
        result.stderr
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "precious");
    assert_eq!(file_names(dir.path()), vec!["a.opentab.json".to_string()]);
}

#[test]
fn new_into_a_missing_directory_is_an_input_error() {
    let dir = temp();
    let path = dir.path().join("missing").join("a.opentab.json");
    let result = run(&["new", "--out", path_str(&path), "--json"]);
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["kind"], "io");
    assert_eq!(result.stderr, "");
}
