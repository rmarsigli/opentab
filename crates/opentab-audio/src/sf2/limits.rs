pub const MAX_FILE_BYTES: u64 = 2 << 30;
pub const MAX_LOADER_BYTES: u64 = 256 << 20;
pub const MAX_REGION_PAIRS: u64 = 1 << 22;
pub const MAX_PAIRS_PER_PRESET: u64 = 1 << 16;
pub const MAX_LAYERS_PER_NOTE: u32 = 64;
pub const MAX_VOICES_PER_NOTE: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sf2Limits {
    pub file_bytes: u64,
    pub loader_bytes: u64,
    pub region_pairs: u64,
    pub pairs_per_preset: u64,
    pub layers_per_note: u32,
    pub voices_per_note: u32,
}

impl Sf2Limits {
    pub const DEFAULT: Sf2Limits = Sf2Limits {
        file_bytes: MAX_FILE_BYTES,
        loader_bytes: MAX_LOADER_BYTES,
        region_pairs: MAX_REGION_PAIRS,
        pairs_per_preset: MAX_PAIRS_PER_PRESET,
        layers_per_note: MAX_LAYERS_PER_NOTE,
        voices_per_note: MAX_VOICES_PER_NOTE,
    };
}

impl Default for Sf2Limits {
    fn default() -> Sf2Limits {
        Sf2Limits::DEFAULT
    }
}
