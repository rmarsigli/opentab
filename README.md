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

## Platforms

Linux and Windows are the primary targets. macOS is verified by continuous integration only; nobody on the team runs it day to day.

## Licence

Licensed under either of the MIT licence (`LICENSE-MIT`) or the Apache License, Version 2.0 (`LICENSE-APACHE`), at your option.

## Trademark

OpenTab is an independent project and is not affiliated with, endorsed by or sponsored by Arobas Music. Guitar Pro is a trademark of Arobas Music. No Guitar Pro code, assets, sounds or trademarks are used in this project.
