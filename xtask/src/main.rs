use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;
use xtask::oracle::check::check;
use xtask::oracle::harvest::{HarvestOptions, harvest};
use xtask::oracle::host::Host;

#[derive(Debug, Parser)]
#[command(
    name = "cargo xtask",
    about = "OpenTab project tasks",
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: TopCommand,
}

#[derive(Debug, Subcommand)]
enum TopCommand {
    #[command(about = "Guitar Pro 8 oracle: harvest goldens and check their freshness")]
    Oracle(OracleArgs),
}

#[derive(Debug, Args)]
struct OracleArgs {
    #[command(subcommand)]
    command: OracleCommand,
}

#[derive(Debug, Subcommand)]
enum OracleCommand {
    #[command(about = "Run fixtures through Guitar Pro 8 and write goldens")]
    Harvest(HarvestArgs),
    #[command(about = "Report goldens that are stale, missing or orphaned")]
    Check(CheckArgs),
}

#[derive(Debug, Args)]
struct HarvestArgs {
    #[arg(long, help = "A Guitar Pro file or a directory of them")]
    input: PathBuf,
    #[arg(long, help = "Golden output directory")]
    out: PathBuf,
    #[arg(
        long,
        help = "Keep C:\\gf-oracle\\jobs\\<id> after a successful export"
    )]
    keep_jobs: bool,
}

#[derive(Debug, Args)]
struct CheckArgs {
    #[arg(long, help = "Fixture directory")]
    input: PathBuf,
    #[arg(long, help = "Golden directory holding manifest.json")]
    goldens: PathBuf,
}

fn failure(message: &str) -> ExitCode {
    eprintln!("error: {message}");
    ExitCode::from(2)
}

fn run_harvest(args: HarvestArgs) -> ExitCode {
    let host = match Host::from_env() {
        Ok(host) => host,
        Err(error) => return failure(error.message()),
    };
    let options = HarvestOptions {
        input: args.input,
        out: args.out,
        keep_jobs: args.keep_jobs,
    };
    match harvest(&host, &options) {
        Ok(report) if report.failures.is_empty() => {
            println!("harvested {} fixture(s)", report.harvested.len());
            ExitCode::SUCCESS
        }
        Ok(report) => failure(&format!(
            "{} of {} fixture(s) failed: {}",
            report.failures.len(),
            report.failures.len() + report.harvested.len(),
            report.failures.join(", ")
        )),
        Err(error) => failure(error.message()),
    }
}

fn run_check(args: CheckArgs) -> ExitCode {
    match check(&args.input, &args.goldens) {
        Ok(findings) if findings.is_empty() => {
            println!("all goldens are fresh");
            ExitCode::SUCCESS
        }
        Ok(findings) => {
            for finding in &findings {
                println!("{finding}");
            }
            ExitCode::from(2)
        }
        Err(error) => failure(error.message()),
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        TopCommand::Oracle(oracle) => match oracle.command {
            OracleCommand::Harvest(args) => run_harvest(args),
            OracleCommand::Check(args) => run_check(args),
        },
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
