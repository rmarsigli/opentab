use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

pub struct Run {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Run {
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.stdout)
            .unwrap_or_else(|error| panic!("stdout is not JSON ({error}): {}", self.stdout))
    }

    pub fn assert_no_panic(&self) {
        assert!(!self.stderr.contains("panicked"), "{}", self.stderr);
        assert!(!self.stdout.contains("panicked"), "{}", self.stdout);
        assert_ne!(self.code, 101);
    }
}

pub fn run(args: &[&str]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_opentab"))
        .args(args)
        .output()
        .expect("spawn opentab");
    summarise(output)
}

#[cfg(unix)]
pub fn run_os(args: &[&std::ffi::OsStr]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_opentab"))
        .args(args)
        .output()
        .expect("spawn opentab");
    summarise(output)
}

fn summarise(output: Output) -> Run {
    Run {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

pub fn temp() -> TempDir {
    tempfile::tempdir().expect("tempdir")
}

pub fn path_str(path: &Path) -> &str {
    path.to_str().expect("utf-8 path")
}

pub fn new_score(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    let run = run(&["new", "--out", path_str(&path)]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    path
}

pub fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).expect("read score");
    serde_json::from_str(&text).expect("score is JSON")
}

pub fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

pub fn whole_rest_beat_id(score: &Value) -> u64 {
    score["tracks"][0]["staves"][0]["bars"][0]["voices"][0]["beats"][0]["id"]
        .as_u64()
        .expect("beat id")
}
