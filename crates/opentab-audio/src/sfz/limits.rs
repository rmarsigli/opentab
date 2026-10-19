pub const SFZ_MAX_FILE_BYTES: u64 = 4 << 20;
pub const SFZ_MAX_EXPANDED_BYTES: u64 = 16 << 20;
pub const SFZ_MAX_BANK_TEXT_BYTES: u64 = 512 << 20;
pub const SFZ_MAX_REGIONS: u64 = 100_000;
pub const SFZ_MAX_INCLUDE_DEPTH: u64 = 8;
pub const SFZ_MAX_INCLUDES: u64 = 4096;
pub const SFZ_MAX_DEFINES: u64 = 16_384;
pub const SFZ_MAX_DEFINE_BYTES: u64 = 1 << 20;
pub const SFZ_MAX_DEFINE_NAME_BYTES: usize = 64;
pub const SFZ_MAX_LINE_BYTES: u64 = 64 << 10;
pub const SFZ_MAX_READ_BYTES: u64 = 32 << 20;
pub const SFZ_MAX_LISTED_ENTRIES: u64 = 1 << 20;
pub const SFZ_MAX_PATH_BYTES: usize = 1024;
pub const SFZ_MAX_PATH_LOOKUPS: u64 = 1 << 16;
pub const SFZ_MAX_VORBIS_COMMENT_BYTES: u64 = 1 << 20;
pub const SFZ_MAX_FOLDER_ENTRIES: u64 = 200_000;
pub const SFZ_MAX_FOLDER_DEPTH: u64 = 16;
pub const SFZ_MAX_SFZ_FILES: u64 = 8192;
pub const SFZ_MAX_LOADER_BYTES: u64 = 256 << 20;
pub const SFZ_MAX_SIDECAR_BYTES: u64 = 1 << 20;
pub const SFZ_MAX_AUDIO_FILE_BYTES: u64 = 256 << 20;
pub const SFZ_MAX_SAMPLE_FRAMES: u64 = 1 << 24;
pub const SFZ_MAX_SAMPLE_CHANNELS: u32 = 2;
pub const SFZ_MIN_SAMPLE_RATE: u32 = 8_000;
pub const SFZ_MAX_SAMPLE_RATE: u32 = 192_000;
pub const SFZ_MAX_DECODED_BYTES: u64 = 1 << 30;
pub const SFZ_MAX_VORBIS_ENTRIES: u64 = 1 << 18;
pub const SFZ_MAX_VORBIS_SETUP_BYTES: u64 = 32 << 20;
pub const SFZ_MAX_VOICES: usize = 128;
pub const SFZ_MAX_VOICES_PER_NOTE: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SfzLimits {
    pub file_bytes: u64,
    pub expanded_bytes: u64,
    pub bank_text_bytes: u64,
    pub regions: u64,
    pub include_depth: u64,
    pub includes: u64,
    pub defines: u64,
    pub define_bytes: u64,
    pub define_name_bytes: usize,
    pub line_bytes: u64,
    pub read_bytes: u64,
    pub listed_entries: u64,
    pub path_bytes: usize,
    pub path_lookups: u64,
    pub folder_entries: u64,
    pub folder_depth: u64,
    pub sfz_files: u64,
    pub loader_bytes: u64,
    pub sidecar_bytes: u64,
    pub audio_file_bytes: u64,
    pub sample_frames: u64,
    pub sample_channels: u32,
    pub min_sample_rate: u32,
    pub max_sample_rate: u32,
    pub decoded_bytes: u64,
    pub vorbis_entries: u64,
    pub vorbis_setup_bytes: u64,
    pub vorbis_comment_bytes: u64,
    pub voices: usize,
    pub voices_per_note: usize,
}

impl SfzLimits {
    pub const DEFAULT: SfzLimits = SfzLimits {
        file_bytes: SFZ_MAX_FILE_BYTES,
        expanded_bytes: SFZ_MAX_EXPANDED_BYTES,
        bank_text_bytes: SFZ_MAX_BANK_TEXT_BYTES,
        regions: SFZ_MAX_REGIONS,
        include_depth: SFZ_MAX_INCLUDE_DEPTH,
        includes: SFZ_MAX_INCLUDES,
        defines: SFZ_MAX_DEFINES,
        define_bytes: SFZ_MAX_DEFINE_BYTES,
        define_name_bytes: SFZ_MAX_DEFINE_NAME_BYTES,
        line_bytes: SFZ_MAX_LINE_BYTES,
        read_bytes: SFZ_MAX_READ_BYTES,
        listed_entries: SFZ_MAX_LISTED_ENTRIES,
        path_bytes: SFZ_MAX_PATH_BYTES,
        path_lookups: SFZ_MAX_PATH_LOOKUPS,
        folder_entries: SFZ_MAX_FOLDER_ENTRIES,
        folder_depth: SFZ_MAX_FOLDER_DEPTH,
        sfz_files: SFZ_MAX_SFZ_FILES,
        loader_bytes: SFZ_MAX_LOADER_BYTES,
        sidecar_bytes: SFZ_MAX_SIDECAR_BYTES,
        audio_file_bytes: SFZ_MAX_AUDIO_FILE_BYTES,
        sample_frames: SFZ_MAX_SAMPLE_FRAMES,
        sample_channels: SFZ_MAX_SAMPLE_CHANNELS,
        min_sample_rate: SFZ_MIN_SAMPLE_RATE,
        max_sample_rate: SFZ_MAX_SAMPLE_RATE,
        decoded_bytes: SFZ_MAX_DECODED_BYTES,
        vorbis_entries: SFZ_MAX_VORBIS_ENTRIES,
        vorbis_setup_bytes: SFZ_MAX_VORBIS_SETUP_BYTES,
        vorbis_comment_bytes: SFZ_MAX_VORBIS_COMMENT_BYTES,
        voices: SFZ_MAX_VOICES,
        voices_per_note: SFZ_MAX_VOICES_PER_NOTE,
    };
}

impl Default for SfzLimits {
    fn default() -> SfzLimits {
        SfzLimits::DEFAULT
    }
}

pub(crate) const KIB: u64 = 1 << 10;
pub(crate) const MIB: u64 = 1 << 20;
