mod args;
mod engine;
mod error;
mod render;
mod server;
mod session;
mod tools;
mod wire;

use std::process::ExitCode;

use rmcp::ServiceExt;
use tokio::io::{duplex, stdin, stdout};
use tokio::sync::mpsc::unbounded_channel;

use crate::server::OpenTabServer;
use crate::wire::{pump_in, pump_out};

const PIPE_BYTES: usize = 1 << 16;

fn main() -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => return fail(&error.to_string()),
    };
    let outcome = runtime.block_on(serve());
    runtime.shutdown_background();
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => fail(&message),
    }
}

async fn serve() -> Result<(), String> {
    let (to_server, server_input) = duplex(PIPE_BYTES);
    let (server_output, from_server) = duplex(PIPE_BYTES);
    let (errors, error_lines) = unbounded_channel();
    tokio::spawn(pump_in(stdin(), to_server, errors));
    let writer = tokio::spawn(pump_out(from_server, stdout(), error_lines));
    let service = OpenTabServer::default()
        .serve((server_input, server_output))
        .await
        .map_err(|error| error.to_string())?;
    let waited = service.waiting().await;
    let _ = writer.await;
    waited.map(|_| ()).map_err(|error| error.to_string())
}

fn fail(message: &str) -> ExitCode {
    eprintln!("opentab-mcp: {message}");
    ExitCode::FAILURE
}
