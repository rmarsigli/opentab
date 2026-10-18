use crate::support::run;

#[test]
fn a_missing_argument_exits_1() {
    let result = run(&["info"]);
    assert_eq!(result.code, 1);
    assert!(result.stderr.contains("Usage"), "{}", result.stderr);
    assert_eq!(result.stdout, "");
    assert_eq!(run(&["new"]).code, 1);
    assert_eq!(run(&["apply", "a.opentab.json", "c.jsonl"]).code, 1);
}

#[test]
fn no_arguments_and_unknown_subcommands_exit_1() {
    assert_eq!(run(&[]).code, 1);
    assert_eq!(run(&["frobnicate"]).code, 1);
    assert_eq!(run(&["info", "a.opentab.json", "--bogus"]).code, 1);
}

#[test]
fn usage_errors_with_json_go_to_stdout() {
    let result = run(&["info", "--json"]);
    assert_eq!(result.code, 1);
    assert_eq!(result.stderr, "");
    let body = result.json();
    assert_eq!(body["error"]["kind"], "usage");
    assert!(body["error"].get("line").is_none());
}

#[test]
fn help_and_version_exit_0() {
    let help = run(&["--help"]);
    assert_eq!(help.code, 0);
    assert!(help.stdout.contains("apply"));
    assert_eq!(run(&["info", "--help"]).code, 0);
    assert_eq!(run(&["--version"]).code, 0);
}

#[test]
fn subcommand_help_documents_arguments_examples_and_json_shapes() {
    let new = run(&["new", "--help"]);
    assert_eq!(new.code, 0);
    for needle in [
        "--out",
        "--title",
        "--json",
        "never overwritten",
        "Examples:",
        "\"created\"",
    ] {
        assert!(new.stdout.contains(needle), "new: {needle}\n{}", new.stdout);
    }
    let info = run(&["info", "--help"]);
    assert_eq!(info.code, 0);
    for needle in ["--ids", "--json", "\"ids\" section", "Examples:"] {
        assert!(
            info.stdout.contains(needle),
            "info: {needle}\n{}",
            info.stdout
        );
    }
    let apply = run(&["apply", "--help"]);
    assert_eq!(apply.code, 0);
    for needle in [
        "COMMANDS",
        "--out",
        "\"allocated\"",
        "sub_kind",
        "wrong_instrument",
        "symbolic link",
        "Examples:",
    ] {
        assert!(
            apply.stdout.contains(needle),
            "apply: {needle}\n{}",
            apply.stdout
        );
    }
}

#[test]
fn top_level_help_lists_exit_codes() {
    let help = run(&["--help"]);
    assert!(help.stdout.contains("Exit codes"), "{}", help.stdout);
    assert!(help.stdout.contains("new"));
    assert!(help.stdout.contains("info"));
}
