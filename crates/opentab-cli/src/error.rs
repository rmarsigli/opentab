use std::path::{Path, PathBuf};

use opentab_command::CommandError;
use opentab_model::ModelError;
use serde::Serialize;

pub const EXIT_USAGE: u8 = 1;
pub const EXIT_INPUT: u8 = 2;

#[derive(Debug)]
pub enum CliError {
    Io { path: PathBuf, message: String },
    UnsupportedFormat { path: PathBuf },
    InvalidScore { path: PathBuf, message: String },
    OutputExists { path: PathBuf },
    InvalidCommand { line: usize, message: String },
    CommandFailed { line: usize, source: CommandError },
    Output { message: String },
    Internal { message: String },
}

#[derive(Serialize)]
pub struct ErrorBody {
    pub kind: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

#[derive(Serialize)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

impl CliError {
    pub fn io(path: &Path, source: &std::io::Error) -> CliError {
        CliError::Io {
            path: path.to_path_buf(),
            message: source.to_string(),
        }
    }

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
                line: None,
            },
            CliError::UnsupportedFormat { path } => ErrorBody {
                kind: "unsupported_format",
                message: format!(
                    "{}: unsupported file type, supported formats: {}",
                    path.display(),
                    crate::files::SUFFIX
                ),
                line: None,
            },
            CliError::InvalidScore { path, message } => ErrorBody {
                kind: "invalid_score",
                message: format!("{}: {message}", path.display()),
                line: None,
            },
            CliError::OutputExists { path } => ErrorBody {
                kind: "output_exists",
                message: format!("{} already exists, refusing to overwrite", path.display()),
                line: None,
            },
            CliError::InvalidCommand { line, message } => ErrorBody {
                kind: "invalid_command",
                message: format!("line {line}: {message}"),
                line: Some(*line),
            },
            CliError::CommandFailed { line, source } => ErrorBody {
                kind: "command_failed",
                message: format!("line {line}: {source}"),
                line: Some(*line),
            },
            CliError::Output { message } => ErrorBody {
                kind: "io",
                message: format!("cannot write the result to stdout: {message}"),
                line: None,
            },
            CliError::Internal { message } => ErrorBody {
                kind: "internal",
                message: message.clone(),
                line: None,
            },
        }
    }
}
