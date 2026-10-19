# OpenTab

OpenTab is an open-source guitar tablature and notation editor written in Rust, built as a free replacement for Guitar Pro. It is designed around a headless core and a single JSON command protocol, so the command line, an MCP server and the desktop app all edit scores the same way. The goal for 1.0 is to open, play, edit and save Guitar Pro 7 and 8 files without data loss.

## Status

Pre-alpha. The workspace, the domain model and the command layer are being built. Nothing is usable for real work yet, and the name OpenTab is provisional.

## Build

OpenTab pins Rust 1.92 through `rust-toolchain.toml`; `rustup` installs it automatically.

```
cargo build --workspace
cargo test --workspace
cargo run -p opentab-cli -- --version
```

The repository ships a `.cargo/config.toml` that limits build parallelism to two jobs and reduces debug information, so the workspace builds on machines with 8 GB of RAM.

## Command line

The `opentab` binary works on `.opentab.json` scores and on Guitar Pro `.gp` files. The format of every path is picked by its suffix: `.gp` or `.opentab.json`, in any letter case and in any combination. It has five subcommands; `opentab <command> --help` shows the full help.

```
opentab new --out song.opentab.json --title "My song"
opentab info song.opentab.json
opentab info song.opentab.json --ids
opentab info song.gp
opentab apply song.opentab.json edits.jsonl --out edited.opentab.json
opentab apply song.gp edits.jsonl --out edited.gp
opentab convert song.gp song.opentab.json
opentab convert song.opentab.json song.gp --json
opentab render song.gp --format svg --out song.svg
opentab render song.opentab.json --format png --scale 2 --out pages/
opentab render song.gp --format png --track 2 --bars 5-12 --staff tab --out riff.png
opentab render song.gp --format svg --page 2 --out page2.svg --json
```

- `new` creates a one-bar score with a six-string guitar track. It writes only `.opentab.json` and never overwrites an existing file.
- `info` prints the title, artist, tracks, time signatures and diagnostics. `--ids` also lists the id of every element; the JSON output always contains them in an `ids` section.
- `apply` reads one JSON command per line from the commands file (blank lines and lines starting with `//` are ignored, and line numbers count them), applies all of them and writes the result to `--out`. Nothing is written unless every command succeeds. `--out` may be the input file, and a symbolic link at `--out` is kept while its target is replaced. The output format follows the suffix of `--out`, so editing a `.gp` and writing `--out edited.gp` gives a `.gp`.
- `convert` reads the first path and writes the second, in the format of each suffix: `.gp` to `.opentab.json`, the reverse, or the same format on both sides. Writing is atomic and replaces an existing output. `convert --json` prints `{"converted": {"from": "song.gp", "to": "song.opentab.json"}}`.
- `render` lays the score out on A4 portrait pages and writes one SVG or PNG file per page. When `--out` is an existing directory the pages are named after the input (`song.svg`, or `song-p1.svg`, `song-p2.svg`… for several pages); otherwise `--out` must end in `.svg` or `.png` to match `--format`, its directory must already exist, and several pages become `<stem>-p<N>.<ext>` next to it. `--page N` (counted from 1) writes only that page, to `--out` itself or, in a directory, under the name it would have in a full render. `--scale` sets the PNG size, where 1 means 96 pixels per inch (an A4 page is 794×1123 pixels); it accepts `2`, `1.5` or `3/2`, and a PNG may hold at most 60 million pixels (a 240 MB image in memory, reached near scale 8.2 for an A4 page). `--track N` (repeatable, counted from 1 as in `info`) draws only those tracks, `--bars A-B` (or a single `A`, counted from 1) only those master bars, and `--staff standard|tab|both` forces the staves of every fretted track; by default a guitar, bass, sitar, banjo, shamisen or koto track (General MIDI programs 25–40 and 105–108, counted from 1) with 4 to 8 strings gets standard notation and tablature, and any other track standard notation only (keyboards are stored as six-string fretted tracks by Guitar Pro). Drum tracks get one five-line staff with a percussion clef, each General MIDI drum class of the drum kit on its own line, with x noteheads for cymbals, hi-hats, side stick, cowbell and tambourine. With no tempo in the file, "♩ = 120" is printed at the first bar. A capo is printed under the title block as "Capo fret N". The staff space is 1.75 mm; when a system is too tall for the page or a bar too wide for a line it shrinks, never below 0.9 mm, and the output says so. A bar still too wide at 0.9 mm is squeezed and reported as crowded. `render --json` prints `{"pages": 2, "systems": 9, "bars": 40, "staff_space": 1750, "crowded": [], "dropped": [], "title_page": false, "unrendered": ["lyrics"], "files": ["song-p1.svg", "song-p2.svg"]}`: `pages` counts every page of the layout (also with `--page`), `staff_space` is the staff space used, in micrometres, `crowded` lists the crowded bars, counted from 1, `dropped` lists the marks left out for lack of room (`{"bar": 3, "kind": "chord"}`, or `{"kind": "title"}` for a title block cut short), `title_page` says whether page 1 holds only the title block, and `unrendered` names, sorted, what the score holds but the drawn tracks, bars and staves do not show. The names are `accents`, `arpeggios`, `bends`, `chord_diagrams`, `copyright`, `directions`, `fade_in`, `fermatas`, `golpe`, `hairpins`, `hammer_on_pull_off`, `harmonic_types`, `instructions`, `legato_slurs`, `let_ring`, `let_ring_throughout`, `lyrics`, `missing_characters` (text drawn as empty boxes because the bundled font lacks the character), `multi_bar_rests`, `notices`, `ornaments`, `palm_mute`, `percussion_articulations`, `pop`, `right_hand_fingering`, `secondary_voice_rhythm` (rhythm under a lone tab follows one voice), `simile_marks`, `slap`, `slash_notation`, `slides`, `staccato`, `strokes`, `tapping`, `triplet_feel`, `vibrato`, `wah` and `whammy_bar`. Tab effects are listed when no tab is drawn for their track (for example `--staff standard`), and accents and staccato when only the tab is drawn. `crowded`, `dropped` and `unrendered` always describe the whole selected layout, every page of it, also with `--page`. The human output prints the same facts in short sentences.

Supported formats are `.gp` (Guitar Pro 7 and 8) and `.opentab.json`. Any other suffix is an `unsupported_format` error that lists them.

Add `--json` to any subcommand to get a single JSON line on standard output, for results and for errors. `apply --json` prints `{"applied": 2, "allocated": [12, 13]}`, where `allocated` lists the ids that did not exist before, in ascending order. Errors look like `{"error": {"kind": "command_failed", "sub_kind": "not_found", "message": "...", "line": 2}}`; `sub_kind` is present only for `command_failed` and is one of `not_found`, `index_out_of_range`, `invalid_value` and `wrong_instrument`. The error kinds are:

- `usage`: the command line is wrong (exit 1).
- `io`: a file could not be read or written, or the result could not be printed. `render` also uses it when the directory of `--out` does not exist.
- `unsupported_format`: a path has a suffix that is neither `.gp` nor `.opentab.json`, or a `render --out` file does not end in the suffix of `--format`.
- `invalid_score`: an `.opentab.json` file is not a valid score.
- `invalid_input`: a `.gp` file or a commands file could not be read; the message names the place in the file that is wrong. `render` also uses it when `--scale` is given for SVG output, when a page would be larger than 16384 pixels on a side or 60 million pixels in all, or when `--track`, `--bars` or `--page` names a track, bar or page the score does not have.
- `invalid_command`: a line of the commands file is not a valid command.
- `command_failed`: a command could not be applied.
- `output_exists`: `new` refused to overwrite a file.
- `unsupported`: the `.gp` writer refuses the score, for example because a text holds a character a `.gp` cannot store, or because the score has no master bar or no track; the message names the place in the score.
- `layout_limit`: `render` cannot lay the score out, because a system is too tall for the page even at the smallest staff space (0.9 mm) or a bar's rhythm is too complex to place exactly.
- `internal`: an unexpected failure inside OpenTab, for example when a score cannot be encoded.

Exit codes: 0 on success, 1 for a usage error and 2 for an input or output error. Human output prints control characters as `\u{XX}`; JSON output stays standard JSON.

## MCP server

`opentab-mcp` is a [Model Context Protocol](https://modelcontextprotocol.io) server for AI agents. It speaks JSON-RPC over standard input and output, one message per line, and is built with the official Rust SDK (`rmcp`). Build it with `cargo build --release -p opentab-mcp`; the binary is `target/release/opentab-mcp`.

To use it from an MCP client, register the binary as a stdio server. Most clients take a JSON configuration like this one (the file and the key differ between clients):

```json
{
  "mcpServers": {
    "opentab": {
      "command": "/path/to/opentab/target/release/opentab-mcp",
      "args": []
    }
  }
}
```

The tools work on sessions, so an agent can keep several scores open (at most 16 at once):

- `open {path}` opens a `.gp` or `.opentab.json` file (a regular file: directories, pipes and devices are refused) and returns `{"session", "title", "tracks", "bars"}`.
- `info {session}` returns what `opentab info --json` prints, with every element id, plus `can_undo` and `can_redo`.
- `render {session, page?, track?, bars?, staff?, scale?}` returns one page as a PNG image and a text part with the fields of `render --json` (without `files`) plus `page`, `width`, `height` and `scale`. It renders page 1 at scale 4/3 by default, 1058×1497 pixels for A4, which keeps the long side under 1568 pixels, a common size limit before vision models shrink an image; the 60 million pixel cap of `render` applies.
- `apply {session, commands}` applies a list of commands of the `opentab apply` protocol, all or nothing, and returns `{"applied", "allocated"}` as `apply --json` does.
- `undo {session}` and `redo {session}` take back or replay the last `apply`, all of its commands at once.
- `export {session, path}` writes the score to a path ending in `.gp` or `.opentab.json`, in that format, and refuses any other suffix. It writes as `convert` does: a temporary file in the same directory, then a rename, so an existing file (even the one the session was opened from) is replaced whole or not at all.
- `close {session}` forgets the session.

A tool that fails returns a tool result marked as an error whose text is `{"error": {"kind": ..., "message": ...}}`, with the error kinds of the command line (`index` instead of `line` for a command of `apply`); malformed tool arguments are `usage` errors. An unknown tool, or a known method with malformed params, is a JSON-RPC `-32602` error. A line that is not JSON gets `-32700`. JSON that is not a valid JSON-RPC 2.0 message gets `-32600`, and so do an id that is neither a string nor an integer (answered with a null id), a batch array (MCP 2025-06-18 has no batching) and a line longer than 64 MiB, which is skipped without being kept in memory. A request before `initialize` gets `-32002`. The server keeps serving after every error. Paths are used as given, with the permissions of the user who runs the server. Text inside a score is data, never instructions to follow.

## Mutation testing

`cargo xtask mutants` runs [cargo-mutants](https://mutants.rs) on `opentab-model`, `opentab-command`, `opentab-gp`, `opentab-layout`, `opentab-render`, `opentab-mcp`, `opentab-files`, `opentab-playback` and `opentab-audio` and checks that the tests catch every change it makes to the code. It runs one job at a time with a 120 second timeout, reads the shared settings from `.cargo/mutants.toml`, and writes its results under `target/mutants/mutants.out/`.

```
cargo install cargo-mutants --version 27.1.0 --locked
cargo xtask mutants
```

A full run takes several hours: with `opentab-gp` in the list it took about five hours on an 8 GB WSL machine. It uses most of that machine's memory, so do not start other builds or test runs while it works. It exits with 0 when no mutant was missed and none timed out, with 1 when any was, and with 2 when cargo-mutants is not installed or could not start. The missed and timed-out mutants are printed at the end.

To spread the work over several sittings, run one shard at a time. `--shard K/N` is passed to cargo-mutants, which splits the list of mutants of all eight crates into N parts and tests only part K, counting from 0. The four runs below together cover every mutant:

```
cargo xtask mutants --shard 0/4
cargo xtask mutants --shard 1/4
cargo xtask mutants --shard 2/4
cargo xtask mutants --shard 3/4
```

Each run replaces `target/mutants/mutants.out/` (cargo-mutants keeps the previous one as `mutants.out.old`), so read or copy a shard's results before you start the next one. The M1 milestone gate ran `opentab-model` and `opentab-command` in full but only shard 0/4 of the `opentab-gp` mutants; the other three `opentab-gp` shards have not run yet.

`--iterate` is passed to cargo-mutants as well: it retests only the mutants the previous run in `target/mutants/mutants.out/` missed or left unknown, which is the quick way to check fixes after a full run.

It is not part of continuous integration: the run is too slow and too heavy for a shared runner, and its result depends on timing and on the machine. Run it before closing a milestone and whenever the model, command or gp crates change.

## Memory safety for heavy runs

A mutated build can make a test allocate without bound, and on a small WSL machine that can freeze or take down the whole VM. `cargo xtask mutants` therefore runs cargo-mutants inside a systemd user scope limited to 5 GB of memory and 1 GB of swap, so the kernel kills a runaway process instead of the machine. The same cap is available for any heavy command:

```
cargo xtask capped -- cargo test --workspace
```

The scope sets `OOMPolicy=continue`, so when one process is OOM-killed only that process dies and the rest of the run, such as cargo-mutants, keeps going. The exit code is the command's. The cap needs Linux and a working `systemd-run --user`; when either is missing, both commands print a warning and run without a limit.

## Platforms

Linux and Windows are the primary targets. macOS is verified by continuous integration only; nobody on the team runs it day to day.

## Fonts

OpenTab draws music symbols and text as outlines from two bundled fonts, stored unmodified with their licences under `assets/fonts/` (see `assets/fonts/README.md` for versions, sources and checksums):

- [Bravura](https://github.com/steinbergmedia/bravura) by Steinberg Media Technologies GmbH, the SMuFL reference music font, with its metadata. Licensed under the SIL Open Font License, Version 1.1.
- [Source Sans 3](https://github.com/adobe-fonts/source-sans) by Paul D. Hunt for Adobe. Licensed under the SIL Open Font License, Version 1.1.

## Licence

Licensed under either of the MIT licence (`LICENSE-MIT`) or the Apache License, Version 2.0 (`LICENSE-APACHE`), at your option.

## Trademark

OpenTab is an independent project and is not affiliated with, endorsed by or sponsored by Arobas Music. Guitar Pro is a trademark of Arobas Music. No Guitar Pro code, assets, sounds or trademarks are used in this project.
