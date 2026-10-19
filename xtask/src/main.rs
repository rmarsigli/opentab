use clap::{Args, Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, ExitStatus, Stdio};
use xtask::cap::{CommandLine, LINUX, UNAVAILABLE_WARNING, cap_applicable, probe, wrap};
use xtask::fixtures::write_fixtures;
use xtask::mutants::{INSTALL_HINT, MutantsReport, Shard, mutants_args, output_dir, verdict};
use xtask::oracle::check::check;
use xtask::oracle::examples::{ExamplesOptions, harvest_examples};
use xtask::oracle::harvest::{HarvestOptions, harvest};
use xtask::oracle::host::Host;
use xtask::oracle::publish::{PublishOptions, publish};

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
    #[command(
        about = "Run cargo-mutants on opentab-model, opentab-command, opentab-gp, opentab-layout, opentab-render, opentab-mcp, opentab-files, opentab-playback and opentab-audio"
    )]
    Mutants(MutantsArgs),
    #[command(about = "Run a command inside the memory cap (5G memory, 1G swap)")]
    Capped(CappedArgs),
    #[command(about = "Write the synthetic .gp fixtures, one per feature family")]
    Fixtures(FixturesArgs),
}

#[derive(Debug, Args)]
struct MutantsArgs {
    #[arg(
        long,
        value_parser = Shard::parse,
        help = "Run only shard K of N (K/N, from 0/N), passed to cargo mutants --shard"
    )]
    shard: Option<Shard>,
    #[arg(
        long,
        help = "Retest only the mutants the previous run missed or left unknown, passed to cargo mutants --iterate"
    )]
    iterate: bool,
}

#[derive(Debug, Args)]
struct CappedArgs {
    #[arg(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        required = true,
        value_name = "COMMAND"
    )]
    command: Vec<String>,
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
    #[command(
        about = "Copy the committable golden set (resave.gpif, MIDI, MusicXML, GP5, PNG) out of a full harvest"
    )]
    Publish(PublishArgs),
}

#[derive(Debug, Args)]
struct PublishArgs {
    #[arg(long, help = "Full harvest directory, kept outside the repository")]
    from: PathBuf,
    #[arg(long, help = "Golden directory to commit")]
    out: PathBuf,
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

fn run_publish(args: PublishArgs) -> ExitCode {
    let options = PublishOptions {
        from: args.from,
        out: args.out,
    };
    match publish(&options) {
        Ok(count) => {
            println!("published {count} fixture(s) to {}", options.out.display());
            ExitCode::SUCCESS
        }
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

fn cap_available() -> bool {
    if std::env::consts::OS != LINUX {
        return false;
    }
    let probe = probe();
    let works = Command::new(&probe.program)
        .args(&probe.args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    cap_applicable(std::env::consts::OS, works)
}

fn launch(command: CommandLine, current_dir: Option<&Path>) -> std::io::Result<ExitStatus> {
    let available = cap_available();
    if !available {
        eprintln!("{UNAVAILABLE_WARNING}");
    }
    let wrapped = wrap(command, available);
    let mut process = Command::new(&wrapped.program);
    process.args(&wrapped.args);
    if let Some(dir) = current_dir {
        process.current_dir(dir);
    }
    process.status()
}

fn run_capped(args: CappedArgs) -> ExitCode {
    let Some((program, rest)) = args.command.split_first() else {
        return failure("capped needs a command to run");
    };
    match launch(CommandLine::new(program, rest), None) {
        Ok(status) => status.code().map_or(ExitCode::from(1), |code| {
            ExitCode::from(u8::try_from(code).unwrap_or(1))
        }),
        Err(error) => failure(&format!("cannot run {program}: {error}")),
    }
}

fn run_mutants(args: MutantsArgs) -> ExitCode {
    let root = workspace_root();
    if !cargo_mutants_installed(&root) {
        return failure(INSTALL_HINT);
    }
    let output = output_dir(&root);
    if let Err(error) = std::fs::create_dir_all(&output) {
        return failure(&format!("cannot create {}: {error}", output.display()));
    }
    let command = CommandLine::new("cargo", &mutants_args(&output, args.shard, args.iterate));
    let status = match launch(command, Some(&root)) {
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
        TopCommand::Mutants(args) => run_mutants(args),
        TopCommand::Capped(args) => run_capped(args),
        TopCommand::Fixtures(args) => run_fixtures(args),
        TopCommand::Oracle(oracle) => match oracle.command {
            OracleCommand::Harvest(args) => run_harvest(args),
            OracleCommand::Check(args) => run_check(args),
            OracleCommand::Examples(args) => run_examples(args),
            OracleCommand::Publish(args) => run_publish(args),
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
