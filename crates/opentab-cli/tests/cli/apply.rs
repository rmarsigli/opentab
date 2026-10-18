use std::collections::BTreeSet;

use serde_json::{Value, json};

use crate::support::{file_names, new_score, path_str, read_json, run, temp, whole_rest_beat_id};

fn happy_commands(beat: u64) -> String {
    [
        json!({"op": "set_meta", "field": "title", "value": "Etude"}).to_string(),
        json!({"op": "add_note", "beat": beat, "string": 0, "fret": 3}).to_string(),
        json!({"op": "insert_master_bar", "index": 1, "time_signature": {"numerator": 6, "denominator": 8}}).to_string(),
    ]
    .join("\n")
}

fn ids_of(array: &Value) -> Vec<u64> {
    array
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_u64().unwrap())
        .collect()
}

fn element_ids(score: &Value) -> BTreeSet<u64> {
    let mut ids = BTreeSet::new();
    collect_ids(&score["master_bars"], &mut ids);
    collect_ids(&score["tracks"], &mut ids);
    ids
}

fn collect_ids(value: &Value, ids: &mut BTreeSet<u64>) {
    match value {
        Value::Object(map) => {
            if let Some(id) = map.get("id").and_then(Value::as_u64) {
                ids.insert(id);
            }
            map.values().for_each(|inner| collect_ids(inner, ids));
        }
        Value::Array(items) => items.iter().for_each(|inner| collect_ids(inner, ids)),
        _ => {}
    }
}

fn expected_bars(score: &Value) -> Value {
    let bars = score["tracks"][0]["staves"][0]["bars"].as_array().unwrap();
    Value::Array(
        bars.iter()
            .map(|bar| {
                json!({
                    "id": bar["id"],
                    "voices": bar["voices"].as_array().unwrap().iter().map(|voice| json!({
                        "id": voice["id"],
                        "beats": voice["beats"].as_array().unwrap().iter().map(|beat| json!({
                            "id": beat["id"],
                            "notes": ids_of(&beat["notes"])
                        })).collect::<Vec<Value>>()
                    })).collect::<Vec<Value>>()
                })
            })
            .collect(),
    )
}

#[test]
fn new_apply_info_happy_path() {
    let dir = temp();
    let score = new_score(dir.path(), "a.opentab.json");
    let beat = whole_rest_beat_id(&read_json(&score));
    let commands = dir.path().join("c.jsonl");
    std::fs::write(&commands, happy_commands(beat)).unwrap();
    let out = dir.path().join("b.opentab.json");
    let applied = run(&[
        "apply",
        path_str(&score),
        path_str(&commands),
        "--out",
        path_str(&out),
        "--json",
    ]);
    assert_eq!(applied.code, 0, "{}", applied.stderr);
    let before = element_ids(&read_json(&score));
    let after = element_ids(&read_json(&out));
    let expected: Vec<u64> = after.difference(&before).copied().collect();
    let body = applied.json();
    assert_eq!(body["applied"], 3);
    assert_eq!(body["allocated"], json!(expected));
    assert!(expected.len() >= 3, "{expected:?}");
    let allocator_before = read_json(&score)["ids"]["next"].as_u64().unwrap();
    let range: Vec<u64> =
        (allocator_before..read_json(&out)["ids"]["next"].as_u64().unwrap()).collect();
    assert_eq!(body["allocated"], json!(range));

    let info = run(&["info", path_str(&out), "--json"]);
    assert_eq!(info.code, 0, "{}", info.stderr);
    let track_id = read_json(&out)["tracks"][0]["id"].clone();
    assert_eq!(
        info.json(),
        json!({
            "title": "Etude",
            "artist": "",
            "tracks": [{
                "id": track_id,
                "name": "Guitar",
                "kind": "fretted",
                "strings": [64, 59, 55, 50, 45, 40],
                "capo": 0
            }],
            "master_bars": 2,
            "time_signatures": [["4/4", 1], ["6/8", 1]],
            "diagnostics": [],
            "ids": info.json()["ids"].clone()
        })
    );
    assert_eq!(
        info.json()["ids"],
        json!({
            "master_bars": ids_of(&read_json(&out)["master_bars"]),
            "tracks": [{
                "id": track_id,
                "staves": [{
                    "id": read_json(&out)["tracks"][0]["staves"][0]["id"],
                    "bars": expected_bars(&read_json(&out))
                }]
            }]
        })
    );
    let note =
        &read_json(&out)["tracks"][0]["staves"][0]["bars"][0]["voices"][0]["beats"][0]["notes"][0];
    assert_eq!(
        note["kind"],
        json!({"type": "fretted", "string": 0, "fret": 3})
    );
}

#[test]
fn the_input_file_is_untouched_by_apply() {
    let dir = temp();
    let score = new_score(dir.path(), "a.opentab.json");
    let before = std::fs::read(&score).unwrap();
    let commands = dir.path().join("c.jsonl");
    std::fs::write(
        &commands,
        r#"{"op":"set_meta","field":"artist","value":"X"}"#,
    )
    .unwrap();
    let out = dir.path().join("b.opentab.json");
    let result = run(&[
        "apply",
        path_str(&score),
        path_str(&commands),
        "--out",
        path_str(&out),
    ]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(
        result.stdout.contains("applied 1 command"),
        "{}",
        result.stdout
    );
    assert_eq!(std::fs::read(&score).unwrap(), before);
    assert_eq!(
        file_names(dir.path()),
        vec!["a.opentab.json", "b.opentab.json", "c.jsonl"]
    );
}

#[test]
fn apply_may_overwrite_its_output_and_the_input_itself() {
    let dir = temp();
    let score = new_score(dir.path(), "a.opentab.json");
    let commands = dir.path().join("c.jsonl");
    std::fs::write(
        &commands,
        r#"{"op":"set_meta","field":"artist","value":"X"}"#,
    )
    .unwrap();
    let result = run(&[
        "apply",
        path_str(&score),
        path_str(&commands),
        "--out",
        path_str(&score),
    ]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(read_json(&score)["meta"]["artist"], "X");
}

#[test]
fn a_bad_command_on_line_two_exits_2_and_writes_nothing() {
    let dir = temp();
    let score = new_score(dir.path(), "a.opentab.json");
    let beat = whole_rest_beat_id(&read_json(&score));
    let commands = dir.path().join("c.jsonl");
    let lines = [
        json!({"op": "set_meta", "field": "title", "value": "T"}).to_string(),
        json!({"op": "add_note", "beat": 999_999, "string": 0, "fret": 1}).to_string(),
        json!({"op": "add_note", "beat": beat, "string": 0, "fret": 1}).to_string(),
    ];
    std::fs::write(&commands, lines.join("\n")).unwrap();
    let out = dir.path().join("b.opentab.json");

    let human = run(&[
        "apply",
        path_str(&score),
        path_str(&commands),
        "--out",
        path_str(&out),
    ]);
    assert_eq!(human.code, 2);
    assert!(human.stderr.contains("line 2"), "{}", human.stderr);
    assert_eq!(human.stdout, "");
    assert!(!out.exists());

    let machine = run(&[
        "apply",
        path_str(&score),
        path_str(&commands),
        "--out",
        path_str(&out),
        "--json",
    ]);
    assert_eq!(machine.code, 2);
    assert_eq!(machine.stderr, "");
    let body = machine.json();
    assert_eq!(body["error"]["line"], 2);
    assert_eq!(body["error"]["kind"], "command_failed");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not found")
    );
    assert!(!out.exists());
    assert_eq!(file_names(dir.path()), vec!["a.opentab.json", "c.jsonl"]);
}

#[test]
fn a_failing_apply_keeps_an_existing_output_intact() {
    let dir = temp();
    let score = new_score(dir.path(), "a.opentab.json");
    let commands = dir.path().join("c.jsonl");
    std::fs::write(&commands, r#"{"op":"remove_note","note":424242}"#).unwrap();
    let out = dir.path().join("b.opentab.json");
    std::fs::write(&out, "keep me").unwrap();
    let result = run(&[
        "apply",
        path_str(&score),
        path_str(&commands),
        "--out",
        path_str(&out),
    ]);
    assert_eq!(result.code, 2);
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "keep me");
}

#[test]
fn line_numbers_count_blank_and_comment_lines() {
    let dir = temp();
    let score = new_score(dir.path(), "a.opentab.json");
    let commands = dir.path().join("c.jsonl");
    let text = "\u{feff}// header\r\n\r\n{\"op\":\"set_meta\",\"field\":\"title\",\"value\":\"A\"}\r\n// middle\r\n{\"op\":\"remove_note\",\"note\":424242}\r\n";
    std::fs::write(&commands, text).unwrap();
    let out = dir.path().join("b.opentab.json");
    let result = run(&[
        "apply",
        path_str(&score),
        path_str(&commands),
        "--out",
        path_str(&out),
        "--json",
    ]);
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["line"], 5);
    assert!(!out.exists());
}

#[test]
fn malformed_json_on_a_line_reports_that_line() {
    let dir = temp();
    let score = new_score(dir.path(), "a.opentab.json");
    let commands = dir.path().join("c.jsonl");
    std::fs::write(
        &commands,
        "{\"op\":\"set_meta\",\"field\":\"title\",\"value\":\"A\"}\n{\"op\":\"nope\"}\n",
    )
    .unwrap();
    let out = dir.path().join("b.opentab.json");
    let result = run(&[
        "apply",
        path_str(&score),
        path_str(&commands),
        "--out",
        path_str(&out),
        "--json",
    ]);
    assert_eq!(result.code, 2);
    let body: Value = result.json();
    assert_eq!(body["error"]["line"], 2);
    assert_eq!(body["error"]["kind"], "invalid_command");
    assert!(!out.exists());
}

#[test]
fn a_syntax_error_on_a_later_line_is_reported_before_any_command_runs() {
    let dir = temp();
    let score = new_score(dir.path(), "a.opentab.json");
    let commands = dir.path().join("c.jsonl");
    std::fs::write(
        &commands,
        "{\"op\":\"remove_note\",\"note\":424242}\nnot json\n",
    )
    .unwrap();
    let out = dir.path().join("b.opentab.json");
    let result = run(&[
        "apply",
        path_str(&score),
        path_str(&commands),
        "--out",
        path_str(&out),
        "--json",
    ]);
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["line"], 2);
    assert_eq!(result.json()["error"]["kind"], "invalid_command");
}

#[test]
fn an_empty_command_file_applies_zero_commands() {
    let dir = temp();
    let score = new_score(dir.path(), "a.opentab.json");
    let commands = dir.path().join("c.jsonl");
    std::fs::write(&commands, "").unwrap();
    let out = dir.path().join("b.opentab.json");
    let result = run(&[
        "apply",
        path_str(&score),
        path_str(&commands),
        "--out",
        path_str(&out),
        "--json",
    ]);
    assert_eq!(result.code, 0);
    assert_eq!(result.json(), json!({"applied": 0, "allocated": []}));
    assert_eq!(read_json(&out), read_json(&score));
}

#[test]
fn a_missing_or_binary_command_file_exits_2() {
    let dir = temp();
    let score = new_score(dir.path(), "a.opentab.json");
    let out = dir.path().join("b.opentab.json");
    let missing = dir.path().join("nope.jsonl");
    let result = run(&[
        "apply",
        path_str(&score),
        path_str(&missing),
        "--out",
        path_str(&out),
    ]);
    assert_eq!(result.code, 2);
    let binary = dir.path().join("bin.jsonl");
    std::fs::write(&binary, [0xff, 0xfe, 0x00, 0x9f]).unwrap();
    let result = run(&[
        "apply",
        path_str(&score),
        path_str(&binary),
        "--out",
        path_str(&out),
    ]);
    result.assert_no_panic();
    assert_eq!(result.code, 2);
    assert!(!out.exists());
    let machine = run(&[
        "apply",
        path_str(&score),
        path_str(&binary),
        "--out",
        path_str(&out),
        "--json",
    ]);
    let error = &machine.json()["error"];
    assert_eq!(error["kind"], "invalid_input");
    assert!(error.get("line").is_none());
    assert!(error["message"].as_str().unwrap().contains("UTF-8"));
}
