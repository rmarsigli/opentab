use crate::support::{file_names, new_score, path_str, read_json, run, temp};

fn artist_commands(dir: &std::path::Path) -> std::path::PathBuf {
    let commands = dir.join("c.jsonl");
    std::fs::write(
        &commands,
        r#"{"op":"set_meta","field":"artist","value":"X"}"#,
    )
    .unwrap();
    commands
}

#[test]
fn a_directory_as_input_is_an_io_error() {
    let dir = temp();
    let folder = dir.path().join("folder.opentab.json");
    std::fs::create_dir(&folder).unwrap();
    let result = run(&["info", path_str(&folder), "--json"]);
    result.assert_no_panic();
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["kind"], "io");
}

#[test]
fn a_directory_as_the_commands_file_is_an_io_error() {
    let dir = temp();
    let score = new_score(dir.path(), "a.opentab.json");
    let folder = dir.path().join("commands");
    std::fs::create_dir(&folder).unwrap();
    let out = dir.path().join("b.opentab.json");
    let result = run(&[
        "apply",
        path_str(&score),
        path_str(&folder),
        "--out",
        path_str(&out),
        "--json",
    ]);
    result.assert_no_panic();
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["kind"], "io");
    assert!(!out.exists());
}

#[test]
fn a_directory_as_out_is_an_io_error_and_leaves_no_temp_file() {
    let dir = temp();
    let score = new_score(dir.path(), "a.opentab.json");
    let commands = artist_commands(dir.path());
    let out = dir.path().join("target.opentab.json");
    std::fs::create_dir(&out).unwrap();
    let result = run(&[
        "apply",
        path_str(&score),
        path_str(&commands),
        "--out",
        path_str(&out),
        "--json",
    ]);
    result.assert_no_panic();
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["kind"], "io");
    assert!(out.is_dir());
    assert_eq!(
        file_names(dir.path()),
        vec!["a.opentab.json", "c.jsonl", "target.opentab.json"]
    );
}

#[test]
fn new_with_a_directory_as_out_is_refused_as_existing() {
    let dir = temp();
    let out = dir.path().join("target.opentab.json");
    std::fs::create_dir(&out).unwrap();
    let result = run(&["new", "--out", path_str(&out), "--json"]);
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["kind"], "output_exists");
    assert_eq!(file_names(dir.path()), vec!["target.opentab.json"]);
}

#[test]
fn a_missing_parent_directory_for_out_is_an_io_error() {
    let dir = temp();
    let score = new_score(dir.path(), "a.opentab.json");
    let commands = artist_commands(dir.path());
    let out = dir.path().join("missing").join("b.opentab.json");
    let result = run(&[
        "apply",
        path_str(&score),
        path_str(&commands),
        "--out",
        path_str(&out),
        "--json",
    ]);
    result.assert_no_panic();
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["kind"], "io");
    assert_eq!(file_names(dir.path()), vec!["a.opentab.json", "c.jsonl"]);
}

#[test]
fn new_creates_exactly_one_file_and_no_leftover_temp() {
    let dir = temp();
    let path = new_score(dir.path(), "fresh.opentab.json");
    assert_eq!(file_names(dir.path()), vec!["fresh.opentab.json"]);
    let again = run(&["new", "--out", path_str(&path)]);
    assert_eq!(again.code, 2);
    assert_eq!(file_names(dir.path()), vec!["fresh.opentab.json"]);
}

#[test]
fn a_two_hundred_character_file_name_is_written() {
    let dir = temp();
    let name = format!("{}.opentab.json", "n".repeat(230));
    let path = new_score(dir.path(), &name);
    let commands = artist_commands(dir.path());
    let result = run(&[
        "apply",
        path_str(&path),
        path_str(&commands),
        "--out",
        path_str(&path),
    ]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(read_json(&path)["meta"]["artist"], "X");
}

#[cfg(unix)]
#[test]
fn new_refuses_a_dangling_symlink_and_keeps_it() {
    let dir = temp();
    let link = dir.path().join("link.opentab.json");
    std::os::unix::fs::symlink(dir.path().join("nowhere.opentab.json"), &link).unwrap();
    let result = run(&["new", "--out", path_str(&link), "--json"]);
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["kind"], "output_exists");
    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
    assert!(!dir.path().join("nowhere.opentab.json").exists());
}

#[cfg(unix)]
#[test]
fn apply_writes_through_a_symlinked_out_and_keeps_the_symlink() {
    let dir = temp();
    let score = new_score(dir.path(), "a.opentab.json");
    let commands = artist_commands(dir.path());
    let real_dir = dir.path().join("real");
    std::fs::create_dir(&real_dir).unwrap();
    let real = real_dir.join("target.opentab.json");
    std::fs::write(&real, "old").unwrap();
    let link = dir.path().join("link.opentab.json");
    std::os::unix::fs::symlink("real/target.opentab.json", &link).unwrap();
    let result = run(&[
        "apply",
        path_str(&score),
        path_str(&commands),
        "--out",
        path_str(&link),
    ]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
    assert_eq!(
        std::fs::read_link(&link).unwrap(),
        std::path::PathBuf::from("real/target.opentab.json")
    );
    assert_eq!(read_json(&real)["meta"]["artist"], "X");
    assert_eq!(read_json(&link)["meta"]["artist"], "X");
    assert_eq!(file_names(&real_dir), vec!["target.opentab.json"]);
}

#[cfg(unix)]
#[test]
fn apply_follows_a_chain_of_symlinks_and_creates_a_dangling_target() {
    let dir = temp();
    let score = new_score(dir.path(), "a.opentab.json");
    let commands = artist_commands(dir.path());
    let target = dir.path().join("created.opentab.json");
    let middle = dir.path().join("middle.opentab.json");
    let link = dir.path().join("link.opentab.json");
    std::os::unix::fs::symlink(&target, &middle).unwrap();
    std::os::unix::fs::symlink(&middle, &link).unwrap();
    let result = run(&[
        "apply",
        path_str(&score),
        path_str(&commands),
        "--out",
        path_str(&link),
    ]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
    assert!(std::fs::symlink_metadata(&middle).unwrap().is_symlink());
    assert!(std::fs::symlink_metadata(&target).unwrap().is_file());
    assert_eq!(read_json(&target)["meta"]["artist"], "X");
}

#[cfg(unix)]
#[test]
fn a_symlink_loop_as_out_is_an_io_error() {
    let dir = temp();
    let score = new_score(dir.path(), "a.opentab.json");
    let commands = artist_commands(dir.path());
    let first = dir.path().join("one.opentab.json");
    let second = dir.path().join("two.opentab.json");
    std::os::unix::fs::symlink(&second, &first).unwrap();
    std::os::unix::fs::symlink(&first, &second).unwrap();
    let result = run(&[
        "apply",
        path_str(&score),
        path_str(&commands),
        "--out",
        path_str(&first),
        "--json",
    ]);
    result.assert_no_panic();
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["kind"], "io");
}
