use std::io::Write;
use std::process::ExitCode;

use clap::error::ErrorKind;
use serde::Serialize;

use crate::error::{CliError, EXIT_INPUT, EXIT_USAGE, ErrorBody, ErrorEnvelope};

pub trait Report: Serialize {
    fn human(&self) -> String;
}

pub fn emit<T: Report>(report: &T, json: bool) {
    let text = if json {
        serde_json::to_string(report).unwrap_or_default()
    } else {
        report.human()
    };
    let _ = writeln!(std::io::stdout(), "{text}");
}

pub fn failure(error: &CliError, json: bool) -> ExitCode {
    print_error(error.body(), json);
    ExitCode::from(EXIT_INPUT)
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
        message: error.to_string().trim_end().to_string(),
        line: None,
    };
    print_error(body, json_hint);
    ExitCode::from(EXIT_USAGE)
}

fn print_error(body: ErrorBody, json: bool) {
    if json {
        let envelope = ErrorEnvelope { error: body };
        let text = serde_json::to_string(&envelope).unwrap_or_default();
        let _ = writeln!(std::io::stdout(), "{text}");
    } else {
        let _ = writeln!(std::io::stderr(), "error: {}", body.message);
    }
}
