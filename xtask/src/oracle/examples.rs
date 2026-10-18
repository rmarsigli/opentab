use crate::oracle::error::OracleError;
use crate::oracle::harvest::{
    EXIT_GP_NOT_RUNNING, LOG_FILE, PID_FILE, RESULT_FILE, StepResult, describe_steps,
    fresh_job_dir, io, stop_script,
};
use crate::oracle::hash::sha256_hex;
use crate::oracle::host::{EXAMPLE_SCRIPT_NAME, Host, PidGate, ScriptRun};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const EXAMPLES_INDEX: &str = "examples.json";
pub const MODE_LIST: &str = "list";
pub const MODE_SAVE: &str = "save";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptArg {
    pub name: &'static str,
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExampleResult {
    pub ok: bool,
    #[serde(default)]
    pub gp_running: bool,
    #[serde(default)]
    pub gp_version: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub steps: Vec<StepResult>,
    #[serde(default)]
    pub modals_at_start: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExampleEntry {
    pub name: String,
    pub file: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExamplesIndex {
    pub gp_version: String,
    pub examples: Vec<ExampleEntry>,
}

#[derive(Debug, Clone)]
pub struct ExamplesOptions {
    pub out: PathBuf,
    pub keep_jobs: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ExamplesReport {
    pub saved: Vec<String>,
    pub failures: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedExample {
    pub name: String,
    pub file: String,
}

enum RunOutcome {
    Finished(ExampleResult),
    Failed(String),
}

pub fn example_file_name(name: &str) -> Option<String> {
    let mut slug = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    (!slug.is_empty()).then(|| format!("{slug}.gp"))
}

pub fn plan_examples(names: &[String]) -> Result<Vec<PlannedExample>, OracleError> {
    let mut seen = BTreeSet::new();
    let mut planned = Vec::new();
    for name in names {
        let file = example_file_name(name)
            .ok_or_else(|| OracleError::new(format!("example '{name}' has no usable file name")))?;
        if !seen.insert(file.clone()) {
            return Err(OracleError::new(format!(
                "two examples map to the same file {file}"
            )));
        }
        planned.push(PlannedExample {
            name: name.clone(),
            file,
        });
    }
    Ok(planned)
}

pub fn list_args(out_dir: &str, budget: &[(&'static str, String)]) -> Vec<ScriptArg> {
    let mut args = vec![
        ScriptArg {
            name: "OutDir",
            value: out_dir.to_owned(),
        },
        ScriptArg {
            name: "Mode",
            value: MODE_LIST.to_owned(),
        },
    ];
    args.extend(budget_args(budget));
    args
}

pub fn save_args(
    out_dir: &str,
    example: &str,
    target: &str,
    budget: &[(&'static str, String)],
) -> Vec<ScriptArg> {
    let mut args = vec![
        ScriptArg {
            name: "OutDir",
            value: out_dir.to_owned(),
        },
        ScriptArg {
            name: "Mode",
            value: MODE_SAVE.to_owned(),
        },
        ScriptArg {
            name: "Example",
            value: example.to_owned(),
        },
        ScriptArg {
            name: "Target",
            value: target.to_owned(),
        },
    ];
    args.extend(budget_args(budget));
    args
}

fn budget_args(budget: &[(&'static str, String)]) -> Vec<ScriptArg> {
    budget
        .iter()
        .map(|(name, value)| ScriptArg {
            name,
            value: value.clone(),
        })
        .collect()
}

pub fn parse_example_result(bytes: &[u8]) -> Result<ExampleResult, OracleError> {
    let trimmed = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    serde_json::from_slice(trimmed)
        .map_err(|error| OracleError::new(format!("unreadable result.json: {error}")))
}

pub fn is_inside(root: &Path, candidate: &Path) -> bool {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut probe = candidate.to_path_buf();
    let mut tail = Vec::new();
    while !probe.exists() {
        match (probe.file_name(), probe.parent()) {
            (Some(name), Some(parent)) => {
                tail.push(name.to_owned());
                probe = parent.to_path_buf();
            }
            _ => break,
        }
    }
    let mut resolved = probe.canonicalize().unwrap_or(probe);
    for name in tail.iter().rev() {
        resolved.push(name);
    }
    resolved.starts_with(&root)
}

fn script_path() -> Result<PathBuf, OracleError> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("oracle")
        .join(format!("{EXAMPLE_SCRIPT_NAME}.ps1"));
    path.canonicalize()
        .map_err(|error| OracleError::io(format!("cannot find {}", path.display()), &error))
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

fn run_job(
    host: &Host,
    script: &str,
    job: &Path,
    args: &[ScriptArg],
) -> Result<RunOutcome, OracleError> {
    let job_out = job.join("out");
    let pid_file = job_out.join(PID_FILE);
    let gate = PidGate {
        file: &pid_file,
        wait: host.pid_wait,
    };
    let pairs: Vec<(&str, &str)> = args
        .iter()
        .map(|arg| (arg.name, arg.value.as_str()))
        .collect();
    let run = host.run_script(script, &pairs, &job.join(LOG_FILE), gate)?;
    let status = match run {
        ScriptRun::PidMissing => {
            return Ok(RunOutcome::Failed(format!(
                "no pid recorded within {} ms of starting PowerShell; job kept in {}",
                host.pid_wait.as_millis(),
                job.display()
            )));
        }
        ScriptRun::TimedOut(record) => {
            let stopped = stop_script(host, &record, EXAMPLE_SCRIPT_NAME);
            return Ok(RunOutcome::Failed(format!(
                "timed out after {} ms ({stopped}); job kept in {}",
                host.timeout.as_millis(),
                job.display()
            )));
        }
        ScriptRun::Exited(status) => status,
    };
    let Ok(raw) = std::fs::read(job_out.join(RESULT_FILE)) else {
        let log = std::fs::read(job.join(LOG_FILE)).unwrap_or_default();
        let message = format!(
            "PowerShell exited with {:?} without writing result.json: {}",
            status.code(),
            String::from_utf8_lossy(&log).trim()
        );
        if status.code() == Some(EXIT_GP_NOT_RUNNING) {
            return Err(OracleError::new(message));
        }
        return Ok(RunOutcome::Failed(format!(
            "{message}; job kept in {}",
            job.display()
        )));
    };
    let result = parse_example_result(&raw)?;
    if !result.gp_running {
        return Err(OracleError::new(result.error.unwrap_or_else(|| {
            String::from(
                "Guitar Pro is not running; start it yourself, the harness never launches it",
            )
        })));
    }
    if !result.ok {
        return Ok(RunOutcome::Failed(format!(
            "{}; job kept in {}",
            describe_steps(result.error.as_deref(), &result.steps),
            job.display()
        )));
    }
    Ok(RunOutcome::Finished(result))
}

fn new_job(jobs: &Path, index: usize) -> Result<PathBuf, OracleError> {
    let job = fresh_job_dir(jobs, index)?;
    let out = job.join("out");
    io(std::fs::create_dir(&out), || {
        format!("cannot create {}", out.display())
    })?;
    Ok(job)
}

fn list_examples(host: &Host, script: &str, jobs: &Path) -> Result<ExampleResult, OracleError> {
    let job = new_job(jobs, 0)?;
    let out_win = host.to_windows(&job.join("out"))?;
    let budget = host.budget.script_args();
    match run_job(host, script, &job, &list_args(&out_win, &budget))? {
        RunOutcome::Finished(result) => {
            let _ = std::fs::remove_dir_all(&job);
            Ok(result)
        }
        RunOutcome::Failed(reason) => Err(OracleError::new(format!(
            "cannot list the examples: {reason}"
        ))),
    }
}

fn save_example(
    host: &Host,
    script: &str,
    jobs: &Path,
    index: usize,
    planned: &PlannedExample,
    options: &ExamplesOptions,
) -> Result<Result<ExampleEntry, String>, OracleError> {
    let job = new_job(jobs, index)?;
    let job_out = job.join("out");
    let saved = job_out.join(&planned.file);
    let out_win = host.to_windows(&job_out)?;
    let target_win = host.to_windows(&saved)?;
    let budget = host.budget.script_args();
    let args = save_args(&out_win, &planned.name, &target_win, &budget);
    match run_job(host, script, &job, &args)? {
        RunOutcome::Failed(reason) => Ok(Err(reason)),
        RunOutcome::Finished(result) => {
            for modal in &result.modals_at_start {
                eprintln!(
                    "note: {}: dialog '{modal}' was open at start and was dismissed",
                    planned.name
                );
            }
            let bytes = io(std::fs::read(&saved), || {
                format!("cannot read {}", saved.display())
            })?;
            let destination = options.out.join(&planned.file);
            io(std::fs::write(&destination, &bytes), || {
                format!("cannot write {}", destination.display())
            })?;
            if !options.keep_jobs {
                let _ = std::fs::remove_dir_all(&job);
            }
            Ok(Ok(ExampleEntry {
                name: planned.name.clone(),
                file: planned.file.clone(),
                sha256: sha256_hex(&bytes),
            }))
        }
    }
}

pub fn harvest_examples(
    host: &Host,
    options: &ExamplesOptions,
) -> Result<ExamplesReport, OracleError> {
    if is_inside(&workspace_root(), &options.out) {
        return Err(OracleError::new(format!(
            "{} is inside the repository; Guitar Pro examples are Arobas content and must stay outside it",
            options.out.display()
        )));
    }
    let script = host.to_windows(&script_path()?)?;
    let jobs = host.to_linux(&host.jobs_windows)?;
    io(std::fs::create_dir_all(&jobs), || {
        format!("cannot create {}", jobs.display())
    })?;
    let listing = list_examples(host, &script, &jobs)?;
    if listing.examples.is_empty() {
        return Err(OracleError::new("Guitar Pro listed no examples"));
    }
    let planned = plan_examples(&listing.examples)?;
    io(std::fs::create_dir_all(&options.out), || {
        format!("cannot create {}", options.out.display())
    })?;
    let mut index = ExamplesIndex {
        gp_version: listing.gp_version.clone().unwrap_or_default(),
        examples: Vec::new(),
    };
    let mut report = ExamplesReport::default();
    for (position, example) in planned.iter().enumerate() {
        match save_example(host, &script, &jobs, position + 1, example, options)? {
            Ok(entry) => {
                println!("ok: {} -> {}", example.name, example.file);
                report.saved.push(example.file.clone());
                index.examples.push(entry);
            }
            Err(reason) => {
                eprintln!("failed: {}: {reason}", example.name);
                report.failures.push(example.name.clone());
            }
        }
    }
    let path = options.out.join(EXAMPLES_INDEX);
    let text = serde_json::to_string_pretty(&index)
        .map_err(|error| OracleError::new(format!("cannot encode {EXAMPLES_INDEX}: {error}")))?;
    io(std::fs::write(&path, format!("{text}\n")), || {
        format!("cannot write {}", path.display())
    })?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::{
        MODE_LIST, MODE_SAVE, ScriptArg, example_file_name, is_inside, list_args,
        parse_example_result, plan_examples, save_args,
    };
    use crate::oracle::budget::StepBudget;

    fn pairs(args: &[ScriptArg]) -> Vec<(&str, &str)> {
        args.iter()
            .map(|arg| (arg.name, arg.value.as_str()))
            .collect()
    }

    #[test]
    fn example_names_become_kebab_case_file_names() {
        let cases = [
            ("Amazing Grace", "amazing-grace.gp"),
            ("Adelita (Tarrega)", "adelita-tarrega.gp"),
            (
                "Moon Over Shanghai (Francois Sciortino)",
                "moon-over-shanghai-francois-sciortino.gp",
            ),
            ("All of them Witches", "all-of-them-witches.gp"),
            ("  Twelve  ", "twelve.gp"),
            ("A--B", "a-b.gp"),
            ("Café 7", "caf-7.gp"),
        ];
        for (name, file) in cases {
            assert_eq!(example_file_name(name).as_deref(), Some(file), "{name}");
        }
        for empty in ["", "   ", "()", "ééé"] {
            assert_eq!(example_file_name(empty), None, "{empty:?}");
        }
    }

    #[test]
    fn planning_rejects_collisions_and_unnamed_examples() {
        let names = vec![String::from("Blues"), String::from("Slow Blues")];
        let planned = plan_examples(&names).expect("plan");
        assert_eq!(planned.len(), 2);
        assert_eq!(planned[1].name, "Slow Blues");
        assert_eq!(planned[1].file, "slow-blues.gp");
        let clash = vec![String::from("Hard Rock"), String::from("Hard  Rock!")];
        let error = plan_examples(&clash).expect_err("collision");
        assert!(error.message().contains("hard-rock.gp"));
        assert!(plan_examples(&[String::from("??")]).is_err());
    }

    #[test]
    fn list_arguments_carry_the_mode_and_the_budget() {
        let budget = StepBudget::default().script_args();
        let args = list_args("C:\\gf-oracle\\jobs\\1\\out", &budget);
        assert_eq!(
            pairs(&args),
            vec![
                ("OutDir", "C:\\gf-oracle\\jobs\\1\\out"),
                ("Mode", MODE_LIST),
                ("DialogTimeoutMs", "8000"),
                ("StepTimeoutMs", "60000"),
                ("MaxAttempts", "3"),
            ]
        );
    }

    #[test]
    fn save_arguments_name_the_example_and_the_target() {
        let budget = StepBudget::default().script_args();
        let args = save_args(
            "C:\\j\\out",
            "Adelita (Tarrega)",
            "C:\\j\\out\\adelita-tarrega.gp",
            &budget,
        );
        assert_eq!(
            pairs(&args)[..4],
            [
                ("OutDir", "C:\\j\\out"),
                ("Mode", MODE_SAVE),
                ("Example", "Adelita (Tarrega)"),
                ("Target", "C:\\j\\out\\adelita-tarrega.gp"),
            ]
        );
        assert_eq!(args.len(), 7);
        assert_eq!(args[6].name, "MaxAttempts");
    }

    #[test]
    fn script_modes_match_the_powershell_validate_set() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("oracle")
            .join("Invoke-GpExample.ps1");
        let text = std::fs::read_to_string(path).expect("script readable");
        assert!(text.contains(&format!(
            "[ValidateSet('{MODE_LIST}', '{MODE_SAVE}')][string]$Mode = '{MODE_SAVE}'"
        )));
        for name in ["OutDir", "Example", "Target"] {
            assert!(text.contains(&format!("[string]${name}")), "{name}");
        }
    }

    #[test]
    fn example_result_parses_with_bom_and_lists_names() {
        let raw = "\u{feff}{\"ok\":true,\"gp_running\":true,\"gp_version\":\"8.1.5.31\",\"examples\":[\"Blues\",\"Twelve\"],\"steps\":[],\"extra\":1}";
        let result = parse_example_result(raw.as_bytes()).expect("parse");
        assert!(result.ok);
        assert_eq!(result.examples, vec!["Blues", "Twelve"]);
        assert_eq!(result.gp_version.as_deref(), Some("8.1.5.31"));
        let bare = parse_example_result(b"{\"ok\":false}").expect("parse");
        assert!(bare.examples.is_empty());
        assert!(!bare.gp_running);
        assert!(parse_example_result(b"[").is_err());
    }

    #[test]
    fn output_inside_the_repository_is_detected_even_before_it_exists() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("repo");
        std::fs::create_dir_all(root.join("fixtures")).expect("mkdir");
        assert!(is_inside(&root, &root.join("fixtures")));
        assert!(is_inside(&root, &root.join("new").join("deeper")));
        assert!(is_inside(
            &root,
            &root.join("fixtures").join("..").join("x")
        ));
        assert!(!is_inside(&root, &dir.path().join("corpus")));
        assert!(!is_inside(&root, &dir.path().join("repo-sibling")));
    }
}
