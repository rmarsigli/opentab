use crate::oracle::budget::StepBudget;
use crate::oracle::error::OracleError;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

pub const POWERSHELL_ENV: &str = "OPENTAB_ORACLE_POWERSHELL";
pub const WSLPATH_ENV: &str = "OPENTAB_ORACLE_WSLPATH";
pub const JOBS_ENV: &str = "OPENTAB_ORACLE_JOBS";
pub const TIMEOUT_ENV: &str = "OPENTAB_ORACLE_TIMEOUT_MS";
pub const PID_WAIT_ENV: &str = "OPENTAB_ORACLE_PID_WAIT_MS";
pub const DEFAULT_JOBS: &str = "C:\\gf-oracle\\jobs";
pub const KILL_NOT_MATCHED_EXIT: i32 = 3;
pub const SCRIPT_NAME: &str = "Invoke-GpExport";
const POLL: Duration = Duration::from_millis(50);
const KILL_WAIT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct Host {
    pub powershell: PathBuf,
    pub wslpath: PathBuf,
    pub jobs_windows: String,
    pub budget: StepBudget,
    pub timeout: Duration,
    pub pid_wait: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PidRecord {
    pub pid: u32,
    pub start_filetime: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillOutcome {
    Stopped,
    NotMatched,
    Unavailable,
}

impl PidRecord {
    pub fn parse(text: &str) -> Option<Self> {
        let mut lines = text.trim_start_matches('\u{feff}').lines();
        let pid = lines.next()?.trim().parse::<u32>().ok()?;
        let start_filetime = lines
            .next()
            .and_then(|line| line.trim().parse::<i64>().ok());
        Some(Self {
            pid,
            start_filetime,
        })
    }

    pub fn kill_command(&self) -> String {
        let pid = self.pid;
        match self.start_filetime {
            Some(start) => format!(
                "$p = Get-Process -Id {pid} -ErrorAction SilentlyContinue; \
                 if ($p -and $p.ProcessName -eq 'powershell' -and $p.StartTime.ToFileTimeUtc() -eq {start}) \
                 {{ Stop-Process -Id {pid} -Force; exit 0 }}; exit {KILL_NOT_MATCHED_EXIT}"
            ),
            None => format!(
                "$p = Get-CimInstance Win32_Process -Filter 'ProcessId = {pid}' -ErrorAction SilentlyContinue; \
                 if ($p -and $p.Name -eq 'powershell.exe' -and $p.CommandLine -like '*{SCRIPT_NAME}*') \
                 {{ Stop-Process -Id {pid} -Force; exit 0 }}; exit {KILL_NOT_MATCHED_EXIT}"
            ),
        }
    }
}

#[derive(Debug)]
pub enum ScriptRun {
    Exited(ExitStatus),
    TimedOut(PidRecord),
    PidMissing,
}

#[derive(Debug, Clone, Copy)]
pub struct PidGate<'a> {
    pub file: &'a Path,
    pub wait: Duration,
}

enum PidWait {
    Recorded(PidRecord),
    Exited(ExitStatus),
    Missing,
}

pub fn read_complete_pid(path: &Path) -> Option<PidRecord> {
    let text = std::fs::read_to_string(path).ok()?;
    if text.ends_with('\n') {
        PidRecord::parse(&text)
    } else {
        None
    }
}

fn wait_for_pid(child: &mut Child, gate: PidGate<'_>) -> Result<PidWait, OracleError> {
    let deadline = Instant::now() + gate.wait;
    loop {
        if let Some(record) = read_complete_pid(gate.file) {
            return Ok(PidWait::Recorded(record));
        }
        match child.try_wait() {
            Ok(Some(status)) => return Ok(PidWait::Exited(status)),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL),
            Ok(None) => return Ok(PidWait::Missing),
            Err(error) => return Err(OracleError::io("cannot wait for PowerShell", &error)),
        }
    }
}

fn resolve_program(name: &OsString) -> Option<PathBuf> {
    let candidate = PathBuf::from(name);
    if candidate.components().count() > 1 {
        return candidate.is_file().then_some(candidate);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(&candidate))
        .find(|full| full.is_file())
}

impl Host {
    pub fn from_env() -> Result<Self, OracleError> {
        let requested =
            std::env::var_os(POWERSHELL_ENV).unwrap_or_else(|| OsString::from("powershell.exe"));
        let powershell = resolve_program(&requested).ok_or_else(|| {
            OracleError::new(format!(
                "oracle harvest needs Windows PowerShell, but {} was not found (set {POWERSHELL_ENV} to override); \
                 it only runs on a Windows host or WSL with Windows interop",
                requested.to_string_lossy()
            ))
        })?;
        let requested_wslpath =
            std::env::var_os(WSLPATH_ENV).unwrap_or_else(|| OsString::from("wslpath"));
        let wslpath = resolve_program(&requested_wslpath).ok_or_else(|| {
            OracleError::new(format!(
                "oracle harvest needs wslpath, but {} was not found (set {WSLPATH_ENV} to override)",
                requested_wslpath.to_string_lossy()
            ))
        })?;
        let jobs_windows = std::env::var(JOBS_ENV).unwrap_or_else(|_| DEFAULT_JOBS.to_owned());
        let budget = StepBudget::default();
        let timeout = match std::env::var(TIMEOUT_ENV) {
            Ok(text) => Duration::from_millis(text.parse::<u64>().map_err(|_| {
                OracleError::new(format!(
                    "{TIMEOUT_ENV} must be a number of milliseconds, got {text}"
                ))
            })?),
            Err(_) => budget.backstop(),
        };
        let pid_wait = match std::env::var(PID_WAIT_ENV) {
            Ok(text) => Duration::from_millis(text.parse::<u64>().map_err(|_| {
                OracleError::new(format!(
                    "{PID_WAIT_ENV} must be a number of milliseconds, got {text}"
                ))
            })?),
            Err(_) => budget.startup(),
        };
        Ok(Self {
            powershell,
            wslpath,
            jobs_windows,
            pid_wait,
            budget,
            timeout,
        })
    }

    fn wslpath(&self, flag: &str, path: &std::ffi::OsStr) -> Result<String, OracleError> {
        let output = Command::new(&self.wslpath)
            .arg(flag)
            .arg(path)
            .stdin(Stdio::null())
            .output()
            .map_err(|error| OracleError::io("cannot run wslpath", &error))?;
        let text = String::from_utf8(output.stdout)
            .map_err(|_| OracleError::new("wslpath printed a non UTF-8 path"))?;
        let trimmed = text.trim_end_matches(['\r', '\n']).to_owned();
        if !output.status.success() || trimmed.is_empty() {
            return Err(OracleError::new(format!(
                "wslpath {flag} {} failed: {}",
                path.to_string_lossy(),
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(trimmed)
    }

    pub fn to_windows(&self, path: &Path) -> Result<String, OracleError> {
        self.wslpath("-w", path.as_os_str())
    }

    pub fn to_linux(&self, path: &str) -> Result<PathBuf, OracleError> {
        self.wslpath("-u", std::ffi::OsStr::new(path))
            .map(PathBuf::from)
    }

    pub fn run_script(
        &self,
        script: &str,
        args: &[(&str, &str)],
        log: &Path,
        gate: PidGate<'_>,
    ) -> Result<ScriptRun, OracleError> {
        let log_file = std::fs::File::create(log)
            .map_err(|error| OracleError::io(format!("cannot create {}", log.display()), &error))?;
        let err_file = log_file
            .try_clone()
            .map_err(|error| OracleError::io("cannot duplicate log handle", &error))?;
        let mut command = Command::new(&self.powershell);
        command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(script);
        for (name, value) in args {
            command.arg(format!("-{name}")).arg(value);
        }
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::from(log_file))
            .stderr(Stdio::from(err_file))
            .spawn()
            .map_err(|error| {
                OracleError::io(
                    format!("cannot start {}", self.powershell.display()),
                    &error,
                )
            })?;
        let record = match wait_for_pid(&mut child, gate)? {
            PidWait::Recorded(record) => record,
            PidWait::Exited(status) => return Ok(ScriptRun::Exited(status)),
            PidWait::Missing => {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(ScriptRun::PidMissing);
            }
        };
        let deadline = Instant::now() + self.timeout;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return Ok(ScriptRun::Exited(status)),
                Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL),
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(ScriptRun::TimedOut(record));
                }
                Err(error) => return Err(OracleError::io("cannot wait for PowerShell", &error)),
            }
        }
    }

    pub fn stop_windows_process(&self, record: &PidRecord) -> KillOutcome {
        let mut child = match Command::new(&self.powershell)
            .args(["-NoProfile", "-NonInteractive", "-Command"])
            .arg(record.kill_command())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(_) => return KillOutcome::Unavailable,
        };
        let deadline = Instant::now() + KILL_WAIT;
        while Instant::now() < deadline {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return KillOutcome::Stopped,
                Ok(Some(status)) if status.code() == Some(KILL_NOT_MATCHED_EXIT) => {
                    return KillOutcome::NotMatched;
                }
                Ok(Some(_)) | Err(_) => return KillOutcome::Unavailable,
                Ok(None) => std::thread::sleep(POLL),
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        KillOutcome::Unavailable
    }
}

#[cfg(test)]
mod tests {
    use super::{KILL_NOT_MATCHED_EXIT, PidRecord, read_complete_pid};

    #[test]
    fn pid_file_parses_with_and_without_start_time_and_bom() {
        let both = PidRecord::parse("\u{feff}4242\r\n133000000000000000\r\n").expect("both");
        assert_eq!(both.pid, 4242);
        assert_eq!(both.start_filetime, Some(133_000_000_000_000_000));
        let bare = PidRecord::parse("77").expect("bare");
        assert_eq!(bare.start_filetime, None);
        assert!(PidRecord::parse("").is_none());
        assert!(PidRecord::parse("abc\n1").is_none());
        assert!(PidRecord::parse("-5").is_none());
    }

    #[test]
    fn pid_file_counts_only_when_completely_written() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("pid.txt");
        assert!(read_complete_pid(&path).is_none());
        for partial in ["", "42", "4242\n13300", "abc\n", "\n"] {
            std::fs::write(&path, partial).expect("write");
            assert!(read_complete_pid(&path).is_none(), "{partial:?}");
        }
        for complete in ["4242\n", "4242\n133000000000000000\n", "\u{feff}7\r\n9\r\n"] {
            std::fs::write(&path, complete).expect("write");
            assert!(read_complete_pid(&path).is_some(), "{complete:?}");
        }
    }

    #[test]
    fn kill_by_start_time_checks_name_and_start_time_never_name_alone() {
        let command = PidRecord {
            pid: 4242,
            start_filetime: Some(99),
        }
        .kill_command();
        assert!(command.contains("Get-Process -Id 4242"));
        assert!(command.contains("$p.ProcessName -eq 'powershell'"));
        assert!(command.contains("$p.StartTime.ToFileTimeUtc() -eq 99"));
        assert!(command.contains("Stop-Process -Id 4242 -Force; exit 0"));
        assert!(command.ends_with(&format!("exit {KILL_NOT_MATCHED_EXIT}")));
    }

    #[test]
    fn kill_without_start_time_requires_the_script_in_the_command_line() {
        let command = PidRecord {
            pid: 7,
            start_filetime: None,
        }
        .kill_command();
        assert!(command.contains("Win32_Process"));
        assert!(command.contains("ProcessId = 7"));
        assert!(command.contains("$p.CommandLine -like '*Invoke-GpExport*'"));
        assert!(!command.contains("StartTime"));
    }
}
