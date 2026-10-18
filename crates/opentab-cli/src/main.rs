mod apply;
mod cli;
mod convert;
mod error;
mod files;
mod ids;
mod info;
mod new;
mod output;
mod render;
mod text;

use std::ffi::OsString;
use std::process::ExitCode;

use clap::Parser;

use crate::cli::{Cli, Subcommand};
use crate::error::CliError;
use crate::render::RenderRequest;

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
        Subcommand::Info { path, ids, json } => output::emit(&info::run(&path, ids)?, json),
        Subcommand::Apply {
            path,
            commands,
            out,
            json,
        } => output::emit(&apply::run(&path, &commands, &out)?, json),
        Subcommand::Convert { path, out, json } => output::emit(&convert::run(&path, &out)?, json),
        Subcommand::Render {
            path,
            format,
            out,
            scale,
            json,
        } => {
            let request = RenderRequest {
                path: &path,
                format,
                out: &out,
                scale,
            };
            output::emit(&render::run(&request)?, json)
        }
    }
}
