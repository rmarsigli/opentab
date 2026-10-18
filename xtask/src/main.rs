use clap::{Args, Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use xtask::fixtures::write_fixtures;
use xtask::mutants::{INSTALL_HINT, MutantsReport, mutants_args, output_dir, verdict};
use xtask::oracle::check::check;
use xtask::oracle::examples::{ExamplesOptions, harvest_examples};
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
    #[command(about = "Run cargo-mutants on opentab-model and opentab-command")]
    Mutants,
    #[command(about = "Write the synthetic .gp fixtures, one per feature family")]
    Fixtures(FixturesArgs),
}

#[derive(Debug, Args)]
struct FixturesArgs {
    #[arg(long, help = "Directory that receives the .gp fixtures")]
    out: PathBuf,
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
    #[command(about = "Save every built-in Guitar Pro 8 example as a .gp into a local corpus")]
    Examples(ExamplesArgs),
}

#[derive(Debug, Args)]
struct ExamplesArgs {
    #[arg(long, help = "Corpus directory outside the repository")]
    out: PathBuf,
    #[arg(long, help = "Keep C:\\gf-oracle\\jobs\\<id> after a successful save")]
    keep_jobs: bool,
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

fn run_examples(args: ExamplesArgs) -> ExitCode {
    let host = match Host::from_env() {
        Ok(host) => host,
        Err(error) => return failure(error.message()),
    };
    let options = ExamplesOptions {
        out: args.out,
        keep_jobs: args.keep_jobs,
    };
    match harvest_examples(&host, &options) {
        Ok(report) if report.failures.is_empty() => {
            println!("saved {} example(s)", report.saved.len());
            ExitCode::SUCCESS
        }
        Ok(report) => failure(&format!(
            "{} of {} example(s) failed: {}",
            report.failures.len(),
            report.failures.len() + report.saved.len(),
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

fn run_fixtures(args: FixturesArgs) -> ExitCode {
    match write_fixtures(&args.out) {
        Ok(paths) => {
            println!("wrote {} fixture(s) to {}", paths.len(), args.out.display());
            ExitCode::SUCCESS
        }
        Err(error) => failure(error.message()),
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

fn cargo_mutants_installed(root: &Path) -> bool {
    Command::new("cargo")
        .args(["mutants", "--version"])
        .current_dir(root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn run_mutants() -> ExitCode {
    let root = workspace_root();
    if !cargo_mutants_installed(&root) {
        return failure(INSTALL_HINT);
    }
    let output = output_dir(&root);
    if let Err(error) = std::fs::create_dir_all(&output) {
        return failure(&format!("cannot create {}: {error}", output.display()));
    }
    let status = match Command::new("cargo")
        .args(mutants_args(&output))
        .current_dir(&root)
        .status()
    {
        Ok(status) => status,
        Err(error) => return failure(&format!("cannot run cargo mutants: {error}")),
    };
    let report = MutantsReport::read(&output);
    println!("{}", report.render());
    if verdict(status.success(), &report) {
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "error: cargo mutants did not finish clean (exit {}); results in {}",
            status
                .code()
                .map_or_else(|| "signal".to_owned(), |code| code.to_string()),
            output.join("mutants.out").display()
        );
        ExitCode::from(1)
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        TopCommand::Mutants => run_mutants(),
        TopCommand::Fixtures(args) => run_fixtures(args),
        TopCommand::Oracle(oracle) => match oracle.command {
            OracleCommand::Harvest(args) => run_harvest(args),
            OracleCommand::Check(args) => run_check(args),
            OracleCommand::Examples(args) => run_examples(args),
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
