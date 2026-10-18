mod apply;
mod cli;
mod error;
mod files;
mod info;
mod new;
mod output;

use std::ffi::OsString;
use std::process::ExitCode;

use clap::Parser;

use crate::cli::{Cli, Subcommand};
use crate::error::CliError;

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().collect();
    let json_hint = args.iter().any(|arg| arg.to_str() == Some("--json"));
    let cli = match Cli::try_parse_from(&args) {
        Ok(cli) => cli,
        Err(error) => return output::usage(&error, json_hint),
    };
    let json = cli.command.json();
    match run(cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => output::failure(&error, json),
    }
}

fn run(command: Subcommand) -> Result<(), CliError> {
    match command {
        Subcommand::New { out, title, json } => output::emit(&new::run(&out, title)?, json),
        Subcommand::Info { path, json } => output::emit(&info::run(&path)?, json),
        Subcommand::Apply {
            path,
            commands,
            out,
            json,
        } => output::emit(&apply::run(&path, &commands, &out)?, json),
    }
    Ok(())
}
