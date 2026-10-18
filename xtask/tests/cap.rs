use xtask::cap::{
    CommandLine, MEMORY_MAX, MEMORY_SWAP_MAX, OOM_POLICY, SYSTEMD_RUN, cap_applicable,
    cap_arguments, capped, probe, wrap,
};

fn words(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

#[test]
fn the_limits_are_five_gigabytes_of_memory_and_one_of_swap() {
    assert_eq!(MEMORY_MAX, "5G");
    assert_eq!(MEMORY_SWAP_MAX, "1G");
    assert_eq!(
        cap_arguments(),
        words(&[
            "--user",
            "--scope",
            "-q",
            "-p",
            "MemoryMax=5G",
            "-p",
            "MemorySwapMax=1G",
            "-p",
            "OOMPolicy=continue"
        ])
    );
}

#[test]
fn an_out_of_memory_kill_stops_only_the_offending_process() {
    assert_eq!(OOM_POLICY, "continue");
    assert!(cap_arguments().contains(&format!("OOMPolicy={OOM_POLICY}")));
}

#[test]
fn a_capped_command_runs_under_a_user_scope_and_keeps_its_arguments_in_order() {
    let command = CommandLine::new("cargo", &words(&["test", "--workspace"]));
    let wrapped = capped(&command);
    assert_eq!(wrapped.program, SYSTEMD_RUN);
    let mut expected = cap_arguments();
    expected.extend(words(&["cargo", "test", "--workspace"]));
    assert_eq!(wrapped.args, expected);
}

#[test]
fn wrapping_is_skipped_when_the_cap_is_unavailable() {
    let command = CommandLine::new("cargo", &words(&["mutants"]));
    assert_eq!(wrap(command.clone(), false), command);
    assert_eq!(wrap(command.clone(), true), capped(&command));
}

#[test]
fn the_cap_needs_linux_and_a_working_systemd_run() {
    assert!(cap_applicable("linux", true));
    assert!(!cap_applicable("linux", false));
    assert!(!cap_applicable("windows", true));
    assert!(!cap_applicable("macos", true));
}

#[test]
fn the_probe_runs_a_harmless_command_under_the_same_cap() {
    let mut expected = cap_arguments();
    expected.push("true".to_owned());
    let probe = probe();
    assert_eq!(probe.program, SYSTEMD_RUN);
    assert_eq!(probe.args, expected);
}
