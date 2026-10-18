use std::path::Path;

use xtask::mutants::{
    CRATES, INSTALL_HINT, MutantsReport, Shard, lines_of, mutants_args, output_dir, verdict,
};

#[test]
fn arguments_select_the_five_crates_serially_with_a_timeout_and_target_output() {
    let output = output_dir(Path::new("/work"));
    let args = mutants_args(&output, None, false);
    let expected_output = Path::new("/work")
        .join("target/mutants")
        .display()
        .to_string();
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
            "--package",
            "opentab-layout",
            "--package",
            "opentab-render",
            "--jobs",
            "1",
            "--timeout",
            "120",
            "--output",
            expected_output.as_str(),
        ]
    );
    assert_eq!(
        CRATES,
        [
            "opentab-model",
            "opentab-command",
            "opentab-gp",
            "opentab-layout",
            "opentab-render"
        ]
    );
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
    assert_eq!(
        keys,
        ["build_timeout_multiplier", "cap_lints", "exclude_re"]
    );
    let args = mutants_args(&output_dir(Path::new("/work")), None, false);
    for overridden in ["timeout_multiplier", "minimum_test_timeout", "jobs"] {
        assert!(!table.contains_key(overridden), "{overridden}");
    }
    assert!(args.iter().any(|arg| arg == "--timeout"));
    assert!(!args.iter().any(|arg| arg.starts_with("--build-timeout")));
}

#[test]
fn a_shard_is_passed_through_to_cargo_mutants() {
    let shard = Shard::parse("1/4").expect("valid shard");
    assert_eq!(shard, Shard { index: 1, count: 4 });
    let args = mutants_args(&output_dir(Path::new("/work")), Some(shard), false);
    assert_eq!(args[args.len() - 2..], ["--shard", "1/4"]);
    assert_eq!(
        mutants_args(&output_dir(Path::new("/work")), None, false).len() + 2,
        args.len()
    );
}

#[test]
fn malformed_shards_are_rejected() {
    for text in ["4/4", "5/4", "0/0", "1", "a/4", "1/b", "-1/4", "1/4/2", ""] {
        let error = Shard::parse(text).expect_err(text);
        assert!(error.contains("K/N"), "{text}: {error}");
    }
    assert_eq!(Shard::parse("0/1"), Ok(Shard { index: 0, count: 1 }));
}

#[test]
fn iterate_is_passed_through_to_cargo_mutants() {
    let output = output_dir(Path::new("/work"));
    let plain = mutants_args(&output, None, false);
    let iterating = mutants_args(&output, None, true);
    assert!(!plain.iter().any(|arg| arg == "--iterate"));
    assert_eq!(iterating.last().map(String::as_str), Some("--iterate"));
    assert_eq!(iterating.len(), plain.len() + 1);
    let shard = Shard::parse("2/4").expect("valid shard");
    let both = mutants_args(&output, Some(shard), true);
    assert_eq!(both[both.len() - 3..], ["--shard", "2/4", "--iterate"]);
}

#[test]
fn only_the_proven_equivalent_mutants_are_excluded() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(".cargo")
        .join("mutants.toml");
    let text = std::fs::read_to_string(path).expect("mutants.toml readable");
    let table: toml::Table = text.parse().expect("mutants.toml parses");
    let excluded: Vec<&str> = table["exclude_re"]
        .as_array()
        .expect("exclude_re is a list")
        .iter()
        .filter_map(toml::Value::as_str)
        .collect();
    assert_eq!(
        excluded,
        [
            r"replace /= with \*= in general_format",
            r"replace \| with \^ in accent_bits",
        ]
    );
}
