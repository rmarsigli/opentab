use opentab_model::{ModelError, TrackId};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CommandError {
    #[error("{what} {id} not found")]
    NotFound { what: &'static str, id: u64 },
    #[error("{what} index {index} is out of range for length {len}")]
    IndexOutOfRange {
        what: &'static str,
        index: usize,
        len: usize,
    },
    #[error("{message}")]
    InvalidValue { message: String },
    #[error("track {} is not a fretted instrument", .track.0.0)]
    WrongInstrument { track: TrackId },
}

impl CommandError {
    pub(crate) fn invalid(message: impl Into<String>) -> CommandError {
        CommandError::InvalidValue {
            message: message.into(),
        }
    }

    pub(crate) fn model(error: ModelError) -> CommandError {
        CommandError::invalid(error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Error)]
#[error("line {line}: {message}")]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}
