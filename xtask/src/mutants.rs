use std::fmt;
use std::path::{Path, PathBuf};

pub const CRATES: [&str; 3] = ["opentab-model", "opentab-command", "opentab-gp"];
pub const JOBS: u32 = 1;
pub const TIMEOUT_SECS: u32 = 120;
pub const OUTPUT_DIR: &str = "target/mutants";
pub const INSTALL_HINT: &str = "cargo-mutants is not installed; install it with `cargo install cargo-mutants --version 27.1.0 --locked`";

const RESULTS_DIR: &str = "mutants.out";
const MISSED_FILE: &str = "missed.txt";
const TIMEOUT_FILE: &str = "timeout.txt";

pub fn output_dir(root: &Path) -> PathBuf {
    root.join(OUTPUT_DIR)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shard {
    pub index: u32,
    pub count: u32,
}

impl Shard {
    pub fn parse(text: &str) -> Result<Shard, String> {
        let invalid = || format!("shard \"{text}\" must be K/N with 0 <= K < N, for example 0/4");
        let (index, count) = text.split_once('/').ok_or_else(invalid)?;
        let index: u32 = index.parse().map_err(|_| invalid())?;
        let count: u32 = count.parse().map_err(|_| invalid())?;
        if index >= count {
            return Err(invalid());
        }
        Ok(Shard { index, count })
    }
}

impl fmt::Display for Shard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.index, self.count)
    }
}

pub fn mutants_args(output: &Path, shard: Option<Shard>, iterate: bool) -> Vec<String> {
    let mut args = vec!["mutants".to_owned()];
    for name in CRATES {
        args.push("--package".to_owned());
        args.push(name.to_owned());
    }
    args.push("--jobs".to_owned());
    args.push(JOBS.to_string());
    args.push("--timeout".to_owned());
    args.push(TIMEOUT_SECS.to_string());
    args.push("--output".to_owned());
    args.push(output.display().to_string());
    if let Some(shard) = shard {
        args.push("--shard".to_owned());
        args.push(shard.to_string());
    }
    if iterate {
        args.push("--iterate".to_owned());
    }
    args
}

pub fn lines_of(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MutantsReport {
    pub missed: Vec<String>,
    pub timeouts: Vec<String>,
}

impl MutantsReport {
    pub fn read(output: &Path) -> MutantsReport {
        let results = output.join(RESULTS_DIR);
        let read = |name: &str| {
            std::fs::read_to_string(results.join(name))
                .map(|text| lines_of(&text))
                .unwrap_or_default()
        };
        MutantsReport {
            missed: read(MISSED_FILE),
            timeouts: read(TIMEOUT_FILE),
        }
    }

    pub fn is_clean(&self) -> bool {
        self.missed.is_empty() && self.timeouts.is_empty()
    }

    pub fn render(&self) -> String {
        let mut lines = vec![format!(
            "{} missed, {} timed out",
            self.missed.len(),
            self.timeouts.len()
        )];
        lines.extend(self.missed.iter().map(|line| format!("missed: {line}")));
        lines.extend(self.timeouts.iter().map(|line| format!("timeout: {line}")));
        lines.join("\n")
    }
}

pub fn verdict(tool_succeeded: bool, report: &MutantsReport) -> bool {
    tool_succeeded && report.is_clean()
}
