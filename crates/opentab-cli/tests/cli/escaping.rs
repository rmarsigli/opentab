#[cfg(unix)]
use crate::support::run_os;
use crate::support::{path_str, run, temp};

const HOSTILE: &str = "red\u{1b}[31m\nline\r\u{7}bell\u{85}next\u{9f}end\u{7f}";

fn score_with_title(title: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = temp();
    let path = dir.path().join("a.opentab.json");
    let created = run(&["new", "--out", path_str(&path), "--title", title]);
    assert_eq!(created.code, 0, "{}", created.stderr);
    (dir, path)
}

#[test]
fn control_characters_are_escaped_in_human_info() {
    let (_dir, path) = score_with_title(HOSTILE);
    let result = run(&["info", path_str(&path)]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(
        result.stdout.lines().any(|line| line
            == "title: red\\u{1b}[31m\\u{a}line\\u{d}\\u{7}bell\\u{85}next\\u{9f}end\\u{7f}"),
        "{:?}",
        result.stdout
    );
    assert!(!result.stdout.chars().any(|c| c.is_control() && c != '\n'));
    let lines: Vec<&str> = result.stdout.lines().collect();
    assert_eq!(
        lines[6..],
        [
            "diagnostics: 1",
            "  {\"kind\":\"unwritable_text\",\"element\":null,\"field\":\"meta.title\",\"code\":27}",
        ]
    );
    assert_eq!(lines.len(), 8);
}

#[test]
fn json_output_stays_standard_json_for_control_characters() {
    let (_dir, path) = score_with_title(HOSTILE);
    let result = run(&["info", path_str(&path), "--json"]);
    assert_eq!(result.json()["title"], HOSTILE);
    assert!(result.stdout.contains("\\u001b"), "{}", result.stdout);
    assert!(!result.stdout.contains("\\u{"));
}

#[test]
fn printable_unicode_is_not_escaped() {
    let (_dir, path) = score_with_title("Sérénade 日本語 \u{1f3b8}");
    let result = run(&["info", path_str(&path)]);
    assert!(result.stdout.contains("title: Sérénade 日本語 \u{1f3b8}"));
}

#[cfg(unix)]
#[test]
fn control_characters_in_paths_are_escaped_in_human_messages() {
    use std::ffi::OsStr;

    let dir = temp();
    let path = dir.path().join("evil\u{1b}[2Jname.opentab.json");
    let created = run_os(&[OsStr::new("new"), OsStr::new("--out"), path.as_os_str()]);
    assert_eq!(created.code, 0, "{}", created.stderr);
    assert!(
        created.stdout.contains("evil\\u{1b}[2Jname"),
        "{}",
        created.stdout
    );
    assert!(!created.stdout.contains('\u{1b}'));
    let again = run_os(&[OsStr::new("new"), OsStr::new("--out"), path.as_os_str()]);
    assert_eq!(again.code, 2);
    assert!(
        again.stderr.contains("evil\\u{1b}[2Jname"),
        "{}",
        again.stderr
    );
    assert!(!again.stderr.contains('\u{1b}'));
    let machine = run_os(&[
        OsStr::new("new"),
        OsStr::new("--out"),
        path.as_os_str(),
        OsStr::new("--json"),
    ]);
    assert!(!machine.stdout.contains('\u{1b}'));
    assert!(
        machine.json()["error"]["message"]
            .as_str()
            .unwrap()
            .contains('\u{1b}')
    );
}

#[cfg(unix)]
#[test]
fn control_characters_in_an_io_error_path_are_escaped() {
    let dir = temp();
    let path = dir.path().join("a.opentab.json");
    run(&["new", "--out", path_str(&path)]);
    let commands = dir.path().join("missing\u{1b}[31m.jsonl");
    let out = dir.path().join("b.opentab.json");
    let result = run(&[
        "apply",
        path_str(&path),
        path_str(&commands),
        "--out",
        path_str(&out),
    ]);
    assert_eq!(result.code, 2);
    assert!(
        result.stderr.contains("missing\\u{1b}[31m.jsonl"),
        "{:?}",
        result.stderr
    );
    assert!(!result.stderr.contains('\u{1b}'), "{:?}", result.stderr);
}
