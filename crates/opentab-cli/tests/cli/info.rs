use crate::support::{new_score, path_str, read_json, run, temp};

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

#[test]
fn json_info_lists_every_id_in_tree_order() {
    let dir = temp();
    let path = new_score(dir.path(), "a.opentab.json");
    let score = read_json(&path);
    let result = run(&["info", path_str(&path), "--json"]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    let track = &score["tracks"][0];
    let staff = &track["staves"][0];
    let bar = &staff["bars"][0];
    let voice = &bar["voices"][0];
    let beat = &voice["beats"][0];
    assert_eq!(
        result.json()["ids"],
        serde_json::json!({
            "master_bars": [score["master_bars"][0]["id"].clone()],
            "tracks": [{
                "id": track["id"],
                "staves": [{
                    "id": staff["id"],
                    "bars": [{
                        "id": bar["id"],
                        "voices": [{
                            "id": voice["id"],
                            "beats": [{"id": beat["id"], "notes": []}]
                        }]
                    }]
                }]
            }]
        })
    );
}

#[test]
fn human_info_prints_ids_only_with_the_flag() {
    let dir = temp();
    let path = new_score(dir.path(), "a.opentab.json");
    let plain = run(&["info", path_str(&path)]);
    assert!(!plain.stdout.contains("ids:"), "{}", plain.stdout);
    let with_ids = run(&["info", path_str(&path), "--ids"]);
    assert_eq!(with_ids.code, 0, "{}", with_ids.stderr);
    assert!(
        plain
            .stdout
            .lines()
            .all(|line| with_ids.stdout.contains(line))
    );
    let tree: Vec<&str> = with_ids
        .stdout
        .lines()
        .skip_while(|line| *line != "ids:")
        .collect();
    assert_eq!(tree.len(), 7, "{}", with_ids.stdout);
    assert!(tree[1].starts_with("  master bars: "));
    assert!(tree[2].starts_with("  track "));
    assert!(tree[3].starts_with("    staff "));
    assert!(tree[4].starts_with("      bar "));
    assert!(tree[5].starts_with("        voice "));
    assert!(tree[6].starts_with("          beat "));
}

#[test]
fn json_info_with_the_ids_flag_is_unchanged() {
    let dir = temp();
    let path = new_score(dir.path(), "a.opentab.json");
    let plain = run(&["info", path_str(&path), "--json"]);
    let flagged = run(&["info", path_str(&path), "--json", "--ids"]);
    assert_eq!(plain.stdout, flagged.stdout);
}
