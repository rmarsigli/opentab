use crate::oracle::error::OracleError;
use crate::oracle::fixtures::{Fixture, discover};
use crate::oracle::hash::sha256_hex;
use crate::oracle::host::{Host, KillOutcome, PidRecord};
use crate::oracle::layout::{MANIFEST_FILE, is_byte_identical, select_outputs};
use crate::oracle::manifest::{Manifest, ManifestEntry, OutputEntry};
use crate::oracle::normalize::normalize_output;
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub const EXIT_GP_NOT_RUNNING: i32 = 2;
const RESULT_FILE: &str = "result.json";
const PID_FILE: &str = "pid.txt";
const LOG_FILE: &str = "powershell.log";

#[derive(Debug, Clone, Deserialize)]
pub struct StepResult {
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub ms: u64,
    #[serde(default)]
    pub attempts: u32,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub screenshot: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExportResult {
    pub ok: bool,
    #[serde(default)]
    pub gp_running: bool,
    #[serde(default)]
    pub gp_version: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub steps: Vec<StepResult>,
    #[serde(default)]
    pub modals_at_start: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct HarvestOptions {
    pub input: PathBuf,
    pub out: PathBuf,
    pub keep_jobs: bool,
}

#[derive(Debug, Clone, Default)]
pub struct HarvestReport {
    pub harvested: Vec<String>,
    pub failures: Vec<String>,
}

enum FixtureOutcome {
    Done(ManifestEntry, String),
    Failed(String),
}

pub fn parse_result(bytes: &[u8]) -> Result<ExportResult, OracleError> {
    let trimmed = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    serde_json::from_slice(trimmed)
        .map_err(|error| OracleError::new(format!("unreadable result.json: {error}")))
}

fn script_path() -> Result<PathBuf, OracleError> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("oracle")
        .join("Invoke-GpExport.ps1");
    path.canonicalize()
        .map_err(|error| OracleError::io(format!("cannot find {}", path.display()), &error))
}

fn io<T>(result: std::io::Result<T>, context: impl FnOnce() -> String) -> Result<T, OracleError> {
    result.map_err(|error| OracleError::io(context(), &error))
}

fn fresh_job_dir(jobs: &Path, index: usize) -> Result<PathBuf, OracleError> {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    for attempt in 0..1000u32 {
        let id = format!("{secs}-{}-{index:03}-{attempt}", std::process::id());
        let dir = jobs.join(&id);
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(OracleError::io(
                    format!("cannot create {}", dir.display()),
                    &error,
                ));
            }
        }
    }
    Err(OracleError::new("cannot allocate a fresh job directory"))
}

fn describe_failure(result: &ExportResult) -> String {
    let mut parts = Vec::new();
    if let Some(error) = &result.error {
        parts.push(error.clone());
    }
    for step in result.steps.iter().filter(|step| step.status != "ok") {
        let mut text = format!("step {} {}", step.name, step.status);
        if let Some(error) = &step.error {
            text.push_str(&format!(": {error}"));
        }
        if let Some(shot) = &step.screenshot {
            text.push_str(&format!(" (screenshot {shot})"));
        }
        parts.push(text);
    }
    if parts.is_empty() {
        String::from("export reported failure without details")
    } else {
        parts.join("; ")
    }
}

fn stop_script(host: &Host, job_out: &Path) -> &'static str {
    let record = std::fs::read_to_string(job_out.join(PID_FILE))
        .ok()
        .and_then(|text| PidRecord::parse(&text));
    match record.map(|record| host.stop_windows_process(&record)) {
        Some(KillOutcome::Stopped) => "stopped the PowerShell process",
        Some(KillOutcome::NotMatched) => {
            "did not stop PowerShell: pid no longer matches the recorded process"
        }
        Some(KillOutcome::Unavailable) => "could not stop PowerShell: kill command failed",
        None => "could not stop PowerShell: no pid recorded",
    }
}

fn write_golden(
    out: &Path,
    fixture: &Fixture,
    job_out: &Path,
    fixture_sha: String,
) -> Result<ManifestEntry, OracleError> {
    let selected = select_outputs(job_out)?;
    let golden = out.join(fixture.golden_dir());
    if golden.exists() {
        io(std::fs::remove_dir_all(&golden), || {
            format!("cannot replace {}", golden.display())
        })?;
    }
    let mut outputs = Vec::new();
    for (relative, source) in selected {
        let raw = io(std::fs::read(&source), || {
            format!("cannot read {}", source.display())
        })?;
        let normalised = normalize_output(&relative, &raw);
        let target = golden.join(&relative);
        if let Some(parent) = target.parent() {
            io(std::fs::create_dir_all(parent), || {
                format!("cannot create {}", parent.display())
            })?;
        }
        io(std::fs::write(&target, &normalised), || {
            format!("cannot write {}", target.display())
        })?;
        outputs.push(OutputEntry {
            sha256: is_byte_identical(&relative).then(|| sha256_hex(&normalised)),
            byte_identical: is_byte_identical(&relative),
            path: relative,
        });
    }
    Ok(ManifestEntry {
        fixture: fixture.key.clone(),
        sha256: fixture_sha,
        outputs,
    })
}

fn run_fixture(
    host: &Host,
    script: &str,
    jobs: &Path,
    index: usize,
    fixture: &Fixture,
    options: &HarvestOptions,
    expected_version: &str,
) -> Result<FixtureOutcome, OracleError> {
    let bytes = io(std::fs::read(&fixture.path), || {
        format!("cannot read {}", fixture.path.display())
    })?;
    let fixture_sha = sha256_hex(&bytes);
    let job = fresh_job_dir(jobs, index)?;
    let source = job.join(fixture.file_name());
    io(std::fs::write(&source, &bytes), || {
        format!("cannot write {}", source.display())
    })?;
    let job_out = job.join("out");
    io(std::fs::create_dir(&job_out), || {
        format!("cannot create {}", job_out.display())
    })?;
    let source_win = host.to_windows(&source)?;
    let out_win = host.to_windows(&job_out)?;
    let budget_args = host.budget.script_args();
    let mut args: Vec<(&str, &str)> = vec![("Source", &source_win), ("OutDir", &out_win)];
    args.extend(
        budget_args
            .iter()
            .map(|(name, value)| (*name, value.as_str())),
    );
    let run = host.run_script(script, &args, &job.join(LOG_FILE))?;
    if run.timed_out {
        let stopped = stop_script(host, &job_out);
        return Ok(FixtureOutcome::Failed(format!(
            "timed out after {} ms ({stopped}); job kept in {}",
            host.timeout.as_millis(),
            job.display()
        )));
    }
    let result_path = job_out.join(RESULT_FILE);
    let result = match std::fs::read(&result_path) {
        Ok(raw) => parse_result(&raw)?,
        Err(_) => {
            let log = std::fs::read(job.join(LOG_FILE)).unwrap_or_default();
            let code = run.status.and_then(|status| status.code());
            let message = format!(
                "PowerShell exited with {code:?} without writing result.json: {}",
                String::from_utf8_lossy(&log).trim()
            );
            if code == Some(EXIT_GP_NOT_RUNNING) {
                let _ = std::fs::remove_dir_all(&job);
                return Err(OracleError::new(message));
            }
            return Ok(FixtureOutcome::Failed(format!(
                "{message}; job kept in {}",
                job.display()
            )));
        }
    };
    if !result.gp_running {
        let _ = std::fs::remove_dir_all(&job);
        return Err(OracleError::new(result.error.unwrap_or_else(|| {
            String::from(
                "Guitar Pro is not running; start it yourself, the harness never launches it",
            )
        })));
    }
    for modal in &result.modals_at_start {
        eprintln!(
            "note: {}: dialog '{modal}' was open at start and was dismissed",
            fixture.key
        );
    }
    let version = result.gp_version.clone().unwrap_or_default();
    if !expected_version.is_empty() && expected_version != version {
        let _ = std::fs::remove_dir_all(&job);
        return Err(OracleError::new(format!(
            "{} holds goldens from Guitar Pro {expected_version} but the host runs {version}; harvest into a fresh directory",
            options.out.display()
        )));
    }
    if !result.ok {
        return Ok(FixtureOutcome::Failed(format!(
            "{}; job kept in {}",
            describe_failure(&result),
            job.display()
        )));
    }
    match write_golden(&options.out, fixture, &job_out, fixture_sha) {
        Ok(entry) => {
            if !options.keep_jobs {
                let _ = std::fs::remove_dir_all(&job);
            }
            Ok(FixtureOutcome::Done(entry, version))
        }
        Err(error) => Ok(FixtureOutcome::Failed(format!(
            "{error}; job kept in {}",
            job.display()
        ))),
    }
}

pub fn harvest(host: &Host, options: &HarvestOptions) -> Result<HarvestReport, OracleError> {
    let fixtures = discover(&options.input)?;
    if fixtures.is_empty() {
        return Err(OracleError::new(format!(
            "no Guitar Pro files under {}",
            options.input.display()
        )));
    }
    let script = host.to_windows(&script_path()?)?;
    io(std::fs::create_dir_all(&options.out), || {
        format!("cannot create {}", options.out.display())
    })?;
    let manifest_path = options.out.join(MANIFEST_FILE);
    let mut manifest = if manifest_path.exists() {
        Manifest::load(&manifest_path)?
    } else {
        Manifest::new("")
    };
    let jobs = host.to_linux(&host.jobs_windows)?;
    io(std::fs::create_dir_all(&jobs), || {
        format!("cannot create {}", jobs.display())
    })?;
    let mut report = HarvestReport::default();
    for (index, fixture) in fixtures.iter().enumerate() {
        let expected = manifest.gp_version.clone();
        match run_fixture(host, &script, &jobs, index, fixture, options, &expected)? {
            FixtureOutcome::Done(entry, version) => {
                manifest.gp_version = version;
                manifest.upsert(entry);
                manifest.save(&manifest_path)?;
                println!("ok: {}", fixture.key);
                report.harvested.push(fixture.key.clone());
            }
            FixtureOutcome::Failed(reason) => {
                eprintln!("failed: {}: {reason}", fixture.key);
                report.failures.push(fixture.key.clone());
            }
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::{describe_failure, parse_result};

    #[test]
    fn result_json_with_bom_parses() {
        let raw = "\u{feff}{\"ok\":false,\"gp_running\":true,\"gp_version\":\"8.1.5.31\",\"steps\":[{\"name\":\"pdf\",\"status\":\"failed\",\"ms\":12,\"attempts\":3,\"error\":\"dialog 'x' never appeared\",\"screenshot\":\"C:\\\\a b\\\\fail-pdf.png\"}],\"extra\":1}";
        let result = parse_result(raw.as_bytes()).expect("parse");
        assert!(!result.ok);
        assert_eq!(result.gp_version.as_deref(), Some("8.1.5.31"));
        let text = describe_failure(&result);
        assert!(text.contains("step pdf failed: dialog 'x' never appeared"));
        assert!(text.contains("fail-pdf.png"));
    }

    #[test]
    fn garbage_result_is_an_error() {
        assert!(parse_result(b"\x00\xff").is_err());
    }
}
