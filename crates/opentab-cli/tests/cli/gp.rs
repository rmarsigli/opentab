use std::path::{Path, PathBuf};

use opentab_gp::read_gp;
use opentab_model::{Score, from_json};
use serde_json::{Value, json};

use crate::support::{path_str, read_json, run, temp};

const ZIP_MAGIC: &[u8] = b"PK\x03\x04";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/gp")
        .join(name)
}

fn gp_score(path: &Path) -> Score {
    read_gp(&std::fs::read(path).expect("read gp")).expect("valid gp")
}

fn json_score(path: &Path) -> Score {
    from_json(&std::fs::read_to_string(path).expect("read json")).expect("valid json")
}

fn commands_file(dir: &Path) -> PathBuf {
    let path = dir.join("edits.jsonl");
    let lines = [
        json!({"op": "set_meta", "field": "title", "value": "Edited"}).to_string(),
        json!({"op": "insert_master_bar", "index": 1, "time_signature": {"numerator": 3, "denominator": 4}}).to_string(),
    ];
    std::fs::write(&path, lines.join("\n")).unwrap();
    path
}

#[test]
fn info_reads_a_gp_fixture() {
    let fixture = fixture("multi-track.gp");
    let result = run(&["info", path_str(&fixture), "--json"]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    let mut body = result.json();
    body.as_object_mut().unwrap().remove("ids");
    let strings = json!([64, 59, 55, 50, 45, 40]);
    assert_eq!(
        body,
        json!({
            "title": "Several tracks",
            "artist": "",
            "tracks": [
                {"id": 3, "name": "Lead guitar", "kind": "fretted", "strings": strings, "capo": 0},
                {"id": 25, "name": "Rhythm guitar", "kind": "fretted", "strings": strings, "capo": 0},
                {"id": 54, "name": "Bass", "kind": "fretted", "strings": [43, 38, 33, 28], "capo": 0},
                {"id": 68, "name": "Keys", "kind": "fretted", "strings": strings, "capo": 0}
            ],
            "master_bars": 2,
            "time_signatures": [["4/4", 2]],
            "diagnostics": []
        })
    );
}

#[test]
fn human_info_reads_a_gp_fixture() {
    let fixture = fixture("multi-track.gp");
    let result = run(&["info", path_str(&fixture)]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(result.stdout.contains("title: Several tracks"));
    assert!(result.stdout.contains("tracks: 4"));
}

#[test]
fn converting_a_gp_to_json_and_back_keeps_the_model() {
    let dir = temp();
    for name in [
        "multi-track.gp",
        "metadata.gp",
        "note-effects.gp",
        "tempo.gp",
    ] {
        let source = fixture(name);
        let middle = dir.path().join("middle.opentab.json");
        let back = dir.path().join("back.gp");
        let first = run(&["convert", path_str(&source), path_str(&middle)]);
        assert_eq!(first.code, 0, "{name}: {}", first.stderr);
        let second = run(&["convert", path_str(&middle), path_str(&back)]);
        assert_eq!(second.code, 0, "{name}: {}", second.stderr);
        assert!(std::fs::read(&back).unwrap().starts_with(ZIP_MAGIC));
        assert_eq!(json_score(&middle), gp_score(&source), "{name}");
        assert_eq!(gp_score(&back), gp_score(&source), "{name}");
    }
}

#[test]
fn convert_between_equal_formats_is_allowed() {
    let dir = temp();
    let source = fixture("durations.gp");
    let again = dir.path().join("again.gp");
    assert_eq!(
        run(&["convert", path_str(&source), path_str(&again)]).code,
        0
    );
    assert_eq!(gp_score(&again), gp_score(&source));
    let first = dir.path().join("a.opentab.json");
    let second = dir.path().join("b.opentab.json");
    run(&["convert", path_str(&source), path_str(&first)]);
    let result = run(&["convert", path_str(&first), path_str(&second)]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        std::fs::read(&first).unwrap(),
        std::fs::read(&second).unwrap()
    );
}

#[test]
fn convert_json_output_names_both_paths() {
    let dir = temp();
    let source = fixture("metadata.gp");
    let out = dir.path().join("m.opentab.json");
    let result = run(&["convert", path_str(&source), path_str(&out), "--json"]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        result.json(),
        json!({"converted": {"from": path_str(&source), "to": path_str(&out)}})
    );
    let human = run(&["convert", path_str(&source), path_str(&out)]);
    assert_eq!(
        human.stdout.trim_end(),
        format!("converted {} to {}", path_str(&source), path_str(&out))
    );
}

#[test]
fn convert_replaces_an_existing_output_atomically() {
    let dir = temp();
    let source = fixture("metadata.gp");
    let out = dir.path().join("m.opentab.json");
    std::fs::write(&out, "old").unwrap();
    let result = run(&["convert", path_str(&source), path_str(&out)]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(json_score(&out), gp_score(&source));
    let names: Vec<String> = crate::support::file_names(dir.path());
    assert_eq!(names, ["m.opentab.json"]);
}

#[test]
fn apply_on_a_gp_writes_a_gp() {
    let dir = temp();
    let commands = commands_file(dir.path());
    let source = fixture("multi-track.gp");
    let out = dir.path().join("edited.gp");
    let result = run(&[
        "apply",
        path_str(&source),
        path_str(&commands),
        "--out",
        path_str(&out),
        "--json",
    ]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(result.json()["applied"], 2);
    let bytes = std::fs::read(&out).unwrap();
    assert!(bytes.starts_with(ZIP_MAGIC));
    let edited = read_gp(&bytes).expect("edited gp reads");
    assert_eq!(edited.meta.title, "Edited");
    assert_eq!(edited.master_bars.len(), 3);
    assert_eq!(edited.tracks.len(), 4);
}

#[test]
fn apply_on_a_gp_can_write_json() {
    let dir = temp();
    let commands = commands_file(dir.path());
    let source = fixture("multi-track.gp");
    let out = dir.path().join("edited.opentab.json");
    let result = run(&[
        "apply",
        path_str(&source),
        path_str(&commands),
        "--out",
        path_str(&out),
    ]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    let written: Value = read_json(&out);
    assert_eq!(written["meta"]["title"], "Edited");
    assert!(!std::fs::read(&out).unwrap().starts_with(ZIP_MAGIC));
}

#[test]
fn apply_on_json_can_write_a_gp() {
    let dir = temp();
    let commands = commands_file(dir.path());
    let json_path = dir.path().join("a.opentab.json");
    let source = fixture("multi-track.gp");
    run(&["convert", path_str(&source), path_str(&json_path)]);
    let out = dir.path().join("edited.gp");
    let result = run(&[
        "apply",
        path_str(&json_path),
        path_str(&commands),
        "--out",
        path_str(&out),
    ]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(std::fs::read(&out).unwrap().starts_with(ZIP_MAGIC));
    assert_eq!(gp_score(&out).meta.title, "Edited");
}

#[test]
fn apply_may_overwrite_its_gp_input() {
    let dir = temp();
    let commands = commands_file(dir.path());
    let path = dir.path().join("song.gp");
    std::fs::copy(fixture("tempo.gp"), &path).unwrap();
    let result = run(&[
        "apply",
        path_str(&path),
        path_str(&commands),
        "--out",
        path_str(&path),
    ]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(gp_score(&path).meta.title, "Edited");
}

#[test]
fn a_writer_refusal_is_reported_as_unsupported() {
    let dir = temp();
    let blank = dir.path().join("blank.opentab.json");
    assert_eq!(run(&["new", "--out", path_str(&blank)]).code, 0);
    let mut score = read_json(&blank);
    score["meta"]["title"] = "line\rbreak".into();
    std::fs::write(&blank, score.to_string()).unwrap();
    let out = dir.path().join("out.gp");
    let result = run(&["convert", path_str(&blank), path_str(&out), "--json"]);
    assert_eq!(result.code, 2, "{}", result.stdout);
    let body = result.json();
    assert_eq!(body["error"]["kind"], "unsupported");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("U+000D"), "{message}");
    assert!(message.contains(path_str(&out)), "{message}");
    assert!(message.contains("title"), "{message}");
    assert!(!out.exists());
    assert_eq!(
        crate::support::file_names(dir.path()),
        ["blank.opentab.json"]
    );
}

#[test]
fn removing_the_last_master_bar_cannot_be_saved_as_gp() {
    let dir = temp();
    let path = dir.path().join("song.gp");
    std::fs::copy(fixture("tempo.gp"), &path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let count = gp_score(&path).master_bars.len();
    let commands = dir.path().join("c.jsonl");
    let lines: Vec<String> = (0..count)
        .map(|_| json!({"op": "remove_master_bar", "index": 0}).to_string())
        .collect();
    std::fs::write(&commands, lines.join("\n")).unwrap();
    let result = run(&[
        "apply",
        path_str(&path),
        path_str(&commands),
        "--out",
        path_str(&path),
        "--json",
    ]);
    assert_eq!(result.code, 2, "{}", result.stdout);
    let body = result.json();
    assert_eq!(body["error"]["kind"], "unsupported");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("master_bars: the score has no master bars"),
        "{message}"
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn a_refused_apply_keeps_the_existing_gp() {
    let dir = temp();
    let path = dir.path().join("song.gp");
    std::fs::copy(fixture("tempo.gp"), &path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let commands = dir.path().join("c.jsonl");
    let title = json!({"op": "set_meta", "field": "title", "value": "a\rb"});
    std::fs::write(&commands, title.to_string()).unwrap();
    let result = run(&[
        "apply",
        path_str(&path),
        path_str(&commands),
        "--out",
        path_str(&path),
        "--json",
    ]);
    assert_eq!(result.code, 2, "{}", result.stdout);
    assert_eq!(result.json()["error"]["kind"], "unsupported");
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn a_garbage_gp_is_invalid_input() {
    let dir = temp();
    let path = dir.path().join("garbage.gp");
    std::fs::write(&path, "this is not a zip archive").unwrap();
    let out = dir.path().join("out.opentab.json");
    for args in [
        vec!["info", path_str(&path), "--json"],
        vec!["convert", path_str(&path), path_str(&out), "--json"],
    ] {
        let result = run(&args);
        assert_eq!(result.code, 2, "{}", result.stdout);
        let body = result.json();
        assert_eq!(body["error"]["kind"], "invalid_input");
        let message = body["error"]["message"].as_str().unwrap();
        assert!(message.contains(path_str(&path)), "{message}");
        assert!(message.contains("archive"), "{message}");
    }
    assert!(!out.exists());
}

#[test]
fn a_missing_gp_is_an_io_error() {
    let dir = temp();
    let path = dir.path().join("absent.gp");
    let result = run(&["info", path_str(&path), "--json"]);
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["kind"], "io");
}

#[test]
fn convert_with_an_unknown_suffix_lists_both_formats() {
    let dir = temp();
    let source = fixture("metadata.gp");
    let out = dir.path().join("song.txt");
    let result = run(&["convert", path_str(&source), path_str(&out), "--json"]);
    assert_eq!(result.code, 2);
    let body = result.json();
    assert_eq!(body["error"]["kind"], "unsupported_format");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains(".gp, .opentab.json"), "{message}");
    assert!(!out.exists());
    let input = dir.path().join("song.txt");
    std::fs::write(&input, "x").unwrap();
    let reverse = dir.path().join("o.gp");
    assert_eq!(
        run(&["convert", path_str(&input), path_str(&reverse)]).code,
        2
    );
    assert!(!reverse.exists());
}

#[test]
fn a_bare_gp_suffix_is_not_a_name() {
    let dir = temp();
    let source = fixture("metadata.gp");
    let out = dir.path().join(".gp");
    assert_eq!(run(&["convert", path_str(&source), path_str(&out)]).code, 2);
    assert!(!out.exists());
}

#[test]
fn new_stays_json_only() {
    let dir = temp();
    let out = dir.path().join("song.gp");
    let result = run(&["new", "--out", path_str(&out), "--json"]);
    assert_eq!(result.code, 2);
    let message = result.json()["error"]["message"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        message.ends_with("supported formats: .opentab.json"),
        "{message}"
    );
    assert!(!out.exists());
}
