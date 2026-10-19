use opentab_playback::PlaybackError;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum AudioError {
    #[error("playback time: {0}")]
    Playback(#[from] PlaybackError),
    #[error("sample rate {hz} Hz is not supported (44100 or 48000)")]
    UnsupportedRate { hz: u32 },
    #[error("{voices} synths were given for {tracks} tracks")]
    VoiceCount { voices: usize, tracks: usize },
    #[error("{frames} frames exceed the longest render a WAV file holds")]
    TooLong { frames: u64 },
    #[error("an event falls at a frame beyond 64 bits")]
    FrameOverflow,
}

pub const SF3_UNSUPPORTED: &str = "This is an SF3 SoundFont, which keeps its sounds compressed. OpenTab plays SF2 SoundFonts only. Convert the file to SF2 (the free Polyphone editor can do it) and import the SF2 file instead.";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BankError {
    #[error("{message}")]
    Unsupported { message: String },
    #[error("this file is not a usable sound bank: {reason}")]
    Invalid { reason: String },
    #[error("the sound engine failed while reading this sound bank: {message}")]
    Panicked { message: String },
    #[error(
        "this sound bank was not loaded because {reason}. The file may be damaged or built to crash programs."
    )]
    Unsafe { reason: UnsafeBank },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Error)]
pub enum UnsafeBank {
    #[error("the file is larger than {limit_mib} MiB, the largest SoundFont OpenTab accepts")]
    FileTooLarge { limit_mib: u64 },
    #[error("a required section is missing or out of place")]
    MissingChunk,
    #[error("a section appears twice")]
    DuplicateChunk,
    #[error("it holds a section OpenTab does not know")]
    UnknownChunk,
    #[error("a table has a broken size")]
    MalformedTable,
    #[error("a section claims more bytes than the file holds")]
    ChunkOverflow,
    #[error("a section has no type")]
    ListType,
    #[error("a version field has the wrong size")]
    VersionSize,
    #[error("the sample data has an odd or tiny size")]
    OddSamples,
    #[error("preset or instrument zone numbers run backwards or past the end")]
    ZoneOrder,
    #[error("zone generator numbers run backwards or past the end")]
    GeneratorOrder,
    #[error("zone modulator numbers run backwards or past the end")]
    ModulatorOrder,
    #[error("reading it would need more memory than OpenTab allows")]
    LoaderBudget,
    #[error("a preset layers too many regions to play in real time")]
    RegionPairs,
    #[error("one key would start too many voices at once")]
    Layers,
    #[error("too many presets stack many layers")]
    LayeredPresets,
    #[error("an instrument text file is larger than {limit_kib} KiB")]
    SfzFileTooLarge { limit_kib: u64 },
    #[error(
        "an instrument grows past {limit_mib} MiB of text once its includes and variables are expanded"
    )]
    ExpandedTextTooLarge { limit_mib: u64 },
    #[error("the bank holds more than {limit_mib} MiB of instrument text")]
    BankTextTooLarge { limit_mib: u64 },
    #[error("an instrument has more than {limit} regions")]
    TooManyRegions { limit: u64 },
    #[error("includes are nested more than {limit} levels deep")]
    IncludeTooDeep { limit: u64 },
    #[error("an instrument includes more than {limit} files")]
    TooManyIncludes { limit: u64 },
    #[error("files include each other in a loop")]
    IncludeCycle,
    #[error("an instrument defines more than {limit} variables")]
    TooManyDefines { limit: u64 },
    #[error("variable definitions hold more than {limit_kib} KiB")]
    DefineTextTooLarge { limit_kib: u64 },
    #[error("the bank folder holds more than {limit} entries")]
    TooManyFiles { limit: u64 },
    #[error("the bank folders are nested more than {limit} levels deep")]
    FolderTooDeep { limit: u64 },
    #[error("the bank has more than {limit} instrument files")]
    TooManyInstruments { limit: u64 },
    #[error("its program map file is larger than {limit_kib} KiB")]
    SidecarTooLarge { limit_kib: u64 },
    #[error("a sample file is larger than {limit_mib} MiB")]
    AudioFileTooLarge { limit_mib: u64 },
    #[error("a sample is longer than {limit} frames")]
    SampleTooLong { limit: u64 },
    #[error("a sample has {channels} channels, and OpenTab plays mono and stereo samples only")]
    SampleChannels { channels: u32 },
    #[error("a sample runs at {hz} Hz, outside the rates OpenTab plays")]
    SampleRate { hz: u32 },
    #[error("its decoded samples would need more than {limit_mib} MiB")]
    DecodedAudioBudget { limit_mib: u64 },
    #[error("an Ogg Vorbis sample declares decoding tables larger than OpenTab allows")]
    VorbisTables,
    #[error("an Ogg Vorbis sample declares more than {limit} codebook entries")]
    VorbisEntries { limit: u64 },
    #[error("the bank folder holds a symbolic link, which OpenTab does not follow in sound banks")]
    BankSymlink,
    #[error("looking up file names would list more than {limit} folder entries")]
    ListedEntries { limit: u64 },
    #[error("an instrument reads more than {limit_mib} MiB of text through its includes")]
    TextReadTooLarge { limit_mib: u64 },
    #[error("a line of instrument text is longer than {limit_kib} KiB")]
    LineTooLong { limit_kib: u64 },
    #[error("an Ogg Vorbis sample declares text longer than the file")]
    VorbisComments,
}
