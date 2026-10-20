use std::path::PathBuf;

use clap::Parser;
use opentab_audio::{BankId, BankRef};
use opentab_model::InstrumentFamily;
use opentab_render::{BarSpan, Ratio, parse_bars, parse_scale};

use crate::families::parse_family;
use crate::render::{ImageFormat, StaffChoice};
use crate::sound::ProfileChoice;

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

const BARS_SEMANTICS: &str = "--bars A-B (or a single A) picks SOURCE bars: the master bars of the score, counted from 1 as in `opentab info` and `render --bars`. Repeats, endings and jumps are followed, so the sound is the first contiguous run of played bars whose source bar lies in A-B: in a score played as 1 2 3 2 3 4, --bars 2-3 is the first pass, played bars 2 and 3, and --bars 4 is the last bar. A range none of whose bars is ever played is an error.";

const EXPORT_ABOUT: &str = "Export a score as a Standard MIDI file";

const EXPORT_LONG_ABOUT: &str = "Read PATH and write its performance as a Standard MIDI File (format 1) to --out, which must end in .mid. PATH may be a .gp or a .opentab.json. An existing file is replaced atomically.\n\n\
--profile picks how the score is performed. gp follows what Guitar Pro itself writes (480 ticks per quarter, one MIDI track per staff, bends in Guitar Pro's steps). musical (the default) reads the score as a musician would (960 ticks per quarter, one MIDI track per score track, finer tempo ramps). Both expand repeats, endings and jumps.\n\n\
Features of the score the chosen profile does not sound are listed as unplayed.";

const EXPORT_AFTER_HELP: &str = "Examples:\n  opentab export song.gp --format mid --profile gp --out song.mid\n  opentab export song.opentab.json --format mid --out song.mid --json\n\n\
JSON result: {\"files\": [\"<path>\"], \"bytes\": <size>, \"profile\": \"gp|musical\", \"ppq\": <ticks per quarter>, \"seconds\": <duration>, \"played_bars\": [<source bar numbers from 1, in played order>], \"unplayed\": [{\"kind\", \"track\": <number from 1 or null>, \"first_bar\": <number from 1>, \"count\"}], \"warnings\": [{\"kind\", \"track\", \"at\": \"<whole notes>\", \"detail\"}]}";

const AUDIO_ABOUT: &str = "Render a score to a WAV file";

const AUDIO_LONG_ABOUT: &str = "Read PATH, perform it and write 16-bit stereo audio at 48 kHz to --out, which must end in .wav. PATH may be a .gp or a .opentab.json. An existing file is replaced atomically. The sound ends 3 seconds after the music so that the last notes can fade; a render may last 20 minutes at most.\n\n\
--track N (repeatable, counted from 1) plays only those tracks. --speed P is a percentage from 25 to 200 (default 100) of the written tempo. --metronome adds a click on every beat. --count-in plays one bar of clicks before the music. --profile gp|musical picks the performance rules, as in `opentab export` (default musical).\n\n\
Every track uses the sound bank resolved for it: the score's own override for that track, else the bank assigned to its instrument family (`opentab soundbank assign`), else the built-in synthesizer. --soundbank ID plays every track with that bank instead (built-in is a valid ID). A bank that is missing or cannot be loaded is replaced by the built-in synthesizer and the output says so, for every track, under \"fallback\".\n\n\
Sound banks live in the data folder: $OPENTAB_DATA_DIR when set, else the platform's data folder for opentab.";

const AUDIO_AFTER_HELP: &str = "Examples:\n  opentab audio song.gp --out song.wav\n  opentab audio song.gp --out riff.wav --track 2 --bars 5-12 --speed 75 --count-in --metronome\n  opentab audio song.gp --out song.wav --soundbank musescore-general --profile gp --json\n\n\
JSON result: {\"files\": [\"<path>\"], \"seconds\": <duration>, \"frames\": <count>, \"sample_rate\": 48000, \"profile\": \"gp|musical\", \"speed\": <percent>, \"tracks\": [<track numbers from 1 that sound>], \"bars\": {\"first\": <A>, \"last\": <B>} or null, \"played_bars\": [<source bar numbers from 1, in played order>], \"backends\": [{\"track\": <number from 1>, \"format\": \"built_in|sf2|sfz\", \"bank\": \"<id or built-in>\", \"fallback\": <why another bank was wanted, or null>}], \"unplayed\": [...], \"warnings\": [...]}\n\
\"unplayed\" and \"warnings\" describe the whole score as performed, as in `opentab export`.";

const PLAY_ABOUT: &str = "Play a score through the sound card";

const PLAY_LONG_ABOUT: &str = "Perform PATH live through the default audio output and wait until it ends. It takes the same options as `opentab audio`, except --out. Press Ctrl-C to stop.\n\n\
Live output needs a build with the `live` feature (cargo build -p opentab-cli --features live); without it the command fails with the error kind live_unavailable. Use `opentab audio` to write a WAV file instead.";

const PLAY_AFTER_HELP: &str = "Examples:\n  opentab play song.gp\n  opentab play song.gp --track 1 --bars 1-8 --speed 60 --count-in\n\n\
JSON result: {\"seconds\": <duration>, \"profile\", \"speed\", \"tracks\", \"bars\", \"backends\", \"unplayed\", \"warnings\"} as in `opentab audio`, printed when playing ends.";

const SOUNDBANK_ABOUT: &str = "Manage sound banks (development and agent use)";

const SOUNDBANK_LONG_ABOUT: &str = "List, import, install, remove and assign sound banks (SF2 and SFZ). People normally do this in the OpenTab app; this command is for development and for agents.\n\n\
Banks live in the data folder: $OPENTAB_DATA_DIR when set, else the platform's data folder for opentab. --catalogue FILE replaces the built-in catalogue with a JSON file (an array of entries with the fields id, name, format sf2|sfz, archive raw|zip|7z, url, sha256, size, licence_name, licence_url, licence_text, attribution, covers), for mirrors and tests; downloads are checked against its sha256 and need https, or http to a loopback address.";

const SOUNDBANK_AFTER_HELP: &str = "Examples:\n  opentab soundbank list\n  opentab soundbank import ~/Downloads/MyFont.sf2\n  opentab soundbank catalogue\n  opentab soundbank install musescore-general\n  opentab soundbank assign guitar spanish-guitar\n  opentab soundbank remove spanish-guitar --json\n\n\
JSON results: list {\"banks\": [{\"id\", \"name\", \"format\", \"source\": \"imported|catalogue\", \"bytes\", \"path\", \"instruments\": <count>}], \"assignments\": {\"<family>\": \"<bank>\"}, \"problems\": [...]}; import {\"imported\": <bank>}; catalogue {\"catalogue\": [{\"id\", \"name\", \"format\", \"archive\", \"size\", \"licence\", \"covers\", \"installed\": <bool>}]}; install {\"installed\": <bank>}; remove {\"removed\": \"<id>\"}; assign {\"assigned\": {\"instrument\": \"<family>\", \"bank\": \"<bank>\"}}.";

const SOUNDBANK_LIST_ABOUT: &str = "List installed sound banks and instrument assignments";
const SOUNDBANK_IMPORT_ABOUT: &str = "Import a .sf2 file, a .zip, or a folder holding an SFZ bank";
const SOUNDBANK_CATALOGUE_ABOUT: &str = "List the banks that can be installed";
const SOUNDBANK_INSTALL_ABOUT: &str = "Download and install a bank from the catalogue";
const SOUNDBANK_REMOVE_ABOUT: &str = "Remove an installed bank";
const SOUNDBANK_ASSIGN_ABOUT: &str = "Use a bank for every track of an instrument family";
const SOUNDBANK_ASSIGN_LONG_ABOUT: &str = "Assign BANK to every track of INSTRUMENT, where INSTRUMENT is an instrument family: piano, chromatic_percussion, organ, guitar, bass, strings, ensemble, brass, reed, pipe, synth_lead, synth_pad, synth_effects, ethnic, percussive, sound_effects or drums. BANK is an installed bank id, or built-in. A score can still override the bank of a single track. Removing a bank keeps the assignment; tracks then use the built-in synthesizer and `opentab audio` reports the fallback.";

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
    #[command(about = EXPORT_ABOUT, long_about = EXPORT_LONG_ABOUT, after_help = EXPORT_AFTER_HELP)]
    Export {
        #[arg(value_name = "PATH", help = "Score to read")]
        path: PathBuf,
        #[arg(long, value_enum, help = "File format")]
        format: MidiFormat,
        #[arg(long, value_enum, default_value_t = ProfileChoice::Musical, help = "Performance rules")]
        profile: ProfileChoice,
        #[arg(long, value_name = "FILE", help = "Path of the .mid file")]
        out: PathBuf,
        #[arg(long, help = "Print the result as JSON")]
        json: bool,
    },
    #[command(about = AUDIO_ABOUT, long_about = AUDIO_LONG_ABOUT, after_help = AUDIO_AFTER_HELP)]
    Audio {
        #[command(flatten)]
        playback: PlaybackArgs,
        #[arg(long, value_name = "FILE", help = "Path of the .wav file")]
        out: PathBuf,
    },
    #[command(about = PLAY_ABOUT, long_about = PLAY_LONG_ABOUT, after_help = PLAY_AFTER_HELP)]
    Play {
        #[command(flatten)]
        playback: PlaybackArgs,
    },
    #[command(about = SOUNDBANK_ABOUT, long_about = SOUNDBANK_LONG_ABOUT, after_help = SOUNDBANK_AFTER_HELP)]
    Soundbank {
        #[arg(
            long,
            global = true,
            value_name = "FILE",
            help = "Read the catalogue from this JSON file"
        )]
        catalogue: Option<PathBuf>,
        #[arg(long, global = true, help = "Print the result as JSON")]
        json: bool,
        #[command(subcommand)]
        action: SoundbankAction,
    },
}

#[derive(clap::Args)]
pub struct PlaybackArgs {
    #[arg(value_name = "PATH", help = "Score to perform")]
    pub path: PathBuf,
    #[arg(long = "track", value_name = "N", value_parser = clap::value_parser!(u32).range(1..), help = "Track to play, counted from 1; repeat for several")]
    pub tracks: Vec<u32>,
    #[arg(long, value_name = "A-B", value_parser = parse_bars, help = "Source bars to play, counted from 1; repeats are followed", long_help = BARS_SEMANTICS)]
    pub bars: Option<BarSpan>,
    #[arg(long, value_name = "PERCENT", default_value_t = 100, value_parser = clap::value_parser!(u16).range(25..=200), help = "Speed as a percentage of the written tempo, 25 to 200")]
    pub speed: u16,
    #[arg(long, help = "Click on every beat")]
    pub metronome: bool,
    #[arg(long, help = "Play one bar of clicks first")]
    pub count_in: bool,
    #[arg(long, value_enum, default_value_t = ProfileChoice::Musical, help = "Performance rules")]
    pub profile: ProfileChoice,
    #[arg(long, value_name = "ID", value_parser = parse_bank, help = "Play every track with this sound bank, or built-in")]
    pub soundbank: Option<BankRef>,
    #[arg(long, help = "Print the result as JSON")]
    pub json: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, clap::ValueEnum)]
pub enum MidiFormat {
    Mid,
}

#[derive(clap::Subcommand)]
pub enum SoundbankAction {
    #[command(about = SOUNDBANK_LIST_ABOUT)]
    List,
    #[command(about = SOUNDBANK_IMPORT_ABOUT)]
    Import {
        #[arg(value_name = "PATH", help = "A .sf2 file, a .zip, or a bank folder")]
        path: PathBuf,
    },
    #[command(about = SOUNDBANK_CATALOGUE_ABOUT)]
    Catalogue,
    #[command(about = SOUNDBANK_INSTALL_ABOUT)]
    Install {
        #[arg(value_name = "ID", value_parser = parse_bank_id, help = "Catalogue id")]
        id: BankId,
    },
    #[command(about = SOUNDBANK_REMOVE_ABOUT)]
    Remove {
        #[arg(value_name = "ID", value_parser = parse_bank_id, help = "Installed bank id")]
        id: BankId,
    },
    #[command(about = SOUNDBANK_ASSIGN_ABOUT, long_about = SOUNDBANK_ASSIGN_LONG_ABOUT)]
    Assign {
        #[arg(value_name = "INSTRUMENT", value_parser = parse_family, help = "Instrument family, such as guitar or drums")]
        instrument: InstrumentFamily,
        #[arg(value_name = "BANK", value_parser = parse_bank, help = "Installed bank id, or built-in")]
        bank: BankRef,
    },
}

fn parse_bank(text: &str) -> Result<BankRef, String> {
    BankRef::parse(text).map_err(|error| error.to_string())
}

fn parse_bank_id(text: &str) -> Result<BankId, String> {
    BankId::new(text).map_err(|error| error.to_string())
}

impl Subcommand {
    pub fn json(&self) -> bool {
        match self {
            Subcommand::New { json, .. }
            | Subcommand::Info { json, .. }
            | Subcommand::Apply { json, .. }
            | Subcommand::Convert { json, .. }
            | Subcommand::Render { json, .. }
            | Subcommand::Export { json, .. }
            | Subcommand::Soundbank { json, .. } => *json,
            Subcommand::Audio { playback, .. } | Subcommand::Play { playback } => playback.json,
        }
    }
}
