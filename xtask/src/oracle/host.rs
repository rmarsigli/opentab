use crate::oracle::error::OracleError;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

pub const POWERSHELL_ENV: &str = "OPENTAB_ORACLE_POWERSHELL";
pub const WSLPATH_ENV: &str = "OPENTAB_ORACLE_WSLPATH";
pub const JOBS_ENV: &str = "OPENTAB_ORACLE_JOBS";
pub const TIMEOUT_ENV: &str = "OPENTAB_ORACLE_TIMEOUT_SECS";
pub const DEFAULT_JOBS: &str = "C:\\gf-oracle\\jobs";
pub const DEFAULT_TIMEOUT_SECS: u64 = 300;
const POLL: Duration = Duration::from_millis(200);

#[derive(Debug, Clone)]
pub struct Host {
    pub powershell: PathBuf,
    pub wslpath: PathBuf,
    pub jobs_windows: String,
    pub timeout: Duration,
}

#[derive(Debug)]
pub struct ScriptRun {
    pub status: Option<ExitStatus>,
    pub timed_out: bool,
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
        let timeout_secs = match std::env::var(TIMEOUT_ENV) {
            Ok(text) => text.parse::<u64>().map_err(|_| {
                OracleError::new(format!(
                    "{TIMEOUT_ENV} must be a number of seconds, got {text}"
                ))
            })?,
            Err(_) => DEFAULT_TIMEOUT_SECS,
        };
        Ok(Self {
            powershell,
            wslpath,
            jobs_windows,
            timeout: Duration::from_secs(timeout_secs),
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
        let deadline = Instant::now() + self.timeout;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    return Ok(ScriptRun {
                        status: Some(status),
                        timed_out: false,
                    });
                }
                Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL),
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(ScriptRun {
                        status: None,
                        timed_out: true,
                    });
                }
                Err(error) => return Err(OracleError::io("cannot wait for PowerShell", &error)),
            }
        }
    }

    pub fn stop_windows_pid(&self, pid: u32) {
        let mut child = match Command::new(&self.powershell)
            .args(["-NoProfile", "-NonInteractive", "-Command"])
            .arg(format!(
                "Get-Process -Id {pid} -ErrorAction SilentlyContinue | Where-Object {{ $_.ProcessName -eq 'powershell' }} | Stop-Process -Force"
            ))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(_) => return,
        };
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if let Ok(Some(_)) = child.try_wait() {
                return;
            }
            std::thread::sleep(POLL);
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}
