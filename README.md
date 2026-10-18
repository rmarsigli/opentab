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

The `opentab` binary works on `.opentab.json` scores and on Guitar Pro `.gp` files. The format of every path is picked by its suffix: `.gp` or `.opentab.json`, in any letter case and in any combination. It has four subcommands; `opentab <command> --help` shows the full help.

```
opentab new --out song.opentab.json --title "My song"
opentab info song.opentab.json
opentab info song.opentab.json --ids
opentab info song.gp
opentab apply song.opentab.json edits.jsonl --out edited.opentab.json
opentab apply song.gp edits.jsonl --out edited.gp
opentab convert song.gp song.opentab.json
opentab convert song.opentab.json song.gp --json
```

- `new` creates a one-bar score with a six-string guitar track. It writes only `.opentab.json` and never overwrites an existing file.
- `info` prints the title, artist, tracks, time signatures and diagnostics. `--ids` also lists the id of every element; the JSON output always contains them in an `ids` section.
- `apply` reads one JSON command per line from the commands file (blank lines and lines starting with `//` are ignored, and line numbers count them), applies all of them and writes the result to `--out`. Nothing is written unless every command succeeds. `--out` may be the input file, and a symbolic link at `--out` is kept while its target is replaced. The output format follows the suffix of `--out`, so editing a `.gp` and writing `--out edited.gp` gives a `.gp`.
- `convert` reads the first path and writes the second, in the format of each suffix: `.gp` to `.opentab.json`, the reverse, or the same format on both sides. Writing is atomic and replaces an existing output. `convert --json` prints `{"converted": {"from": "song.gp", "to": "song.opentab.json"}}`.

Supported formats are `.gp` (Guitar Pro 7 and 8) and `.opentab.json`. Any other suffix is an `unsupported_format` error that lists them.

Add `--json` to any subcommand to get a single JSON line on standard output, for results and for errors. `apply --json` prints `{"applied": 2, "allocated": [12, 13]}`, where `allocated` lists the ids that did not exist before, in ascending order. Errors look like `{"error": {"kind": "command_failed", "sub_kind": "not_found", "message": "...", "line": 2}}`; `sub_kind` is present only for `command_failed` and is one of `not_found`, `index_out_of_range`, `invalid_value` and `wrong_instrument`. The error kinds are:

- `usage`: the command line is wrong (exit 1).
- `io`: a file could not be read or written, or the result could not be printed.
- `unsupported_format`: a path has a suffix that is neither `.gp` nor `.opentab.json`.
- `invalid_score`: an `.opentab.json` file is not a valid score.
- `invalid_input`: a `.gp` file or a commands file could not be read; the message names the place in the file that is wrong.
- `invalid_command`: a line of the commands file is not a valid command.
- `command_failed`: a command could not be applied.
- `output_exists`: `new` refused to overwrite a file.
- `unsupported`: the `.gp` writer refuses the score, for example because a text holds a character a `.gp` cannot store, or because the score has no master bar or no track; the message names the place in the score.
- `internal`: an unexpected failure inside OpenTab, for example when a score cannot be encoded.

Exit codes: 0 on success, 1 for a usage error and 2 for an input or output error. Human output prints control characters as `\u{XX}`; JSON output stays standard JSON.

## Mutation testing

`cargo xtask mutants` runs [cargo-mutants](https://mutants.rs) on `opentab-model`, `opentab-command` and `opentab-gp` and checks that the tests catch every change it makes to the code. It runs one job at a time with a 120 second timeout, reads the shared settings from `.cargo/mutants.toml`, and writes its results under `target/mutants/mutants.out/`.

```
cargo install cargo-mutants --version 27.1.0 --locked
cargo xtask mutants
```

A full run takes several hours: with `opentab-gp` in the list it took about five hours on an 8 GB WSL machine. It uses most of that machine's memory, so do not start other builds or test runs while it works. It exits with 0 when no mutant was missed and none timed out, with 1 when any was, and with 2 when cargo-mutants is not installed or could not start. The missed and timed-out mutants are printed at the end.

To spread the work over several sittings, run one shard at a time. `--shard K/N` is passed to cargo-mutants, which splits the list of mutants of all three crates into N parts and tests only part K, counting from 0. The four runs below together cover every mutant:

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

## Licence

Licensed under either of the MIT licence (`LICENSE-MIT`) or the Apache License, Version 2.0 (`LICENSE-APACHE`), at your option.

## Trademark

OpenTab is an independent project and is not affiliated with, endorsed by or sponsored by Arobas Music. Guitar Pro is a trademark of Arobas Music. No Guitar Pro code, assets, sounds or trademarks are used in this project.
