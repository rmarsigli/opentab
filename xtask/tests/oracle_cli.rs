#![cfg(unix)]

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FAKE_POWERSHELL: &str = r#"#!/bin/sh
src=""
out=""
cmd=""
while [ $# -gt 0 ]; do
  case "$1" in
    -Source) src="$2"; shift ;;
    -OutDir) out="$2"; shift ;;
    -Command) cmd="$2"; shift ;;
  esac
  shift
done
if [ -n "$cmd" ]; then
  printf '%s\n' "$cmd" >> "$FAKE_GP_KILL_LOG"
  exit "${FAKE_GP_KILL_EXIT:-0}"
fi
[ -n "$out" ] || exit 0
case "$FAKE_GP_MODE" in
  silent|silent_hang) ;;
  *)
    if [ "$FAKE_GP_PID_START" = "none" ]; then
      printf '4242\n' > "$out/pid.tmp"
    else
      printf '4242\n133000000000000000\n' > "$out/pid.tmp"
    fi
    mv "$out/pid.tmp" "$out/pid.txt" ;;
esac
case "$FAKE_GP_MODE" in
  silent_hang)
    while [ ! -e "$FAKE_GP_SENTINEL" ]; do
      sleep 0.1
    done
    exit 0 ;;
  hang)
    while [ ! -e "$FAKE_GP_SENTINEL" ]; do
      sleep 0.1
    done
    exit 0 ;;
  not_running)
    printf '{"ok":false,"gp_running":false,"error":"Guitar Pro is not running; start it and leave it on Empty"}' > "$out/result.json"
    exit 2 ;;
  fail)
    mkdir -p "$out/diagnostics"
    printf '{"ok":false,"gp_running":true,"gp_version":"8.1.5.31","steps":[{"name":"pdf","status":"failed","ms":5,"attempts":3,"error":"dialog never appeared","screenshot":"x.png"}]}' > "$out/result.json"
    exit 1 ;;
  silent)
    exit 3 ;;
esac
cp "$src" "$out/resave.gp"
printf 'MThd%s' "$(cat "$src")" > "$out/export.mid"
printf '<score><encoding-date>%s</encoding-date><title>%s</title></score>' "$FAKE_GP_STAMP" "$(cat "$src")" > "$out/export.musicxml"
printf '%%PDF-1.4\n<< /CreationDate (D:%s) /ModDate (D:%s) >>\n%s\n%%%%EOF\n' "$FAKE_GP_STAMP" "$FAKE_GP_STAMP" "$(cat "$src")" > "$out/export.pdf"
printf 'GP5 %s %s' "$(cat "$src")" "$FAKE_GP_STAMP" > "$out/export.gp5"
printf 'GPX %s' "$(cat "$src")" > "$out/export.gpx"
mkdir -p "$out/png" "$out/diagnostics"
printf 'page one' > "$out/png/song 1.png"
printf 'page two' > "$out/png/song 2.png"
printf 'noise %s' "$FAKE_GP_STAMP" > "$out/diagnostics/log.txt"
printf '\357\273\277{"ok":true,"gp_running":true,"gp_version":"8.1.5.31","steps":[],"modals_at_start":%s}' "${FAKE_GP_MODALS:-[]}" > "$out/result.json"
exit 0
"#;

const FAKE_WSLPATH: &str = "#!/bin/sh\nprintf '%s\\n' \"$2\"\n";

struct Sandbox {
    _dir: tempfile::TempDir,
    root: PathBuf,
    powershell: PathBuf,
    wslpath: PathBuf,
    jobs: PathBuf,
    input: PathBuf,
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
    let input = root.join("entrada com espaço ç");
    std::fs::create_dir_all(input.join("sub dir")).expect("mkdir");
    std::fs::write(input.join("example one.gp"), b"fixture-one").expect("write");
    std::fs::write(input.join("sub dir/Canção.gp"), b"fixture-two").expect("write");
    Sandbox {
        jobs: root.join("jobs"),
        _dir: dir,
        root,
        powershell,
        wslpath,
        input,
    }
}

fn xtask(sandbox: &Sandbox, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
    command
        .args(args)
        .env("OPENTAB_ORACLE_POWERSHELL", &sandbox.powershell)
        .env("OPENTAB_ORACLE_WSLPATH", &sandbox.wslpath)
        .env("OPENTAB_ORACLE_JOBS", &sandbox.jobs)
        .env("FAKE_GP_STAMP", "2026-10-17");
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("run xtask")
}

fn harvest(sandbox: &Sandbox, out: &Path, envs: &[(&str, &str)]) -> Output {
    let input = sandbox.input.to_str().expect("utf8");
    let out = out.to_str().expect("utf8");
    xtask(
        sandbox,
        &["oracle", "harvest", "--input", input, "--out", out],
        envs,
    )
}

fn check(sandbox: &Sandbox, goldens: &Path) -> Output {
    let input = sandbox.input.to_str().expect("utf8");
    let goldens = goldens.to_str().expect("utf8");
    xtask(
        sandbox,
        &["oracle", "check", "--input", input, "--goldens", goldens],
        &[],
    )
}

fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, prefix: &str, files: &mut BTreeMap<String, Vec<u8>>) {
        for item in std::fs::read_dir(dir).expect("list") {
            let item = item.expect("entry");
            let name = format!("{prefix}{}", item.file_name().to_string_lossy());
            if item.file_type().expect("type").is_dir() {
                walk(&item.path(), &format!("{name}/"), files);
            } else {
                files.insert(name, std::fs::read(item.path()).expect("read"));
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(root, "", &mut files);
    files
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn missing_powershell_exits_2_with_a_message() {
    let sandbox = sandbox();
    let out = sandbox.root.join("out");
    let output = harvest(
        &sandbox,
        &out,
        &[("OPENTAB_ORACLE_POWERSHELL", "/nonexistent")],
    );
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("/nonexistent was not found"));
    assert!(text(&output).contains("Windows host or WSL"));
    assert!(!out.exists());
}

#[test]
fn gp_not_running_exits_2_and_leaves_no_job() {
    let sandbox = sandbox();
    let out = sandbox.root.join("out");
    let output = harvest(&sandbox, &out, &[("FAKE_GP_MODE", "not_running")]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("Guitar Pro is not running"));
    assert_eq!(std::fs::read_dir(&sandbox.jobs).expect("jobs").count(), 0);
    assert!(!out.join("manifest.json").exists());
}

#[test]
fn step_failure_exits_2_and_keeps_the_job() {
    let sandbox = sandbox();
    let out = sandbox.root.join("out");
    let output = harvest(&sandbox, &out, &[("FAKE_GP_MODE", "fail")]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("step pdf failed: dialog never appeared"));
    assert_eq!(std::fs::read_dir(&sandbox.jobs).expect("jobs").count(), 2);
    assert!(!out.join("manifest.json").exists());
}

#[test]
fn script_without_result_is_a_failure() {
    let sandbox = sandbox();
    let out = sandbox.root.join("out");
    let output = harvest(&sandbox, &out, &[("FAKE_GP_MODE", "silent")]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("without writing result.json"));
}

#[test]
fn harvest_writes_the_golden_layout_and_reharvest_is_byte_identical() {
    let sandbox = sandbox();
    let first = sandbox.root.join("goldens one");
    let second = sandbox.root.join("goldens two");
    let a = harvest(
        &sandbox,
        &first,
        &[("FAKE_GP_MODALS", "[\"About Guitar Pro 8\"]")],
    );
    assert_eq!(a.status.code(), Some(0), "{}", text(&a));
    assert!(text(&a).contains("dialog 'About Guitar Pro 8' was open at start"));
    let b = harvest(&sandbox, &second, &[("FAKE_GP_STAMP", "2031-02-03")]);
    assert_eq!(b.status.code(), Some(0), "{}", text(&b));
    let one = tree(&first);
    let names: Vec<&str> = one.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        [
            "example one.gp/export.gp5",
            "example one.gp/export.gpx",
            "example one.gp/export.mid",
            "example one.gp/export.musicxml",
            "example one.gp/export.pdf",
            "example one.gp/png/song 1.png",
            "example one.gp/png/song 2.png",
            "example one.gp/resave.gp",
            "manifest.json",
            "sub dir/Canção.gp/export.gp5",
            "sub dir/Canção.gp/export.gpx",
            "sub dir/Canção.gp/export.mid",
            "sub dir/Canção.gp/export.musicxml",
            "sub dir/Canção.gp/export.pdf",
            "sub dir/Canção.gp/png/song 1.png",
            "sub dir/Canção.gp/png/song 2.png",
            "sub dir/Canção.gp/resave.gp",
        ]
    );
    let two = tree(&second);
    let volatile = |name: &String| name.ends_with("/export.gp5");
    let stable_one: BTreeMap<&String, &Vec<u8>> =
        one.iter().filter(|(k, _)| !volatile(k)).collect();
    let stable_two: BTreeMap<&String, &Vec<u8>> =
        two.iter().filter(|(k, _)| !volatile(k)).collect();
    assert_eq!(stable_one, stable_two);
    assert_ne!(
        one["example one.gp/export.gp5"],
        two["example one.gp/export.gp5"]
    );
    let fresh = check(&sandbox, &second);
    assert_eq!(fresh.status.code(), Some(0), "{}", text(&fresh));
    let manifest = String::from_utf8(one["manifest.json"].clone()).expect("utf8");
    assert!(manifest.contains("\"gp_version\": \"8.1.5.31\""));
    assert!(!manifest.contains("2026") && !manifest.contains("2031"));
    let parsed: serde_json::Value = serde_json::from_str(&manifest).expect("json");
    let outputs = parsed["entries"][0]["outputs"].as_array().expect("outputs");
    let gp5 = outputs
        .iter()
        .find(|output| output["path"] == "export.gp5")
        .expect("gp5 listed");
    assert_eq!(gp5["byte_identical"], false);
    assert!(gp5["sha256"].is_null());
    let mid = outputs
        .iter()
        .find(|output| output["path"] == "export.mid")
        .expect("mid listed");
    assert_eq!(mid["byte_identical"], true);
    assert_eq!(mid["sha256"].as_str().map(str::len), Some(64));
    let xml = String::from_utf8(one["example one.gp/export.musicxml"].clone()).expect("utf8");
    assert!(xml.contains("<encoding-date>1970-01-01</encoding-date>"));
    assert_eq!(std::fs::read_dir(&sandbox.jobs).expect("jobs").count(), 0);
}

#[test]
fn check_is_fresh_then_stale_after_one_byte_changes() {
    let sandbox = sandbox();
    let goldens = sandbox.root.join("goldens");
    let a = harvest(&sandbox, &goldens, &[]);
    assert_eq!(a.status.code(), Some(0), "{}", text(&a));
    let fresh = check(&sandbox, &goldens);
    assert_eq!(fresh.status.code(), Some(0), "{}", text(&fresh));
    let fixture = sandbox.input.join("sub dir/Canção.gp");
    let mut bytes = std::fs::read(&fixture).expect("read");
    bytes[3] ^= 0x20;
    std::fs::write(&fixture, &bytes).expect("write");
    let stale = check(&sandbox, &goldens);
    assert_eq!(stale.status.code(), Some(2), "{}", text(&stale));
    assert_eq!(
        String::from_utf8_lossy(&stale.stdout),
        "stale: sub dir/Canção.gp\n"
    );
    std::fs::remove_file(sandbox.input.join("example one.gp")).expect("remove");
    std::fs::write(sandbox.input.join("new.gp"), b"n").expect("write");
    let mixed = check(&sandbox, &goldens);
    assert_eq!(mixed.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&mixed.stdout),
        "stale: sub dir/Canção.gp\nmissing: new.gp\norphan: example one.gp\n"
    );
}

#[test]
fn same_stem_pair_is_harvested_into_separate_golden_dirs() {
    let sandbox = sandbox();
    std::fs::write(sandbox.input.join("song.gp"), b"native").expect("write");
    std::fs::write(sandbox.input.join("song.gp5"), b"exported").expect("write");
    let goldens = sandbox.root.join("goldens");
    let output = harvest(&sandbox, &goldens, &[]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let files = tree(&goldens);
    assert_eq!(files["song.gp/resave.gp"], b"native");
    assert_eq!(files["song.gp5/resave.gp"], b"exported");
    let fresh = check(&sandbox, &goldens);
    assert_eq!(fresh.status.code(), Some(0), "{}", text(&fresh));
}

fn hang_envs<'a>(sandbox: &'a Sandbox, extra: &[(&'a str, &'a str)]) -> Vec<(&'a str, String)> {
    let mut envs = vec![
        ("FAKE_GP_MODE", String::from("hang")),
        ("OPENTAB_ORACLE_TIMEOUT_MS", String::from("300")),
        (
            "FAKE_GP_SENTINEL",
            sandbox.root.join("never-created").display().to_string(),
        ),
        (
            "FAKE_GP_KILL_LOG",
            sandbox.root.join("kill.log").display().to_string(),
        ),
    ];
    envs.extend(extra.iter().map(|(key, value)| (*key, (*value).to_owned())));
    envs
}

fn harvest_hanging(sandbox: &Sandbox, extra: &[(&str, &str)]) -> Output {
    let envs = hang_envs(sandbox, extra);
    let borrowed: Vec<(&str, &str)> = envs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    harvest(sandbox, &sandbox.root.join("out"), &borrowed)
}

fn kill_log(sandbox: &Sandbox) -> Vec<String> {
    std::fs::read_to_string(sandbox.root.join("kill.log"))
        .map(|text| text.lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

#[test]
fn hung_powershell_is_killed_by_pid_and_start_time_and_reported() {
    let sandbox = sandbox();
    let output = harvest_hanging(&sandbox, &[]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    let report = text(&output);
    assert!(report.contains("timed out after 300 ms"), "{report}");
    assert!(
        report.contains("stopped the PowerShell process"),
        "{report}"
    );
    let log = kill_log(&sandbox);
    assert_eq!(log.len(), 2);
    for line in &log {
        assert!(line.contains("Get-Process -Id 4242"), "{line}");
        assert!(line.contains("ProcessName -eq 'powershell'"), "{line}");
        assert!(
            line.contains("ToFileTimeUtc() -eq 133000000000000000"),
            "{line}"
        );
    }
    assert_eq!(std::fs::read_dir(&sandbox.jobs).expect("jobs").count(), 2);
    assert!(!sandbox.root.join("out/manifest.json").exists());
}

#[test]
fn pid_without_start_time_is_matched_by_command_line_never_by_name() {
    let sandbox = sandbox();
    let output = harvest_hanging(&sandbox, &[("FAKE_GP_PID_START", "none")]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    let log = kill_log(&sandbox);
    assert_eq!(log.len(), 2);
    for line in &log {
        assert!(line.contains("Win32_Process"), "{line}");
        assert!(
            line.contains("CommandLine -like '*Invoke-GpExport*'"),
            "{line}"
        );
    }
}

#[test]
fn kill_that_matches_nothing_is_reported() {
    let sandbox = sandbox();
    let output = harvest_hanging(&sandbox, &[("FAKE_GP_KILL_EXIT", "3")]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("did not stop PowerShell: pid no longer matches"));
}

#[test]
fn hang_before_any_pid_is_recorded_reports_that_nothing_was_killed() {
    let sandbox = sandbox();
    let output = harvest_hanging(
        &sandbox,
        &[
            ("FAKE_GP_MODE", "silent_hang"),
            ("OPENTAB_ORACLE_PID_WAIT_MS", "300"),
        ],
    );
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("no pid recorded within 300 ms"));
    assert!(!text(&output).contains("timed out after"));
    assert!(kill_log(&sandbox).is_empty());
}

#[test]
fn unsupported_fixture_name_is_rejected_before_any_guitar_pro_work() {
    let sandbox = sandbox();
    std::fs::write(sandbox.input.join("bad:name.gp"), b"x").expect("write");
    let out = sandbox.root.join("out");
    let output = harvest(
        &sandbox,
        &out,
        &[(
            "FAKE_GP_KILL_LOG",
            sandbox.root.join("calls.log").to_str().expect("utf8"),
        )],
    );
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("not supported"));
    assert!(!out.exists());
    assert!(!sandbox.jobs.exists());
}
