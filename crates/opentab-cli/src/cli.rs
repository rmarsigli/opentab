use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
#[command(name = "opentab", version, about = "OpenTab command line")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Subcommand,
}

#[derive(clap::Subcommand)]
pub enum Subcommand {
    New {
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Info {
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Apply {
        path: PathBuf,
        commands: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        json: bool,
    },
}

impl Subcommand {
    pub fn json(&self) -> bool {
        match self {
            Subcommand::New { json, .. }
            | Subcommand::Info { json, .. }
            | Subcommand::Apply { json, .. } => *json,
        }
    }
}
