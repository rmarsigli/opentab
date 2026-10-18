use serde_json::Value;

use crate::support::{new_score, path_str, read_json, run, temp};

fn assert_rejected(bytes: &[u8], needle: &str) {
    let dir = temp();
    let path = dir.path().join("x.opentab.json");
    std::fs::write(&path, bytes).unwrap();
    let human = run(&["info", path_str(&path)]);
    human.assert_no_panic();
    assert_eq!(human.code, 2, "{}", human.stderr);
    assert!(human.stderr.contains(needle), "{}", human.stderr);
    assert_eq!(human.stdout, "");
    let machine = run(&["info", path_str(&path), "--json"]);
    machine.assert_no_panic();
    assert_eq!(machine.code, 2);
    assert_eq!(machine.stderr, "");
    let body = machine.json();
    assert_eq!(body["error"]["kind"], "invalid_score");
    assert!(body["error"]["message"].as_str().unwrap().contains(needle));
    assert!(body["error"].get("line").is_none());
}

#[test]
fn garbage_bytes_exit_2() {
    assert_rejected(&[0xff, 0xfe, 0x00, 0x9f, 0x80, 0x01], "UTF-8");
}

#[test]
fn garbage_text_exits_2() {
    assert_rejected(b"this is not a score", "invalid opentab JSON");
}

#[test]
fn an_empty_file_exits_2() {
    assert_rejected(b"", "invalid opentab JSON");
}

#[test]
fn version_two_exits_2() {
    assert_rejected(
        br#"{"opentab_json": 2}"#,
        "unsupported opentab_json version 2",
    );
}

#[test]
fn a_missing_version_exits_2() {
    assert_rejected(b"{}", "missing \"opentab_json\"");
}

#[test]
fn a_truncated_file_exits_2() {
    let dir = temp();
    let source = new_score(dir.path(), "a.opentab.json");
    let text = std::fs::read(&source).unwrap();
    assert_rejected(&text[..text.len() / 2], "invalid opentab JSON");
}

#[test]
fn missing_fields_and_wrong_types_exit_2() {
    let dir = temp();
    let source = new_score(dir.path(), "a.opentab.json");
    let mut score = read_json(&source);
    score.as_object_mut().unwrap().remove("master_bars");
    assert_rejected(score.to_string().as_bytes(), "master_bars");
    let mut score = read_json(&source);
    score["tracks"][0]["capo"] = Value::String("two".into());
    assert_rejected(score.to_string().as_bytes(), "expected u8");
}

#[test]
fn deeply_nested_input_exits_2_without_crashing() {
    let nested = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
    assert_rejected(nested.as_bytes(), "invalid opentab JSON");
}

#[test]
fn dangling_ids_do_not_crash_info_and_are_diagnosed() {
    let dir = temp();
    let source = new_score(dir.path(), "a.opentab.json");
    let mut score = read_json(&source);
    score["tracks"][0]["staves"][0]["bars"][0]["voices"][0]["beats"][0]["id"] = 99_999.into();
    let path = dir.path().join("edited.opentab.json");
    std::fs::write(&path, score.to_string()).unwrap();
    let result = run(&["info", path_str(&path), "--json"]);
    result.assert_no_panic();
    assert_eq!(result.code, 0);
    let diagnostics = result.json()["diagnostics"].clone();
    assert_eq!(diagnostics[0]["kind"], "id_above_allocator");
}

#[test]
fn zero_tracks_and_zero_master_bars_work() {
    let dir = temp();
    let source = new_score(dir.path(), "a.opentab.json");
    let mut score = read_json(&source);
    score["tracks"] = Value::Array(Vec::new());
    score["master_bars"] = Value::Array(Vec::new());
    let path = dir.path().join("empty.opentab.json");
    std::fs::write(&path, score.to_string()).unwrap();
    let result = run(&["info", path_str(&path), "--json"]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    let body = result.json();
    assert_eq!(body["master_bars"], 0);
    assert_eq!(body["tracks"], Value::Array(Vec::new()));
    assert_eq!(body["time_signatures"], Value::Array(Vec::new()));
    let human = run(&["info", path_str(&path)]);
    assert_eq!(human.code, 0);
}

#[test]
fn a_staff_bar_count_mismatch_is_diagnosed() {
    let dir = temp();
    let source = new_score(dir.path(), "a.opentab.json");
    let mut score = read_json(&source);
    score["tracks"][0]["staves"][0]["bars"] = Value::Array(Vec::new());
    let path = dir.path().join("short.opentab.json");
    std::fs::write(&path, score.to_string()).unwrap();
    let result = run(&["info", path_str(&path), "--json"]);
    assert_eq!(result.code, 0);
    assert_eq!(
        result.json()["diagnostics"][0]["kind"],
        "staff_bar_count_mismatch"
    );
    let human = run(&["info", path_str(&path)]);
    assert!(
        human.stdout.contains("staff_bar_count_mismatch"),
        "{}",
        human.stdout
    );
}

#[test]
fn a_missing_input_file_exits_2() {
    let dir = temp();
    let path = dir.path().join("nope.opentab.json");
    let result = run(&["info", path_str(&path), "--json"]);
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["kind"], "io");
}
