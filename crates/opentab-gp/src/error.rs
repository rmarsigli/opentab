use std::fmt;

use opentab_model::ModelError;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolKind {
    Track,
    Bar,
    Voice,
    Beat,
    Note,
    Rhythm,
}

impl fmt::Display for PoolKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            PoolKind::Track => "track",
            PoolKind::Bar => "bar",
            PoolKind::Voice => "voice",
            PoolKind::Beat => "beat",
            PoolKind::Note => "note",
            PoolKind::Rhythm => "rhythm",
        };
        formatter.write_str(name)
    }
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum LimitProblem {
    #[error("entry is larger than {limit} bytes uncompressed")]
    EntrySize { limit: u64 },
    #[error("archive is larger than {limit} bytes uncompressed")]
    TotalSize { limit: u64 },
    #[error("entry expands more than {limit} times its compressed size")]
    CompressionRatio { limit: u64 },
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum GpifProblem {
    #[error("malformed XML: {message}")]
    Syntax { message: String },
    #[error("score.gpif is not valid UTF-8")]
    NotUtf8,
    #[error("root element is <{found}>, expected <GPIF>")]
    UnexpectedRoot { found: String },
    #[error("XML nesting deeper than {limit} levels")]
    DepthLimit { limit: usize },
    #[error("more than {limit} XML elements")]
    ElementLimit { limit: usize },
    #[error("references expand to more than {limit} model elements")]
    ExpansionLimit { limit: u64 },
    #[error("opaque data would exceed {limit} bytes")]
    OpaqueLimit { limit: u64 },
    #[error("missing element <{name}>")]
    MissingElement { name: String },
    #[error("missing attribute \"{name}\"")]
    MissingAttribute { name: String },
    #[error("element <{name}> appears more than once")]
    DuplicateElement { name: String },
    #[error("\"{text}\" is not a valid number")]
    InvalidNumber { text: String },
    #[error("\"{text}\" is not a valid value")]
    InvalidValue { text: String },
    #[error("{kind} id \"{found}\" is out of order, expected {expected}")]
    IdOutOfOrder {
        kind: PoolKind,
        expected: usize,
        found: String,
    },
    #[error("reference to missing {kind} id {id}")]
    MissingReference { kind: PoolKind, id: String },
    #[error("track id {id} is listed more than once")]
    DuplicateTrackReference { id: usize },
    #[error("lists {found} bars, but the score has {expected} staves")]
    StaffCountMismatch { expected: usize, found: usize },
    #[error("string {string} does not exist on a {strings}-string tuning")]
    StringOutOfRange { string: u32, strings: usize },
    #[error("tempo automation names bar {bar}, but the score has {master_bars} master bars")]
    TempoBarOutOfRange { bar: usize, master_bars: usize },
    #[error("the model ran out of element ids")]
    IdsExhausted,
    #[error(transparent)]
    Model(#[from] ModelError),
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum GpError {
    #[error("archive: {reason}")]
    Archive { reason: String },
    #[error("{entry}: {reason}")]
    Entry { entry: String, reason: String },
    #[error("{entry}: {problem}")]
    Limit {
        entry: String,
        problem: LimitProblem,
    },
    #[error("archive has no Content/score.gpif entry")]
    MissingScore,
    #[error("{entry}: {path}: {problem}")]
    Gpif {
        entry: String,
        path: String,
        problem: GpifProblem,
    },
}
