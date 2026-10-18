use std::path::PathBuf;

use clap::Parser;
use opentab_render::Ratio;

use crate::render::{BarSpan, ImageFormat, StaffChoice, parse_bars, parse_scale};

const ABOUT: &str = "OpenTab command line";

const LONG_ABOUT: &str = "OpenTab command line\n\n\
Create, inspect and edit OpenTab scores. Scores are stored as .opentab.json files or read from and written to Guitar Pro .gp files.\n\n\
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
beat and note. The JSON output always contains these ids in an \"ids\" section. PATH may be a \
.gp or a .opentab.json.";

const INFO_AFTER_HELP: &str = "Examples:\n  opentab info song.opentab.json\n  opentab info song.opentab.json --ids\n  opentab info song.opentab.json --json\n  opentab info song.gp";

const APPLY_ABOUT: &str = "Apply a file of JSON commands to a score";

const APPLY_LONG_ABOUT: &str = "Read PATH, apply every command in COMMANDS and write the result \
to --out. COMMANDS holds one JSON command per line; blank lines and lines starting with // \
are ignored and line numbers count them.\n\n\
Nothing is written unless every command succeeds. --out may be the input file itself and \
replaces an existing file atomically; a symbolic link at --out is kept and its target is \
written. PATH and --out may each be a .gp or a .opentab.json; an edit of a .gp is written as a .gp \
unless --out says otherwise.";

const APPLY_AFTER_HELP: &str = "Examples:\n  opentab apply song.opentab.json edits.jsonl --out edited.opentab.json\n  opentab apply song.opentab.json edits.jsonl --out song.opentab.json --json\n  opentab apply song.gp edits.jsonl --out edited.gp\n\n\
JSON result: {\"applied\": <count>, \"allocated\": [<new ids in order>]}\n\
JSON errors: {\"error\": {\"kind\", \"sub_kind\", \"message\", \"line\"}}; sub_kind appears on \
command_failed: not_found, index_out_of_range, invalid_value or wrong_instrument.";

const CONVERT_ABOUT: &str = "Convert a score between .gp and .opentab.json";

const CONVERT_LONG_ABOUT: &str = "Read PATH and write it to OUT. The format of each file is picked \
by its suffix: .gp or .opentab.json, in any letter case and in any combination, so a .gp can \
also be rewritten as a .gp.\n\n\
Nothing is written unless the whole score converts. OUT replaces an existing file atomically; \
a symbolic link at OUT is kept and its target is written. A score the .gp writer cannot \
represent is refused with the error kind unsupported.";

const CONVERT_AFTER_HELP: &str = "Examples:\n  opentab convert song.gp song.opentab.json\n  opentab convert song.opentab.json song.gp --json\n\n\
JSON result: {\"converted\": {\"from\": \"<path>\", \"to\": \"<path>\"}}";

const RENDER_ABOUT: &str = "Render a score to SVG or PNG pages";

const RENDER_LONG_ABOUT: &str = "Read PATH, lay it out on A4 portrait pages and write one SVG or \
PNG file per page. PATH may be a .gp or a .opentab.json.\n\n\
When --out is an existing directory the pages are written into it, named after PATH: \
<stem>.<ext> for one page and <stem>-p<N>.<ext> for several. Otherwise --out must end in the \
suffix of --format and its directory must exist; one page is written to --out itself and several \
pages to <out-stem>-p<N>.<ext> next to it. Existing files are replaced atomically.\n\n\
--page N writes only page N, counted from 1: to --out itself, or into the directory under the \
name it would have in a full render. The JSON still counts every page of the layout.\n\n\
--scale applies to PNG only: 1 means 96 pixels per inch, so an A4 page is 794x1123 pixels. \
It may be written as 2, 1.5 or 3/2.\n\n\
Every track is drawn unless --track picks some; tracks are numbered from 1 as in `opentab info`, \
and the option may be repeated. --bars A-B (or a single A) draws only the master bars A to B, \
counted from 1. --staff forces the staves of every fretted track: standard notation, tablature \
or both; by default a guitar-family track (General MIDI programs 25-40 and 105-108, counted from 1) with 4 to 8 strings gets both and other tracks get standard notation.\n\n\
The staff space is 1.75 mm unless a system is too tall for the page or a bar too wide for a \
line; then it shrinks, down to 0.9 mm, and the output says so. A bar still too wide at 0.9 mm \
is squeezed until its notes may touch and is listed as crowded. A system still too tall at \
0.9 mm is the error layout_limit.\n\n\
Drum tracks get a percussion clef and one line per General MIDI drum class. Notation that \
is read but not drawn for the chosen tracks, bars and staves is listed under \"unrendered\": \
features out of scope (lyrics, chord diagrams, fermatas...), effects the staff mode hides (bends \
or palm mute with --staff standard, accents with --staff tab), score texts such as the copyright, \
and characters the bundled font cannot draw. \"unrendered\", \"dropped\" and \"crowded\" describe \
the whole selected layout, every page of it, also with --page. A PNG may hold at most 60 million \
pixels; lower --scale for larger pages.";

const RENDER_AFTER_HELP: &str = "Examples:\n  opentab render song.gp --format svg --out song.svg\n  opentab render song.opentab.json --format png --scale 2 --out pages/\n  opentab render song.gp --format png --track 2 --bars 5-12 --staff tab --out riff.png\n  opentab render song.gp --format svg --page 2 --out page2.svg --json\n\n\
JSON result: {\"pages\": <count>, \"systems\": <count>, \"bars\": <count>, \"staff_space\": <micrometres>, \"crowded\": [<bar numbers from 1>], \"dropped\": [{\"bar\": <number from 1>, \"kind\": \"title|tempo|section|chord|text|dynamic\"}], \"title_page\": <true when page 1 holds only the title block>, \"unrendered\": [<sorted feature names, such as \"bends\", \"fermatas\", \"lyrics\">], \"files\": [<written paths in page order>]}\n\
A dropped title block has no \"bar\": {\"kind\": \"title\"}.";

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
    #[command(about = CONVERT_ABOUT, long_about = CONVERT_LONG_ABOUT, after_help = CONVERT_AFTER_HELP)]
    Convert {
        #[arg(value_name = "PATH", help = "Score to read")]
        path: PathBuf,
        #[arg(value_name = "OUT", help = "Path of the converted score")]
        out: PathBuf,
        #[arg(long, help = "Print the result as JSON")]
        json: bool,
    },
    #[command(about = RENDER_ABOUT, long_about = RENDER_LONG_ABOUT, after_help = RENDER_AFTER_HELP)]
    Render {
        #[arg(value_name = "PATH", help = "Score to read")]
        path: PathBuf,
        #[arg(long, value_enum, help = "Image format of the pages")]
        format: ImageFormat,
        #[arg(
            long,
            value_name = "FILE_OR_DIR",
            help = "Page file, or directory for the pages"
        )]
        out: PathBuf,
        #[arg(long, value_name = "F", value_parser = parse_scale, help = "PNG scale, 1 is 96 pixels per inch")]
        scale: Option<Ratio<u32>>,
        #[arg(long = "track", value_name = "N", value_parser = clap::value_parser!(u32).range(1..), help = "Track to draw, counted from 1; repeat for several")]
        tracks: Vec<u32>,
        #[arg(long, value_name = "A-B", value_parser = parse_bars, help = "Master bars to draw, counted from 1")]
        bars: Option<BarSpan>,
        #[arg(long, value_enum, help = "Staves of fretted tracks")]
        staff: Option<StaffChoice>,
        #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(1..), help = "Render only page N, counted from 1")]
        page: Option<u32>,
        #[arg(long, help = "Print the result as JSON")]
        json: bool,
    },
}

impl Subcommand {
    pub fn json(&self) -> bool {
        match self {
            Subcommand::New { json, .. }
            | Subcommand::Info { json, .. }
            | Subcommand::Apply { json, .. }
            | Subcommand::Convert { json, .. }
            | Subcommand::Render { json, .. } => *json,
        }
    }
}
