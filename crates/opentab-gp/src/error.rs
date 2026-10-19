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
    #[error("opaque data and text copied per reference would exceed {limit} bytes")]
    RetainedLimit { limit: u64 },
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

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Refusal {
    #[error("text contains U+{code:04X}, which a GPIF text value cannot hold")]
    Character { code: u32 },
    #[error("the container has {count} score entries; a non-empty container needs exactly one")]
    ScoreEntries { count: usize },
    #[error("the score entry is named \"{name}\"; it must be Content/score.gpif")]
    ScoreEntryName { name: String },
    #[error("entry \"{name}\" appears more than once")]
    DuplicateEntry { name: String },
    #[error("entry name \"{name}\" does not survive the zip format unchanged")]
    EntryName { name: String },
    #[error("directory entry \"{name}\" carries data")]
    DirectoryData { name: String },
    #[error("opaque payload format \"{format}\" is not gpif")]
    PayloadFormat { format: String },
    #[error("opaque payload is malformed: {problem}")]
    MalformedPayload { problem: String },
    #[error("opaque payloads belong to element {id}, which the score does not contain")]
    DanglingExtension { id: u64 },
    #[error("the score has no master bars; Guitar Pro needs at least one")]
    NoMasterBars,
    #[error("the score has no tracks; Guitar Pro needs at least one")]
    NoTracks,
    #[error("the staff has {found} bars, but the score has {expected} master bars")]
    BarCount { expected: usize, found: usize },
    #[error("voice slots are not strictly increasing")]
    VoiceSlots,
    #[error("the note kind does not match the instrument kind of its track")]
    NoteKind,
    #[error("string {string} does not exist on a {strings}-string tuning")]
    String { string: u8, strings: usize },
    #[error("the sounding pitch is outside MIDI 0..=127")]
    Pitch,
    #[error(
        "the trill's auxiliary fret {fret} sounds outside MIDI 0..=127, or the note has no string"
    )]
    TrillPitch { fret: u8 },
    #[error("tempo position {numer}/{denom} is not on the 1/3840 grid within its bar")]
    TempoPosition { numer: i64, denom: i64 },
    #[error("harmonic fret {numer}/{denom} is not a non-negative decimal of at most 30 places")]
    HarmonicFret { numer: i64, denom: i64 },
    #[error("the right hand has no open-string fingering in GPIF")]
    RightHandOpen,
    #[error("a drum kit belongs only to a percussion track")]
    DrumKitOnFretted,
    #[error(
        "articulation {articulation} is outside the track's drum kit of {articulations} articulations"
    )]
    ArticulationOutsideKit {
        articulation: u16,
        articulations: usize,
    },
    #[error("the instrument set belongs to another instrument kind than the track's")]
    InstrumentSet,
    #[error("MIDI program {program} is outside 0..=127")]
    Program { program: u8 },
    #[error("track {track} has more than one sound override")]
    DuplicateSoundOverride { track: u64 },
    #[error("the first sound's program or bank differs from the track's initial program or bank")]
    FirstSound,
    #[error("an automation names master bar {master_bar}, which the score does not contain")]
    AutomationBar { master_bar: u64 },
    #[error(
        "automation position {numer}/{denom} is not on the 1/3840 grid within its bar, end included"
    )]
    AutomationPosition { numer: i64, denom: i64 },
    #[error("a sound automation names sound {index}, which the track does not have")]
    SoundRef { index: usize },
    #[error(
        "a sound automation names sound {index}, whose path, name and role another sound of the track shares"
    )]
    AmbiguousSound { index: usize },
    #[error("a sound override names track {track}, which the score does not contain")]
    SoundOverrideTrack { track: u64 },
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum GpError {
    #[error("archive: {reason}")]
    Archive { reason: String },
    #[error("duplicate archive entry \"{name}\"")]
    DuplicateEntry { name: String },
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
    #[error("{path}: {reason}")]
    Unsupported { path: String, reason: Refusal },
    #[error("writing the archive failed: {reason}")]
    Write { reason: String },
}
