use std::process::ExitCode;

const USAGE: &str = "usage: cargo xtask <command>\n\ncommands:\n  (none yet)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("help" | "--help" | "-h") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("unknown command: {other}\n\n{USAGE}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    fn load_config() -> toml::Table {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(".cargo")
            .join("config.toml");
        let text = std::fs::read_to_string(&path).expect("config.toml readable");
        text.parse::<toml::Table>().expect("config.toml parses")
    }

    #[test]
    fn memory_guard_present() {
        let config = load_config();
        let jobs = config
            .get("build")
            .and_then(|build| build.get("jobs"))
            .and_then(toml::Value::as_integer);
        assert_eq!(jobs, Some(2));
        let threads = config
            .get("env")
            .and_then(|env| env.get("RUST_TEST_THREADS"))
            .and_then(toml::Value::as_str);
        assert_eq!(threads, Some("2"));
        let dev = config
            .get("profile")
            .and_then(|profile| profile.get("dev"))
            .expect("profile.dev");
        assert_eq!(
            dev.get("debug").and_then(toml::Value::as_str),
            Some("line-tables-only")
        );
        let deps = dev
            .get("package")
            .and_then(|package| package.get("*"))
            .expect("profile.dev.package.*");
        assert_eq!(
            deps.get("opt-level").and_then(toml::Value::as_integer),
            Some(1)
        );
        assert_eq!(
            deps.get("debug").and_then(toml::Value::as_bool),
            Some(false)
        );
    }

    #[test]
    fn xtask_alias_present() {
        let config = load_config();
        let alias = config
            .get("alias")
            .and_then(|alias| alias.get("xtask"))
            .and_then(toml::Value::as_str);
        assert_eq!(alias, Some("run --package xtask --"));
    }
}
