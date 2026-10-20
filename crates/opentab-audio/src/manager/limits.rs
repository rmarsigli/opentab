pub const BANK_MAX_BYTES: u64 = 4 << 30;
pub const BANK_MAX_FILE_BYTES: u64 = 1 << 30;
pub const BANK_MAX_ENTRIES: u64 = 200_000;
pub const BANK_MAX_DEPTH: u64 = 16;
pub const BANK_MAX_NAME_BYTES: u64 = 1024;
pub const STORE_MAX_BYTES: u64 = 16 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImportLimits {
    pub total_bytes: u64,
    pub file_bytes: u64,
    pub entries: u64,
    pub depth: u64,
    pub name_bytes: u64,
}

impl ImportLimits {
    pub const DEFAULT: ImportLimits = ImportLimits {
        total_bytes: BANK_MAX_BYTES,
        file_bytes: BANK_MAX_FILE_BYTES,
        entries: BANK_MAX_ENTRIES,
        depth: BANK_MAX_DEPTH,
        name_bytes: BANK_MAX_NAME_BYTES,
    };
}

impl Default for ImportLimits {
    fn default() -> ImportLimits {
        ImportLimits::DEFAULT
    }
}
