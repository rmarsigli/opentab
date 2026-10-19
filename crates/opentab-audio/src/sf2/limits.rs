pub const MAX_FILE_BYTES: u64 = 1 << 30;
pub const LOADER_FILE_FACTOR: u64 = 2;
pub const LOADER_SLACK_BYTES: u64 = 256 << 20;
pub const MAX_REGION_PAIRS: u64 = 1 << 22;
pub const MAX_PAIRS_PER_PRESET: u64 = 1 << 16;
pub const MAX_LAYERS_PER_NOTE: u32 = 64;
pub const MAX_VOICES_PER_NOTE: u32 = 64;
pub const MAX_GRID_PRESETS: u64 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sf2Limits {
    pub file_bytes: u64,
    pub loader_file_factor: u64,
    pub loader_slack_bytes: u64,
    pub region_pairs: u64,
    pub pairs_per_preset: u64,
    pub layers_per_note: u32,
    pub voices_per_note: u32,
    pub grid_presets: u64,
}

impl Sf2Limits {
    pub const DEFAULT: Sf2Limits = Sf2Limits {
        file_bytes: MAX_FILE_BYTES,
        loader_file_factor: LOADER_FILE_FACTOR,
        loader_slack_bytes: LOADER_SLACK_BYTES,
        region_pairs: MAX_REGION_PAIRS,
        pairs_per_preset: MAX_PAIRS_PER_PRESET,
        layers_per_note: MAX_LAYERS_PER_NOTE,
        voices_per_note: MAX_VOICES_PER_NOTE,
        grid_presets: MAX_GRID_PRESETS,
    };

    pub fn loader_allowance(&self, file_bytes: u64) -> Option<u64> {
        self.loader_file_factor
            .checked_mul(file_bytes)?
            .checked_add(self.loader_slack_bytes)
    }
}

impl Default for Sf2Limits {
    fn default() -> Sf2Limits {
        Sf2Limits::DEFAULT
    }
}
