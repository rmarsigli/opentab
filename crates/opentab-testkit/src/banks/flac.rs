const BLOCK_FRAMES: usize = 4096;
const STREAMINFO_BYTES: u32 = 34;
const LAST_STREAMINFO: u8 = 0x80;
const FIXED_SYNC: [u8; 2] = [0xFF, 0xF8];
const BLOCK_SIZE_16_BIT: u8 = 0b0111;
const RATE_FROM_STREAMINFO: u8 = 0b0000;
const VERBATIM_SUBFRAME: u8 = 0b0000_0010;
const CRC8_POLY: u8 = 0x07;
const CRC16_POLY: u16 = 0x8005;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlacSpec {
    pub rate: u32,
    pub channels: u8,
    pub bits: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlacOverrides {
    pub declared_frames: Option<u64>,
    pub declared_channels: Option<u8>,
    pub declared_rate: Option<u32>,
    pub truncate_to: Option<usize>,
}

impl FlacOverrides {
    pub const NONE: FlacOverrides = FlacOverrides {
        declared_frames: None,
        declared_channels: None,
        declared_rate: None,
        truncate_to: None,
    };
}

struct BitWriter {
    bytes: Vec<u8>,
    used: u32,
}

impl BitWriter {
    fn new() -> BitWriter {
        BitWriter {
            bytes: Vec::new(),
            used: 0,
        }
    }

    fn put(&mut self, value: u64, bits: u32) {
        for index in (0..bits).rev() {
            if self.used.is_multiple_of(8) {
                self.bytes.push(0);
            }
            let bit = ((value >> index) & 1) as u8;
            if let Some(last) = self.bytes.last_mut() {
                *last |= bit << (7 - self.used % 8);
            }
            self.used += 1;
        }
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

fn crc8(bytes: &[u8]) -> u8 {
    let mut crc = 0u8;
    for byte in bytes {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ CRC8_POLY
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn crc16(bytes: &[u8]) -> u16 {
    let mut crc = 0u16;
    for byte in bytes {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ CRC16_POLY
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn utf8_number(number: u32) -> Vec<u8> {
    if number < 0x80 {
        vec![number as u8]
    } else if number < 0x800 {
        vec![0xC0 | (number >> 6) as u8, 0x80 | (number & 0x3F) as u8]
    } else {
        vec![
            0xE0 | (number >> 12) as u8,
            0x80 | ((number >> 6) & 0x3F) as u8,
            0x80 | (number & 0x3F) as u8,
        ]
    }
}

fn sample_size_code(bits: u8) -> u8 {
    match bits {
        8 => 0b001,
        12 => 0b010,
        16 => 0b100,
        20 => 0b101,
        24 => 0b110,
        _ => 0b000,
    }
}

fn streaminfo(spec: &FlacSpec, frames: u64, overrides: &FlacOverrides) -> Vec<u8> {
    let mut writer = BitWriter::new();
    writer.put(BLOCK_FRAMES as u64, 16);
    writer.put(BLOCK_FRAMES as u64, 16);
    writer.put(0, 24);
    writer.put(0, 24);
    let rate = overrides.declared_rate.unwrap_or(spec.rate);
    let channels = overrides.declared_channels.unwrap_or(spec.channels);
    writer.put(u64::from(rate), 20);
    writer.put(u64::from(channels.saturating_sub(1)), 3);
    writer.put(u64::from(spec.bits - 1), 5);
    writer.put(overrides.declared_frames.unwrap_or(frames), 36);
    writer.put(0, 64);
    writer.put(0, 64);
    writer.finish()
}

fn frame(spec: &FlacSpec, number: u32, block: &[i32]) -> Vec<u8> {
    let channels = usize::from(spec.channels);
    let frames = block.len() / channels;
    let mut header = Vec::new();
    header.extend_from_slice(&FIXED_SYNC);
    header.push((BLOCK_SIZE_16_BIT << 4) | RATE_FROM_STREAMINFO);
    header.push(((spec.channels - 1) << 4) | (sample_size_code(spec.bits) << 1));
    header.extend_from_slice(&utf8_number(number));
    header.extend_from_slice(&u16::try_from(frames - 1).unwrap_or(0).to_be_bytes());
    header.push(crc8(&header));
    let mut writer = BitWriter::new();
    let mask = (1u64 << spec.bits) - 1;
    for channel in 0..channels {
        writer.put(u64::from(VERBATIM_SUBFRAME), 8);
        for index in 0..frames {
            let value = block[index * channels + channel] as i64 as u64 & mask;
            writer.put(value, u32::from(spec.bits));
        }
    }
    let mut out = header;
    out.extend_from_slice(&writer.finish());
    let crc = crc16(&out);
    out.extend_from_slice(&crc.to_be_bytes());
    out
}

pub fn write_flac(spec: &FlacSpec, interleaved: &[i32]) -> Vec<u8> {
    write_flac_with(spec, interleaved, &FlacOverrides::NONE)
}

pub fn write_flac_with(spec: &FlacSpec, interleaved: &[i32], overrides: &FlacOverrides) -> Vec<u8> {
    let channels = usize::from(spec.channels.max(1));
    let frames = (interleaved.len() / channels) as u64;
    let mut out = Vec::new();
    out.extend_from_slice(b"fLaC");
    out.push(LAST_STREAMINFO);
    out.extend_from_slice(&STREAMINFO_BYTES.to_be_bytes()[1..]);
    out.extend_from_slice(&streaminfo(spec, frames, overrides));
    for (number, block) in interleaved.chunks(BLOCK_FRAMES * channels).enumerate() {
        out.extend_from_slice(&frame(spec, number as u32, block));
    }
    if let Some(length) = overrides.truncate_to {
        out.truncate(length);
    }
    out
}
