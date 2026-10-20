mod apply;
mod audio;
mod bank_store;
mod capture;
mod catalogue_file;
mod cli;
mod convert;
mod error;
mod export;
mod families;
mod ids;
mod info;
mod new;
mod outfile;
mod output;
#[cfg(feature = "live")]
#[path = "play_live.rs"]
mod play;
#[cfg(not(feature = "live"))]
#[path = "play_off.rs"]
mod play;
#[cfg(any(feature = "live", test))]
mod playing;
mod render;
mod sound;
mod soundbank;
mod text;

use std::ffi::OsString;
use std::process::ExitCode;

use clap::Parser;

use crate::cli::{Cli, MidiFormat, Subcommand};
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
        Subcommand::Export {
            path,
            format: MidiFormat::Mid,
            profile,
            out,
            json,
        } => output::emit(&export::run(&path, profile, &out)?, json),
        Subcommand::Audio { playback, out } => {
            output::emit(&audio::run(&playback, &out)?, playback.json)
        }
        Subcommand::Play { playback } => play::run(&playback),
        Subcommand::Soundbank {
            catalogue,
            json,
            action,
        } => soundbank::run(catalogue.as_deref(), action, json),
        Subcommand::Render {
            path,
            format,
            out,
            scale,
            tracks,
            bars,
            staff,
            page,
            json,
        } => {
            let request = RenderRequest {
                path: &path,
                format,
                out: &out,
                scale,
                tracks,
                bars,
                staff,
                page,
            };
            output::emit(&render::run(&request)?, json)
        }
    }
}
