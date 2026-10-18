use crate::support::{new_score, path_str, run, temp};

#[test]
fn an_unknown_suffix_exits_2_and_lists_the_supported_formats() {
    let dir = temp();
    let path = dir.path().join("song.txt");
    std::fs::write(&path, "x").unwrap();
    let human = run(&["info", path_str(&path)]);
    assert_eq!(human.code, 2);
    assert!(
        human.stderr.contains(".gp, .opentab.json"),
        "{}",
        human.stderr
    );
    let machine = run(&["info", path_str(&path), "--json"]);
    assert_eq!(machine.code, 2);
    let body = machine.json();
    assert_eq!(body["error"]["kind"], "unsupported_format");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains(".gp, .opentab.json")
    );
}

#[test]
fn a_plain_json_suffix_is_not_enough() {
    let dir = temp();
    let path = dir.path().join("song.json");
    std::fs::write(&path, "{}").unwrap();
    assert_eq!(run(&["info", path_str(&path)]).code, 2);
    let bare = dir.path().join(".opentab.json");
    let result = run(&["new", "--out", path_str(&bare)]);
    assert_eq!(result.code, 2);
    assert!(!bare.exists());
}

#[test]
fn new_with_an_unknown_suffix_writes_nothing() {
    let dir = temp();
    let path = dir.path().join("song.gp");
    let result = run(&["new", "--out", path_str(&path)]);
    assert_eq!(result.code, 2);
    assert!(!path.exists());
}

#[test]
fn apply_with_an_unknown_output_suffix_exits_2() {
    let dir = temp();
    let score = new_score(dir.path(), "a.opentab.json");
    let commands = dir.path().join("c.jsonl");
    std::fs::write(&commands, "").unwrap();
    let out = dir.path().join("b.txt");
    let result = run(&[
        "apply",
        path_str(&score),
        path_str(&commands),
        "--out",
        path_str(&out),
    ]);
    assert_eq!(result.code, 2);
    assert!(!out.exists());
}

#[test]
fn suffixes_match_in_any_ascii_case() {
    let dir = temp();
    let json = dir.path().join("Song.OpenTab.JSON");
    let created = run(&["new", "--out", path_str(&json)]);
    assert_eq!(created.code, 0, "{}", created.stderr);
    for name in ["upper.GP", "mixed.Gp"] {
        let gp = dir.path().join(name);
        let converted = run(&["convert", path_str(&json), path_str(&gp)]);
        assert_eq!(converted.code, 0, "{name}: {}", converted.stderr);
        let bytes = std::fs::read(&gp).unwrap();
        assert!(bytes.starts_with(b"PK\x03\x04"), "{name} is a zip");
        let info = run(&["info", path_str(&gp), "--json"]);
        assert_eq!(info.code, 0, "{name}: {}", info.stderr);
    }
    let back = dir.path().join("back.OPENTAB.json");
    let converted = run(&[
        "convert",
        path_str(&dir.path().join("upper.GP")),
        path_str(&back),
    ]);
    assert_eq!(converted.code, 0, "{}", converted.stderr);
    assert!(
        std::fs::read_to_string(&back)
            .unwrap()
            .contains("opentab_json")
    );
    let bare = dir.path().join(".GP");
    assert_eq!(run(&["info", path_str(&bare)]).code, 2);
}
