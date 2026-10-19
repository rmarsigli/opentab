use std::path::PathBuf;

use opentab_command::CommandError;
use opentab_files::FileError;
use opentab_model::ModelError;
use serde::Serialize;

pub const EXIT_USAGE: u8 = 1;
pub const EXIT_INPUT: u8 = 2;

#[derive(Debug)]
pub enum CliError {
    Io {
        path: PathBuf,
        message: String,
    },
    UnsupportedFormat {
        path: PathBuf,
        supported: &'static str,
    },
    File(FileError),
    InvalidInput {
        message: String,
    },
    LayoutLimit {
        message: String,
    },
    InvalidCommand {
        line: usize,
        message: String,
    },
    CommandFailed {
        line: usize,
        source: CommandError,
    },
    Output {
        message: String,
    },
    Internal {
        message: String,
    },
}

impl From<FileError> for CliError {
    fn from(error: FileError) -> CliError {
        CliError::File(error)
    }
}

#[derive(Serialize)]
pub struct ErrorBody {
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_kind: Option<&'static str>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

#[derive(Serialize)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

impl CliError {
    pub fn internal(error: ModelError) -> CliError {
        CliError::Internal {
            message: error.to_string(),
        }
    }

    pub fn body(&self) -> ErrorBody {
        match self {
            CliError::Io { path, message } => ErrorBody {
                kind: "io",
                message: format!("{}: {message}", path.display()),
                sub_kind: None,
                line: None,
            },
            CliError::UnsupportedFormat { path, supported } => ErrorBody {
                kind: "unsupported_format",
                message: format!(
                    "{}: unsupported file type, supported formats: {supported}",
                    path.display()
                ),
                sub_kind: None,
                line: None,
            },
            CliError::File(error) => ErrorBody {
                kind: error.kind(),
                message: error.to_string(),
                sub_kind: None,
                line: None,
            },
            CliError::InvalidInput { message } => ErrorBody {
                kind: "invalid_input",
                sub_kind: None,
                message: message.clone(),
                line: None,
            },
            CliError::LayoutLimit { message } => ErrorBody {
                kind: "layout_limit",
                sub_kind: None,
                message: message.clone(),
                line: None,
            },
            CliError::InvalidCommand { line, message } => ErrorBody {
                kind: "invalid_command",
                message: format!("line {line}: {message}"),
                sub_kind: None,
                line: Some(*line),
            },
            CliError::CommandFailed { line, source } => ErrorBody {
                kind: "command_failed",
                sub_kind: Some(sub_kind(source)),
                message: format!("line {line}: {source}"),
                line: Some(*line),
            },
            CliError::Output { message } => ErrorBody {
                kind: "io",
                message: format!("cannot write the result to stdout: {message}"),
                sub_kind: None,
                line: None,
            },
            CliError::Internal { message } => ErrorBody {
                kind: "internal",
                message: message.clone(),
                sub_kind: None,
                line: None,
            },
        }
    }
}

fn sub_kind(error: &CommandError) -> &'static str {
    match error {
        CommandError::NotFound { .. } => "not_found",
        CommandError::IndexOutOfRange { .. } => "index_out_of_range",
        CommandError::InvalidValue { .. } => "invalid_value",
        CommandError::WrongInstrument { .. } => "wrong_instrument",
    }
}
