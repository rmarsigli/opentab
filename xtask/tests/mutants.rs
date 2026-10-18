use std::path::Path;

use xtask::mutants::{
    CRATES, INSTALL_HINT, MutantsReport, lines_of, mutants_args, output_dir, verdict,
};

#[test]
fn arguments_select_the_three_crates_serially_with_a_timeout_and_target_output() {
    let output = output_dir(Path::new("/work"));
    let args = mutants_args(&output);
    assert_eq!(
        args,
        [
            "mutants",
            "--package",
            "opentab-model",
            "--package",
            "opentab-command",
            "--package",
            "opentab-gp",
            "--jobs",
            "1",
            "--timeout",
            "120",
            "--output",
            "/work/target/mutants",
        ]
    );
    assert_eq!(CRATES, ["opentab-model", "opentab-command", "opentab-gp"]);
}

#[test]
fn install_hint_names_the_pinned_version() {
    assert!(INSTALL_HINT.contains("cargo install cargo-mutants"));
    assert!(INSTALL_HINT.contains("27.1.0"));
}

#[test]
fn lines_are_trimmed_and_blank_lines_dropped() {
    assert_eq!(lines_of("a\n\n  b  \r\n \n"), ["a", "b"]);
    assert!(lines_of("").is_empty());
}

#[test]
fn report_reads_missed_and_timeout_lists() {
    let dir = tempfile::tempdir().unwrap();
    let results = dir.path().join("mutants.out");
    std::fs::create_dir_all(&results).unwrap();
    std::fs::write(results.join("missed.txt"), "src/a.rs:1:2: replace x\n").unwrap();
    std::fs::write(
        results.join("timeout.txt"),
        "src/b.rs:3:4: replace y\nsrc/c.rs:5:6: z\n",
    )
    .unwrap();
    let report = MutantsReport::read(dir.path());
    assert_eq!(report.missed, ["src/a.rs:1:2: replace x"]);
    assert_eq!(report.timeouts.len(), 2);
    assert!(!report.is_clean());
    let text = report.render();
    assert!(text.starts_with("1 missed, 2 timed out"));
    assert!(text.contains("missed: src/a.rs:1:2: replace x"));
    assert!(text.contains("timeout: src/c.rs:5:6: z"));
}

#[test]
fn missing_or_empty_result_files_mean_a_clean_report() {
    let dir = tempfile::tempdir().unwrap();
    assert!(MutantsReport::read(dir.path()).is_clean());
    let results = dir.path().join("mutants.out");
    std::fs::create_dir_all(&results).unwrap();
    std::fs::write(results.join("missed.txt"), "").unwrap();
    std::fs::write(results.join("timeout.txt"), "\n").unwrap();
    assert_eq!(MutantsReport::read(dir.path()), MutantsReport::default());
}

#[test]
fn verdict_needs_a_successful_tool_and_a_clean_report() {
    let clean = MutantsReport::default();
    let missed = MutantsReport {
        missed: vec!["m".to_owned()],
        timeouts: Vec::new(),
    };
    let slow = MutantsReport {
        missed: Vec::new(),
        timeouts: vec!["t".to_owned()],
    };
    assert!(verdict(true, &clean));
    assert!(!verdict(false, &clean));
    assert!(!verdict(true, &missed));
    assert!(!verdict(true, &slow));
}

#[test]
fn mutants_config_keeps_only_settings_the_arguments_do_not_override() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(".cargo")
        .join("mutants.toml");
    let text = std::fs::read_to_string(path).expect("mutants.toml readable");
    let table: toml::Table = text.parse().expect("mutants.toml parses");
    let keys: Vec<&str> = table.keys().map(String::as_str).collect();
    assert_eq!(keys, ["build_timeout_multiplier", "cap_lints"]);
    let args = mutants_args(&output_dir(Path::new("/work")));
    for overridden in ["timeout_multiplier", "minimum_test_timeout", "jobs"] {
        assert!(!table.contains_key(overridden), "{overridden}");
    }
    assert!(args.iter().any(|arg| arg == "--timeout"));
    assert!(!args.iter().any(|arg| arg.starts_with("--build-timeout")));
}
