use crate::support::{new_score, path_str, run, temp};

#[test]
fn human_info_is_plain_readable_lines() {
    let dir = temp();
    let path = dir.path().join("a.opentab.json");
    run(&["new", "--out", path_str(&path), "--title", "Demo"]);
    let result = run(&["info", path_str(&path)]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(!result.stdout.contains('\u{1b}'));
    let lines: Vec<&str> = result.stdout.lines().collect();
    assert!(lines.contains(&"title: Demo"), "{}", result.stdout);
    assert!(lines.contains(&"artist: "), "{}", result.stdout);
    assert!(lines.contains(&"tracks: 1"), "{}", result.stdout);
    assert!(
        lines.contains(&"  1: Guitar, fretted, strings 64 59 55 50 45 40, capo 0"),
        "{}",
        result.stdout
    );
    assert!(lines.contains(&"master bars: 1"), "{}", result.stdout);
    assert!(
        lines.contains(&"time signatures: 4/4 x1"),
        "{}",
        result.stdout
    );
    assert!(lines.contains(&"diagnostics: none"), "{}", result.stdout);
}

#[test]
fn info_of_a_fresh_score_has_no_diagnostics() {
    let dir = temp();
    let path = new_score(dir.path(), "a.opentab.json");
    let result = run(&["info", path_str(&path), "--json"]);
    assert_eq!(result.json()["diagnostics"], serde_json::json!([]));
}

#[test]
fn percussion_tracks_report_their_kind() {
    let dir = temp();
    let path = new_score(dir.path(), "a.opentab.json");
    let mut score: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    score["tracks"][0]["instrument"]["kind"] = "percussion".into();
    score["tracks"][0]["tuning"]["strings"] = serde_json::json!([]);
    std::fs::write(&path, score.to_string()).unwrap();
    let result = run(&["info", path_str(&path), "--json"]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    let body = result.json();
    assert_eq!(body["tracks"][0]["kind"], "percussion");
    assert_eq!(body["tracks"][0]["strings"], serde_json::json!([]));
}
