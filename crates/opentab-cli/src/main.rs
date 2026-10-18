use clap::Parser;

#[derive(Parser)]
#[command(name = "opentab", version, about = "OpenTab command line")]
struct Cli {}

fn main() {
    Cli::parse();
}
