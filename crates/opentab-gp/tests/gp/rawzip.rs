use std::io::Write;

use flate2::Compression;
use flate2::write::DeflateEncoder;

pub const METHOD_STORED: u16 = 0;
pub const METHOD_DEFLATED: u16 = 8;
const FLAG_DESCRIPTOR: u16 = 1 << 3;
const DOS_DATE: u16 = 0x0021;
const VERSION: u16 = 20;
const LOCAL_SIGNATURE: u32 = 0x0403_4b50;
const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
const END_SIGNATURE: u32 = 0x0605_4b50;
const DESCRIPTOR_SIGNATURE: u32 = 0x0807_4b50;

pub const LOCAL_HEADER_SIZE: usize = 30;
pub const CENTRAL_HEADER_SIZE: usize = 46;
pub const END_RECORD_SIZE: usize = 22;

#[derive(Debug, Clone)]
pub struct RawEntry {
    pub name: Vec<u8>,
    pub method: u16,
    pub descriptor: bool,
    pub crc: u32,
    pub payload: Vec<u8>,
    pub compressed_size: u32,
    pub size: u32,
}

pub fn checksum(data: &[u8]) -> u32 {
    let mut crc = flate2::Crc::new();
    crc.update(data);
    crc.sum()
}

pub fn deflate(data: &[u8]) -> Vec<u8> {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).expect("deflate");
    encoder.finish().expect("deflate finish")
}

fn length(value: usize) -> u32 {
    u32::try_from(value).expect("fits u32")
}

impl RawEntry {
    pub fn stored(name: &str, data: &[u8]) -> RawEntry {
        RawEntry {
            name: name.as_bytes().to_vec(),
            method: METHOD_STORED,
            descriptor: false,
            crc: checksum(data),
            payload: data.to_vec(),
            compressed_size: length(data.len()),
            size: length(data.len()),
        }
    }

    pub fn deflated(name: &str, data: &[u8]) -> RawEntry {
        let payload = deflate(data);
        RawEntry::precompressed(name, payload, data)
    }

    pub fn precompressed(name: &str, payload: Vec<u8>, data: &[u8]) -> RawEntry {
        RawEntry {
            name: name.as_bytes().to_vec(),
            method: METHOD_DEFLATED,
            descriptor: false,
            crc: checksum(data),
            compressed_size: length(payload.len()),
            size: length(data.len()),
            payload,
        }
    }

    pub fn with_descriptor(mut self) -> RawEntry {
        self.descriptor = true;
        self
    }

    pub fn declaring(mut self, size: u32) -> RawEntry {
        self.size = size;
        self
    }

    fn flags(&self) -> u16 {
        if self.descriptor { FLAG_DESCRIPTOR } else { 0 }
    }
}

fn push16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_local(out: &mut Vec<u8>, entry: &RawEntry) {
    push32(out, LOCAL_SIGNATURE);
    push16(out, VERSION);
    push16(out, entry.flags());
    push16(out, entry.method);
    push16(out, 0);
    push16(out, DOS_DATE);
    if entry.descriptor {
        push32(out, 0);
        push32(out, 0);
        push32(out, 0);
    } else {
        push32(out, entry.crc);
        push32(out, entry.compressed_size);
        push32(out, entry.size);
    }
    push16(out, u16::try_from(entry.name.len()).expect("name length"));
    push16(out, 0);
    out.extend_from_slice(&entry.name);
    out.extend_from_slice(&entry.payload);
    if entry.descriptor {
        push32(out, DESCRIPTOR_SIGNATURE);
        push32(out, entry.crc);
        push32(out, entry.compressed_size);
        push32(out, entry.size);
    }
}

fn write_central(out: &mut Vec<u8>, entry: &RawEntry, offset: u32) {
    push32(out, CENTRAL_SIGNATURE);
    push16(out, VERSION);
    push16(out, VERSION);
    push16(out, entry.flags());
    push16(out, entry.method);
    push16(out, 0);
    push16(out, DOS_DATE);
    push32(out, entry.crc);
    push32(out, entry.compressed_size);
    push32(out, entry.size);
    push16(out, u16::try_from(entry.name.len()).expect("name length"));
    push16(out, 0);
    push16(out, 0);
    push16(out, 0);
    push16(out, 0);
    push32(out, 0);
    push32(out, offset);
    out.extend_from_slice(&entry.name);
}

pub fn build(entries: &[RawEntry]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut offsets = Vec::with_capacity(entries.len());
    for entry in entries {
        offsets.push(length(out.len()));
        write_local(&mut out, entry);
    }
    let directory_start = length(out.len());
    for (entry, offset) in entries.iter().zip(&offsets) {
        write_central(&mut out, entry, *offset);
    }
    let directory_size = length(out.len()) - directory_start;
    let count = u16::try_from(entries.len()).expect("entry count");
    push32(&mut out, END_SIGNATURE);
    push16(&mut out, 0);
    push16(&mut out, 0);
    push16(&mut out, count);
    push16(&mut out, count);
    push32(&mut out, directory_size);
    push32(&mut out, directory_start);
    push16(&mut out, 0);
    out
}
