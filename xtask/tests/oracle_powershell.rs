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
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stdout}{stderr}");
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
    assert_eq!(module.matches("& $script:WindowCloser").count(), 1);
    assert!(start < module.find("& $script:WindowCloser").unwrap());
    assert!(module.find("& $script:WindowCloser").unwrap() < end);
}

fn function_body(source: &str, name: &str) -> String {
    let start = source
        .find(&format!("function {name}"))
        .unwrap_or_else(|| panic!("function {name}"));
    let end = source[start + 1..]
        .find("\nfunction ")
        .map_or(source.len(), |offset| start + 1 + offset);
    source[start..end].to_owned()
}

#[test]
fn clear_gp_modals_only_closes_modal_classes_and_probes_popups_cheaply() {
    let module = oracle_source("GpOracle.psm1");
    let clear = function_body(&module, "Clear-GpModals");
    assert!(clear.contains("Get-GpModals"));
    assert!(!clear.contains("Test-ModalClass"));
    assert!(clear.contains("Test-GpPopupPresent"));
    assert!(!clear.contains("Get-GpPopups"));
    let close_menus = function_body(&module, "Close-GpMenus");
    assert!(!close_menus.contains("Get-GpPopups"));
    let select = function_body(&module, "Select-GpModals");
    assert!(select.contains("Test-ModalClass"));
    let classes = function_body(&module, "Test-ModalClass");
    assert!(classes.contains("'Qt*QWindowIcon'"));
    assert!(classes.contains("'#32770'"));
    assert!(!classes.contains("Tool"));
    let open = function_body(&module, "Open-MenuPath");
    assert!(open.contains("Get-GpPopups"));
    assert!(!open.contains("Test-GpPopupPresent"));
    let popups = function_body(&module, "Get-GpPopups");
    let guard = popups.find("$popups.Count -eq 0").expect("guard");
    assert!(guard < popups.find("FromHandle").expect("uia"));
}

#[test]
fn explorer_folders_are_only_closed_under_the_jobs_root() {
    let module = oracle_source("GpOracle.psm1");
    let body = function_body(&module, "Close-ExplorerFolder");
    let guard = body.find("Test-JobFolder $Folder").expect("guard");
    assert!(guard < body.find("New-Object -ComObject").expect("com"));
    assert!(module.contains("$script:JobsRoot = 'C:\\gf-oracle\\jobs'"));
}

#[test]
fn powershell_sources_use_classes_and_carry_no_comments() {
    for name in [
        "GpOracle.psm1",
        "Invoke-GpExport.ps1",
        "tests/Test-GpOracle.ps1",
    ] {
        let source = oracle_source(name);
        assert!(!source.contains("pscustomobject"), "{name}");
        assert!(!source.contains("<#"), "{name}");
        for line in source.lines() {
            assert!(!line.trim_start().starts_with('#'), "{name}: {line}");
        }
    }
    let module = oracle_source("GpOracle.psm1");
    let first_class = module
        .find("\nclass ")
        .or_else(|| module.starts_with("class ").then_some(0));
    let first_function = module.find("\nfunction ").expect("functions");
    assert!(first_class.expect("classes declared") < first_function);
    assert!(module.starts_with("class "));
}

#[test]
fn module_never_synthesises_keyboard_input_outside_the_guarded_sender() {
    let module = oracle_source("GpOracle.psm1");
    assert!(!module.contains("keybd_event"));
    assert!(!module.contains("SendInput"));
    assert_eq!(module.matches("SendKeys]::SendWait").count(), 1);
    let start = module.find("function Send-KeysTo").expect("sender");
    let end = module[start + 1..]
        .find("\nfunction ")
        .map_or(module.len(), |offset| start + 1 + offset);
    let body = &module[start..end];
    assert!(body.contains("GetForegroundWindow() -eq $Handle"));
    assert!(body.find("GetForegroundWindow").unwrap() < body.find("SendWait").unwrap());
    let entry = oracle_source("Invoke-GpExport.ps1");
    assert!(!entry.contains("SendKeys"));
}

#[test]
fn pid_file_is_published_by_rename_only() {
    let entry = oracle_source("Invoke-GpExport.ps1");
    assert!(!entry.contains("Write-Utf8File (Join-Path $OutDir 'pid.txt')"));
    let temp = entry.find("Write-Utf8File $PidTemp").expect("temp write");
    let publish = entry
        .find("Move-Item -LiteralPath $PidTemp -Destination (Join-Path $OutDir 'pid.txt') -Force")
        .expect("rename");
    assert!(temp < publish);
}
