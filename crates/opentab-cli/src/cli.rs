use std::path::PathBuf;

use clap::Parser;

const ABOUT: &str = "OpenTab command line";

const LONG_ABOUT: &str = "OpenTab command line\n\n\
Create, inspect and edit OpenTab scores. Scores are stored as .opentab.json files.\n\n\
Exit codes: 0 success, 1 usage error, 2 input or output error. With --json every \
result and every error is a single JSON line on standard output.";

const NEW_ABOUT: &str = "Create a blank one-bar guitar score";

const NEW_LONG_ABOUT: &str = "Create a blank score: one 4/4 bar at 120 bpm and one six-string \
guitar track in standard tuning, with a whole-note rest.\n\n\
The output path must end in .opentab.json. An existing file is never overwritten.";

const NEW_AFTER_HELP: &str = "Examples:\n  opentab new --out song.opentab.json --title \"My song\"\n  opentab new --out song.opentab.json --json\n\n\
JSON result: {\"created\": \"<path>\"}";

const INFO_ABOUT: &str = "Print a summary of a score and its diagnostics";

const INFO_LONG_ABOUT: &str = "Print the title, artist, tracks, master bar count, time signatures \
and diagnostics of a score.\n\n\
With --ids the human output also lists the id of every master bar, track, staff, bar, voice, \
beat and note. The JSON output always contains these ids in an \"ids\" section.";

const INFO_AFTER_HELP: &str = "Examples:\n  opentab info song.opentab.json\n  opentab info song.opentab.json --ids\n  opentab info song.opentab.json --json";

const APPLY_ABOUT: &str = "Apply a file of JSON commands to a score";

const APPLY_LONG_ABOUT: &str = "Read PATH, apply every command in COMMANDS and write the result \
to --out. COMMANDS holds one JSON command per line; blank lines and lines starting with // \
are ignored and line numbers count them.\n\n\
Nothing is written unless every command succeeds. --out may be the input file itself and \
replaces an existing file atomically; a symbolic link at --out is kept and its target is \
written.";

const APPLY_AFTER_HELP: &str = "Examples:\n  opentab apply song.opentab.json edits.jsonl --out edited.opentab.json\n  opentab apply song.opentab.json edits.jsonl --out song.opentab.json --json\n\n\
JSON result: {\"applied\": <count>, \"allocated\": [<new ids in order>]}\n\
JSON errors: {\"error\": {\"kind\", \"sub_kind\", \"message\", \"line\"}}; sub_kind appears on \
command_failed: not_found, index_out_of_range, invalid_value or wrong_instrument.";

#[derive(Parser)]
#[command(name = "opentab", version, about = ABOUT, long_about = LONG_ABOUT)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Subcommand,
}

#[derive(clap::Subcommand)]
pub enum Subcommand {
    #[command(about = NEW_ABOUT, long_about = NEW_LONG_ABOUT, after_help = NEW_AFTER_HELP)]
    New {
        #[arg(long, value_name = "FILE", help = "Path of the score to create")]
        out: PathBuf,
        #[arg(long, help = "Title stored in the score")]
        title: Option<String>,
        #[arg(long, help = "Print the result as JSON")]
        json: bool,
    },
    #[command(about = INFO_ABOUT, long_about = INFO_LONG_ABOUT, after_help = INFO_AFTER_HELP)]
    Info {
        #[arg(value_name = "PATH", help = "Score to read")]
        path: PathBuf,
        #[arg(long, help = "Also list the ids of every element")]
        ids: bool,
        #[arg(long, help = "Print the result as JSON")]
        json: bool,
    },
    #[command(about = APPLY_ABOUT, long_about = APPLY_LONG_ABOUT, after_help = APPLY_AFTER_HELP)]
    Apply {
        #[arg(value_name = "PATH", help = "Score to read")]
        path: PathBuf,
        #[arg(value_name = "COMMANDS", help = "File with one JSON command per line")]
        commands: PathBuf,
        #[arg(long, value_name = "FILE", help = "Path of the edited score")]
        out: PathBuf,
        #[arg(long, help = "Print the result as JSON")]
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
