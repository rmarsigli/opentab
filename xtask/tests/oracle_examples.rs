#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FAKE_POWERSHELL: &str = r#"#!/bin/sh
out=""
mode=""
example=""
target=""
while [ $# -gt 0 ]; do
  case "$1" in
    -OutDir) out="$2"; shift ;;
    -Mode) mode="$2"; shift ;;
    -Example) example="$2"; shift ;;
    -Target) target="$2"; shift ;;
  esac
  shift
done
printf '%s|%s|%s\n' "$mode" "$example" "$target" >> "$FAKE_GP_CALLS"
printf '4242\n133000000000000000\n' > "$out/pid.tmp"
mv "$out/pid.tmp" "$out/pid.txt"
if [ "$FAKE_GP_MODE" = "not_running" ]; then
  printf '{"ok":false,"gp_running":false,"error":"Guitar Pro is not running"}' > "$out/result.json"
  exit 2
fi
if [ "$mode" = "list" ]; then
  printf '\357\273\277{"ok":true,"gp_running":true,"gp_version":"%s","examples":%s,"steps":[]}' "${FAKE_GP_VERSION-8.1.5.31}" "$FAKE_GP_EXAMPLES" > "$out/result.json"
  exit 0
fi
if [ "$example" = "$FAKE_GP_FAIL" ]; then
  printf '{"ok":false,"gp_running":true,"steps":[{"name":"open","status":"failed","error":"example did not open"}]}' > "$out/result.json"
  exit 1
fi
printf 'gp:%s' "$example" > "$target"
printf '{"ok":true,"gp_running":true,"gp_version":"8.1.5.31","steps":[]}' > "$out/result.json"
exit 0
"#;

const FAKE_WSLPATH: &str = "#!/bin/sh\nprintf '%s\\n' \"$2\"\n";

struct Sandbox {
    _dir: tempfile::TempDir,
    root: PathBuf,
    powershell: PathBuf,
    wslpath: PathBuf,
    jobs: PathBuf,
    calls: PathBuf,
}

fn script(path: &Path, body: &str) {
    std::fs::write(path, body).expect("write script");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
}

fn sandbox() -> Sandbox {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    let powershell = root.join("fake powershell.sh");
    let wslpath = root.join("fake-wslpath.sh");
    script(&powershell, FAKE_POWERSHELL);
    script(&wslpath, FAKE_WSLPATH);
    Sandbox {
        jobs: root.join("jobs"),
        calls: root.join("calls.log"),
        _dir: dir,
        root,
        powershell,
        wslpath,
    }
}

fn examples(sandbox: &Sandbox, out: &Path, envs: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
    command
        .args(["oracle", "examples", "--out"])
        .arg(out)
        .env("OPENTAB_ORACLE_POWERSHELL", &sandbox.powershell)
        .env("OPENTAB_ORACLE_WSLPATH", &sandbox.wslpath)
        .env("OPENTAB_ORACLE_JOBS", &sandbox.jobs)
        .env("FAKE_GP_CALLS", &sandbox.calls)
        .env("FAKE_GP_EXAMPLES", "[\"Sample\",\"Sample (Composer)\"]");
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("run xtask")
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn calls(sandbox: &Sandbox) -> Vec<String> {
    std::fs::read_to_string(&sandbox.calls)
        .map(|text| text.lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

#[test]
fn every_listed_example_is_saved_under_its_kebab_name_with_an_index() {
    let sandbox = sandbox();
    let out = sandbox.root.join("corpus");
    let output = examples(&sandbox, &out, &[]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    assert!(text(&output).contains("saved 2 example(s)"));
    assert_eq!(
        std::fs::read(out.join("sample.gp")).expect("first"),
        b"gp:Sample"
    );
    assert_eq!(
        std::fs::read(out.join("sample-composer.gp")).expect("second"),
        b"gp:Sample (Composer)"
    );
    let index: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("examples.json")).expect("index"))
            .expect("json");
    assert_eq!(index["gp_version"], "8.1.5.31");
    assert_eq!(index["complete"], true);
    assert_eq!(index["examples"][1]["name"], "Sample (Composer)");
    assert_eq!(index["examples"][1]["file"], "sample-composer.gp");
    assert_eq!(
        index["examples"][0]["sha256"].as_str().map(str::len),
        Some(64)
    );
    let log = calls(&sandbox);
    assert_eq!(log.len(), 3);
    assert_eq!(log[0], "list||");
    assert!(log[1].starts_with("save|Sample|"), "{}", log[1]);
    assert!(log[1].ends_with("/out/sample.gp"), "{}", log[1]);
    assert!(log[2].starts_with("save|Sample (Composer)|"), "{}", log[2]);
    assert_eq!(std::fs::read_dir(&sandbox.jobs).expect("jobs").count(), 0);
}

#[test]
fn a_failed_example_is_reported_and_the_rest_are_saved() {
    let sandbox = sandbox();
    let out = sandbox.root.join("corpus");
    let output = examples(&sandbox, &out, &[("FAKE_GP_FAIL", "Sample")]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    let report = text(&output);
    assert!(
        report.contains("step open failed: example did not open"),
        "{report}"
    );
    assert!(
        report.contains("1 of 2 example(s) failed: Sample"),
        "{report}"
    );
    assert!(!out.join("sample.gp").exists());
    assert!(out.join("sample-composer.gp").exists());
    assert_eq!(std::fs::read_dir(&sandbox.jobs).expect("jobs").count(), 1);
    let index: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("examples.json")).expect("index"))
            .expect("json");
    assert_eq!(index["complete"], false);
    assert_eq!(index["examples"].as_array().map(Vec::len), Some(1));
}

#[test]
fn an_empty_guitar_pro_version_stops_before_any_save() {
    let sandbox = sandbox();
    let out = sandbox.root.join("corpus");
    let output = examples(&sandbox, &out, &[("FAKE_GP_VERSION", " ")]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("reported no version"));
    assert_eq!(calls(&sandbox), vec![String::from("list||")]);
    assert!(!out.exists());
}

#[test]
fn guitar_pro_not_running_stops_before_any_save() {
    let sandbox = sandbox();
    let out = sandbox.root.join("corpus");
    let output = examples(&sandbox, &out, &[("FAKE_GP_MODE", "not_running")]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("Guitar Pro is not running"));
    assert_eq!(calls(&sandbox).len(), 1);
    assert!(!out.exists());
}

#[test]
fn colliding_example_names_stop_before_any_save() {
    let sandbox = sandbox();
    let out = sandbox.root.join("corpus");
    let output = examples(
        &sandbox,
        &out,
        &[("FAKE_GP_EXAMPLES", "[\"Example Two\",\"Example  Two!\"]")],
    );
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("same file example-two.gp"));
    assert_eq!(calls(&sandbox), vec![String::from("list||")]);
}

#[test]
fn output_inside_the_repository_is_refused_before_guitar_pro_is_touched() {
    let sandbox = sandbox();
    let inside = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("target")
        .join("never-created-corpus");
    let output = examples(&sandbox, &inside, &[]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("inside the repository"));
    assert!(calls(&sandbox).is_empty());
    assert!(!inside.exists());
}
