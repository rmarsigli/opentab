use serde_json::{Value, json};

use crate::support::{new_score, path_str, read_json, run, temp, whole_rest_beat_id};

fn failing_sub_kind(score: &std::path::Path, line: &str) -> Value {
    let dir = temp();
    let commands = dir.path().join("c.jsonl");
    std::fs::write(&commands, format!("{line}\n")).unwrap();
    let out = dir.path().join("b.opentab.json");
    let result = run(&[
        "apply",
        path_str(score),
        path_str(&commands),
        "--out",
        path_str(&out),
        "--json",
    ]);
    assert_eq!(result.code, 2, "{}", result.stdout);
    assert!(!out.exists());
    let body = result.json();
    assert_eq!(body["error"]["kind"], "command_failed");
    body["error"]["sub_kind"].clone()
}

#[test]
fn command_failed_carries_the_sub_kind() {
    let dir = temp();
    let score = new_score(dir.path(), "a.opentab.json");
    let beat = whole_rest_beat_id(&read_json(&score));
    assert_eq!(
        failing_sub_kind(&score, r#"{"op":"remove_note","note":424242}"#),
        "not_found"
    );
    let outside = json!({"op": "insert_master_bar", "index": 99, "time_signature": {"numerator": 4, "denominator": 4}});
    assert_eq!(
        failing_sub_kind(&score, &outside.to_string()),
        "index_out_of_range"
    );
    let first = json!({"op": "add_note", "beat": beat, "string": 0, "fret": 1}).to_string();
    let duplicate = format!("{first}\n{first}");
    assert_eq!(failing_sub_kind(&score, &duplicate), "invalid_value");

    let mut percussion = read_json(&score);
    percussion["tracks"][0]["instrument"]["kind"] = "percussion".into();
    percussion["tracks"][0]["tuning"]["strings"] = json!([]);
    let drums = dir.path().join("drums.opentab.json");
    std::fs::write(&drums, percussion.to_string()).unwrap();
    assert_eq!(failing_sub_kind(&drums, &first), "wrong_instrument");
}

#[test]
fn other_errors_have_no_sub_kind() {
    let dir = temp();
    let missing = dir.path().join("nope.opentab.json");
    let result = run(&["info", path_str(&missing), "--json"]);
    assert!(result.json()["error"].get("sub_kind").is_none());
    let usage = run(&["info", "--json"]);
    assert!(usage.json()["error"].get("sub_kind").is_none());
}
