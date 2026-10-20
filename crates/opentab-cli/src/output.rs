use std::io::Write;
use std::process::ExitCode;

use clap::error::ErrorKind;
use serde::Serialize;

use crate::error::{CliError, EXIT_USAGE, ErrorBody, ErrorEnvelope};
use crate::text::{escape_controls, escape_controls_keeping_lines};

pub struct Rendered<T> {
    pub wire: T,
    pub human: String,
}

pub fn emit<T: Serialize>(rendered: &Rendered<T>, json: bool) -> Result<(), CliError> {
    let text = if json {
        serde_json::to_string(&rendered.wire).map_err(|error| CliError::Internal {
            message: error.to_string(),
        })?
    } else {
        rendered.human.clone()
    };
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "{text}")
        .and_then(|()| stdout.flush())
        .map_err(|error| CliError::Output {
            message: error.to_string(),
        })
}

pub fn failure(error: &CliError, json: bool) -> ExitCode {
    print_error(error.body(), json, escape_controls);
    ExitCode::from(error.exit_code())
}

pub fn usage(error: &clap::Error, json_hint: bool) -> ExitCode {
    if matches!(
        error.kind(),
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
    ) {
        let _ = write!(std::io::stdout(), "{error}");
        return ExitCode::SUCCESS;
    }
    let body = ErrorBody {
        kind: "usage",
        sub_kind: None,
        message: error.to_string().trim_end().to_string(),
        line: None,
    };
    print_error(body, json_hint, escape_controls_keeping_lines);
    ExitCode::from(EXIT_USAGE)
}

fn print_error(body: ErrorBody, json: bool, escape: fn(&str) -> String) {
    if json {
        let envelope = ErrorEnvelope { error: body };
        let text = serde_json::to_string(&envelope).unwrap_or_default();
        if writeln!(std::io::stdout(), "{text}").is_err() {
            let _ = writeln!(
                std::io::stderr(),
                "error: {}",
                escape(&envelope.error.message)
            );
        }
    } else {
        let _ = writeln!(std::io::stderr(), "error: {}", escape(&body.message));
    }
}
