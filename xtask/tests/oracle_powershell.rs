use std::path::{Path, PathBuf};
use std::process::Command;

fn on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|full| full.is_file())
}

fn windows_path(wslpath: &Path, path: &Path) -> String {
    let output = Command::new(wslpath)
        .arg("-w")
        .arg(path)
        .output()
        .expect("run wslpath");
    String::from_utf8(output.stdout)
        .expect("utf8")
        .trim_end()
        .to_owned()
}

#[test]
fn module_never_closes_unowned_or_main_windows() {
    let (Some(powershell), Some(wslpath)) = (on_path("powershell.exe"), on_path("wslpath")) else {
        eprintln!("skipped: Windows PowerShell is not reachable from this host");
        return;
    };
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("oracle")
        .join("tests")
        .join("Test-GpOracle.ps1")
        .canonicalize()
        .expect("test script exists");
    let output = Command::new(powershell)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(windows_path(&wslpath, &script))
        .output()
        .expect("run powershell");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{stdout}");
    assert!(stdout.contains("PASS: "), "{stdout}");
}

fn oracle_source(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("oracle")
        .join(name);
    std::fs::read_to_string(&path).expect("oracle source readable")
}

#[test]
fn wm_close_is_only_sent_by_the_guarded_helper() {
    let entry = oracle_source("Invoke-GpExport.ps1");
    assert!(!entry.contains("0x0010"));
    assert!(!entry.contains("PostMessage"));
    let module = oracle_source("GpOracle.psm1");
    let sites: Vec<usize> = module.match_indices("0x0010").map(|(at, _)| at).collect();
    assert_eq!(sites.len(), 1);
    let start = module
        .find("function Close-OwnedWindow")
        .expect("guarded helper");
    let end = module[start + 1..]
        .find("\nfunction ")
        .map_or(module.len(), |offset| start + 1 + offset);
    assert!(start < sites[0] && sites[0] < end);
    let body = &module[start..end];
    assert!(body.contains("Assert-MainKnown $Main"));
    assert!(body.contains("Test-CloseAllowed"));
    assert_eq!(module.matches("PostMessage(").count(), 3);
}
