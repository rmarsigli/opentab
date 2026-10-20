mod sevenz;
mod zipped;

pub use sevenz::{SevenFile, SevenFolder, SevenMethod, UNIX_LINK_ATTRIBUTES, seven_zip};
pub use zipped::{ZipItem, zip_archive};

pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}
