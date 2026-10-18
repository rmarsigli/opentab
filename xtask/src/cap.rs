pub const MEMORY_MAX: &str = "5G";
pub const MEMORY_SWAP_MAX: &str = "1G";
pub const OOM_POLICY: &str = "continue";
pub const SYSTEMD_RUN: &str = "systemd-run";
pub const LINUX: &str = "linux";
pub const UNAVAILABLE_WARNING: &str = "warning: memory cap unavailable (needs Linux and a working `systemd-run --user`); running uncapped";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandLine {
    pub program: String,
    pub args: Vec<String>,
}

impl CommandLine {
    pub fn new(program: &str, args: &[String]) -> CommandLine {
        CommandLine {
            program: program.to_owned(),
            args: args.to_vec(),
        }
    }
}

pub fn cap_arguments() -> Vec<String> {
    vec![
        "--user".to_owned(),
        "--scope".to_owned(),
        "-q".to_owned(),
        "-p".to_owned(),
        format!("MemoryMax={MEMORY_MAX}"),
        "-p".to_owned(),
        format!("MemorySwapMax={MEMORY_SWAP_MAX}"),
        "-p".to_owned(),
        format!("OOMPolicy={OOM_POLICY}"),
    ]
}

pub fn cap_applicable(os: &str, systemd_run_works: bool) -> bool {
    os == LINUX && systemd_run_works
}

pub fn capped(command: &CommandLine) -> CommandLine {
    let mut args = cap_arguments();
    args.push(command.program.clone());
    args.extend(command.args.iter().cloned());
    CommandLine {
        program: SYSTEMD_RUN.to_owned(),
        args,
    }
}

pub fn wrap(command: CommandLine, cap_available: bool) -> CommandLine {
    if cap_available {
        capped(&command)
    } else {
        command
    }
}

pub fn probe() -> CommandLine {
    capped(&CommandLine::new("true", &[]))
}
