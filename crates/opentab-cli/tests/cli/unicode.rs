use serde_json::json;

use crate::support::{new_score, path_str, read_json, run, temp};

const ACCENTED: &str =
    "Sérénade à la lune \u{1f3b8}\u{1f525} 日本語 \u{200d}\u{1f468}\u{200d}\u{1f3a4}";

fn apply_title(title: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = temp();
    let score = new_score(dir.path(), "a.opentab.json");
    let commands = dir.path().join("c.jsonl");
    let line = json!({"op": "set_meta", "field": "title", "value": title}).to_string();
    std::fs::write(&commands, line).unwrap();
    let out = dir.path().join("b.opentab.json");
    let result = run(&[
        "apply",
        path_str(&score),
        path_str(&commands),
        "--out",
        path_str(&out),
    ]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    (dir, out)
}

#[test]
fn accents_and_emoji_round_trip_through_apply_and_info() {
    let (_dir, out) = apply_title(ACCENTED);
    assert_eq!(read_json(&out)["meta"]["title"], ACCENTED);
    let machine = run(&["info", path_str(&out), "--json"]);
    assert_eq!(machine.json()["title"], ACCENTED);
    let human = run(&["info", path_str(&out)]);
    assert_eq!(human.code, 0);
    assert!(human.stdout.contains(ACCENTED), "{}", human.stdout);
}

#[test]
fn a_ten_kilobyte_title_round_trips() {
    let title: String = "Ação\u{1f3b8}".repeat(1_700);
    assert!(title.len() > 10_000);
    let (_dir, out) = apply_title(&title);
    assert_eq!(read_json(&out)["meta"]["title"], title.as_str());
    let machine = run(&["info", path_str(&out), "--json"]);
    assert_eq!(machine.json()["title"], title.as_str());
    let human = run(&["info", path_str(&out)]);
    assert!(human.stdout.contains(&title));
}

#[test]
fn new_title_with_accents_round_trips() {
    let dir = temp();
    let path = dir.path().join("t.opentab.json");
    let result = run(&["new", "--out", path_str(&path), "--title", ACCENTED]);
    assert_eq!(result.code, 0);
    let info = run(&["info", path_str(&path), "--json"]);
    assert_eq!(info.json()["title"], ACCENTED);
}

#[test]
fn paths_with_spaces_and_accents_work() {
    let dir = temp();
    let folder = dir.path().join("pasta com espaço é");
    std::fs::create_dir(&folder).unwrap();
    let path = new_score(&folder, "música nova.opentab.json");
    let info = run(&["info", path_str(&path), "--json"]);
    assert_eq!(info.code, 0, "{}", info.stderr);
}

#[cfg(target_os = "linux")]
#[test]
fn a_non_utf8_path_is_handled() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let dir = temp();
    let name = OsStr::from_bytes(b"bad-\xff-name.opentab.json");
    let path = dir.path().join(name);
    let created =
        crate::support::run_os(&[OsStr::new("new"), OsStr::new("--out"), path.as_os_str()]);
    assert_eq!(created.code, 0, "{}", created.stderr);
    assert!(path.exists());
    let info =
        crate::support::run_os(&[OsStr::new("info"), path.as_os_str(), OsStr::new("--json")]);
    assert_eq!(info.code, 0, "{}", info.stderr);
}
