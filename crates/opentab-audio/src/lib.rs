#[deny(clippy::float_arithmetic)]
mod builtin;

#[deny(clippy::float_arithmetic)]
mod bus;

mod bank;
mod chase;
mod command;
mod error;
mod gm;
#[cfg(feature = "live")]
mod live;
mod manager;
mod output;
mod rate;
mod render;
mod schedule;
mod session;
mod sf2;
mod sfz;
mod status;
mod synth;
mod timeline;
mod transport;
mod wav;

pub use bank::{BuiltInBank, BuiltInSynth};
pub use builtin::{DRUM_DECAY_MS, MELODIC_RELEASE_MS, OUTPUT_FRACTION_BITS, SoundingVoice};
pub use bus::{BUS_FRACTION_BITS, Limiter, LimiterSettings};
pub use command::{BarRange, COMMAND_CAPACITY, QueueFull, TransportCommand};
pub use error::{AudioError, BankError, SF3_UNSUPPORTED, UnsafeBank};
#[cfg(feature = "live")]
pub use live::CpalOutput;
pub use manager::{
    ARCHIVE_MAX_RATIO, ARCHIVE_RATIO_FLOOR_BYTES, ArchiveKind, BANK_ID_MAX_BYTES, BANK_MAX_BYTES,
    BANK_MAX_DEPTH, BANK_MAX_ENTRIES, BANK_MAX_FILE_BYTES, BANK_MAX_NAME_BYTES, BANKS_FOLDER,
    BankId, BankManager, BankRef, BankSource, CatalogueEntry, DAMAGED_SUFFIX, DATA_DIR_NAME,
    DATA_DIR_VARIABLE, DOWNLOAD_CONNECT_SECONDS, DOWNLOAD_DEADLINE_FLOOR_SECONDS,
    DOWNLOAD_MIN_BYTES_PER_SECOND, DOWNLOAD_REDIRECTS, DOWNLOAD_RESPONSE_SECONDS,
    DOWNLOAD_STALL_SECONDS, DownloadTimeouts, Fallback, FallbackReason, ImportLimits,
    InstallCancel, InstallError, InstallProgress, InstallStage, InstalledBank, LOCK_NAME,
    LoadedVoices, PREFERENCES_NAME, REGISTRY_NAME, STAGING_FOLDER, STORE_MAX_BYTES, STORE_VERSION,
    SidecarProgram, SidecarRecord, SoundPlan, StoreProblem, TrackSound, check_download_url,
    default_catalogue, default_data_dir, sidecar_for,
};
pub use output::{AudioOutput, NULL_TICK_MILLIS, NullBlock, NullClock, NullOutput, OutputError};
pub use rate::SampleRate;
pub use render::{
    BLOCK_FRAMES, MAX_FRAMES, RENDER_TAIL_MS, RenderedAudio, TRACK_HEADROOM, full_length,
};
pub use schedule::frame_at;
pub use session::{
    BUILT_IN_BANK_NAME, RenderReport, ScriptStep, Session, TrackBackend, render_session,
};
pub use sf2::{
    BankShape, FAULT_LOG_LIMIT, LOADER_FILE_FACTOR, LOADER_SLACK_BYTES, MASTER_VOLUME,
    MAX_FILE_BYTES, MAX_GRID_PRESETS, MAX_LAYERS_PER_NOTE, MAX_PAIRS_PER_PRESET, MAX_REGION_PAIRS,
    MAX_VOICES_PER_NOTE, Sf2Bank, Sf2Limits, Sf2Synth, SynthFault, SynthStage,
};
pub use sfz::{
    AudioKind, CcRange, DecodedSample, Envelope, IgnoreReason, IgnoredOpcode, InitialController,
    LoopMode, OffMode, SFZ_MASTER_GAIN, SFZ_MAX_AUDIO_FILE_BYTES, SFZ_MAX_BANK_INCLUDES,
    SFZ_MAX_BANK_TEXT_BYTES, SFZ_MAX_DECODED_BYTES, SFZ_MAX_DEFINE_BYTES,
    SFZ_MAX_DEFINE_NAME_BYTES, SFZ_MAX_DEFINES, SFZ_MAX_EXPANDED_BYTES, SFZ_MAX_FILE_BYTES,
    SFZ_MAX_FOLDER_DEPTH, SFZ_MAX_FOLDER_ENTRIES, SFZ_MAX_IGNORED_NAMES, SFZ_MAX_INCLUDE_DEPTH,
    SFZ_MAX_INCLUDES, SFZ_MAX_LINE_BYTES, SFZ_MAX_LISTED_ENTRIES, SFZ_MAX_LOADER_BYTES,
    SFZ_MAX_PATH_BYTES, SFZ_MAX_PATH_LOOKUPS, SFZ_MAX_READ_BYTES, SFZ_MAX_REGIONS,
    SFZ_MAX_SAMPLE_CHANNELS, SFZ_MAX_SAMPLE_FRAMES, SFZ_MAX_SAMPLE_RATE, SFZ_MAX_SFZ_FILES,
    SFZ_MAX_SIDECAR_BYTES, SFZ_MAX_VOICES, SFZ_MAX_VOICES_PER_NOTE, SFZ_MAX_VORBIS_COMMENT_BYTES,
    SFZ_MAX_VORBIS_DIMENSIONS, SFZ_MAX_VORBIS_ENTRIES, SFZ_MAX_VORBIS_SETUP_BYTES,
    SFZ_MIN_SAMPLE_RATE, SIDECAR_NAME, SIDECAR_VERSION, SfzBank, SfzLimits, SfzRegion, SfzScope,
    SfzSynth, SfzText, SfzTextRun, SkippedInstrument, Trigger, VelocityPoint, VorbisSetupCost,
    audio_kind, decode_audio, measure_sfz_text, parse_sfz_text, vorbis_setup_cost,
};
pub use status::{TransportState, TransportStatus};
pub use synth::{Bank, BankFormat, BankInstrument, SoundUse, Synth};
pub use transport::{
    CLICK_BEAT_KEY, CLICK_BEAT_VELOCITY, CLICK_DOWNBEAT_KEY, CLICK_DOWNBEAT_VELOCITY, CURSOR_STEP,
    SessionConfig, Transport, TransportHandle, played_range, range_frames,
};
pub use wav::{WAV_HEADER_BYTES, encode_wav, pcm16};
