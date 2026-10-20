use std::io::Write;

use bzip2::Compression as BzipLevel;
use bzip2::write::BzEncoder;
use flate2::Compression as DeflateLevel;
use flate2::write::DeflateEncoder;

use super::crc32;

pub const UNIX_LINK_ATTRIBUTES: u32 = 0x8000 | (0o120_777 << 16);

const SIGNATURE: [u8; 6] = [b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C];
const VERSION: [u8; 2] = [0, 4];
const SIGNATURE_HEADER_BYTES: usize = 32;
const LZMA2_CHUNK: usize = 0x1_0000;
const LZMA2_DICTIONARY: u8 = 16;
const DIRECTORY_ATTRIBUTE: u32 = 0x10;
const K_END: u8 = 0x00;
const K_HEADER: u8 = 0x01;
const K_MAIN_STREAMS: u8 = 0x04;
const K_FILES: u8 = 0x05;
const K_PACK_INFO: u8 = 0x06;
const K_UNPACK_INFO: u8 = 0x07;
const K_SUB_STREAMS: u8 = 0x08;
const K_SIZE: u8 = 0x09;
const K_CRC: u8 = 0x0A;
const K_FOLDER: u8 = 0x0B;
const K_UNPACK_SIZE: u8 = 0x0C;
const K_NUM_UNPACK: u8 = 0x0D;
const K_EMPTY_STREAM: u8 = 0x0E;
const K_EMPTY_FILE: u8 = 0x0F;
const K_NAME: u8 = 0x11;
const K_ATTRIBUTES: u8 = 0x15;
const HAS_PROPERTIES: u8 = 0x20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SevenMethod {
    Copy,
    Deflate,
    Bzip2,
    Lzma2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SevenFile {
    pub name: String,
    pub data: Vec<u8>,
    pub attributes: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SevenFolder {
    pub method: SevenMethod,
    pub files: Vec<SevenFile>,
}

struct Entry {
    name: String,
    attributes: Option<u32>,
    stream: bool,
    directory: bool,
}

fn number(out: &mut Vec<u8>, value: u64) {
    let mut first = 0u8;
    let mut mask = 0x80u8;
    let mut extra = 0usize;
    while extra < 8 {
        if value < (1u64 << (7 * (extra + 1))) {
            first |= u8::try_from(value >> (8 * extra)).unwrap_or(0);
            break;
        }
        first |= mask;
        mask >>= 1;
        extra += 1;
    }
    out.push(first);
    for index in 0..extra {
        out.push(u8::try_from((value >> (8 * index)) & 0xFF).unwrap_or(0));
    }
}

fn size(out: &mut Vec<u8>, value: usize) {
    number(out, u64::try_from(value).unwrap_or(u64::MAX));
}

fn bits(out: &mut Vec<u8>, flags: &[bool]) {
    for chunk in flags.chunks(8) {
        let mut byte = 0u8;
        for (index, flag) in chunk.iter().enumerate() {
            if *flag {
                byte |= 0x80 >> index;
            }
        }
        out.push(byte);
    }
}

fn lzma2_uncompressed(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for (index, chunk) in data.chunks(LZMA2_CHUNK).enumerate() {
        out.push(if index == 0 { 0x01 } else { 0x02 });
        let length = u16::try_from(chunk.len() - 1).unwrap_or(u16::MAX);
        out.extend_from_slice(&length.to_be_bytes());
        out.extend_from_slice(chunk);
    }
    out.push(0x00);
    out
}

fn encode(method: SevenMethod, data: &[u8]) -> Vec<u8> {
    let outcome = match method {
        SevenMethod::Copy => Ok(data.to_vec()),
        SevenMethod::Lzma2 => Ok(lzma2_uncompressed(data)),
        SevenMethod::Deflate => {
            let mut encoder = DeflateEncoder::new(Vec::new(), DeflateLevel::default());
            encoder.write_all(data).and_then(|()| encoder.finish())
        }
        SevenMethod::Bzip2 => {
            let mut encoder = BzEncoder::new(Vec::new(), BzipLevel::default());
            encoder.write_all(data).and_then(|()| encoder.finish())
        }
    };
    match outcome {
        Ok(bytes) => bytes,
        Err(error) => panic!("cannot encode the test 7z stream: {error}"),
    }
}

fn coder(out: &mut Vec<u8>, method: SevenMethod) {
    match method {
        SevenMethod::Copy => out.extend_from_slice(&[0x01, 0x00]),
        SevenMethod::Deflate => out.extend_from_slice(&[0x03, 0x04, 0x01, 0x08]),
        SevenMethod::Bzip2 => out.extend_from_slice(&[0x03, 0x04, 0x02, 0x02]),
        SevenMethod::Lzma2 => {
            out.extend_from_slice(&[HAS_PROPERTIES | 0x01, 0x21, 0x01, LZMA2_DICTIONARY])
        }
    }
}

fn streams(out: &mut Vec<u8>, folders: &[SevenFolder], packed: &[Vec<u8>]) {
    out.push(K_MAIN_STREAMS);
    out.push(K_PACK_INFO);
    number(out, 0);
    size(out, packed.len());
    out.push(K_SIZE);
    for stream in packed {
        size(out, stream.len());
    }
    out.push(K_END);
    out.push(K_UNPACK_INFO);
    out.push(K_FOLDER);
    size(out, folders.len());
    out.push(0);
    for folder in folders {
        out.push(1);
        coder(out, folder.method);
    }
    out.push(K_UNPACK_SIZE);
    for folder in folders {
        let total: usize = folder.files.iter().map(|file| file.data.len()).sum();
        size(out, total);
    }
    out.push(K_END);
    out.push(K_SUB_STREAMS);
    out.push(K_NUM_UNPACK);
    for folder in folders {
        size(out, folder.files.len());
    }
    out.push(K_SIZE);
    for folder in folders {
        if let Some((_, leading)) = folder.files.split_last() {
            for file in leading {
                size(out, file.data.len());
            }
        }
    }
    out.push(K_CRC);
    out.push(1);
    for folder in folders {
        for file in &folder.files {
            out.extend_from_slice(&crc32(&file.data).to_le_bytes());
        }
    }
    out.push(K_END);
    out.push(K_END);
}

fn property(out: &mut Vec<u8>, kind: u8, body: &[u8]) {
    out.push(kind);
    size(out, body.len());
    out.extend_from_slice(body);
}

fn files(out: &mut Vec<u8>, entries: &[Entry]) {
    out.push(K_FILES);
    size(out, entries.len());
    let empty: Vec<bool> = entries.iter().map(|entry| !entry.stream).collect();
    if empty.iter().any(|flag| *flag) {
        let mut body = Vec::new();
        bits(&mut body, &empty);
        property(out, K_EMPTY_STREAM, &body);
        let empty_files: Vec<bool> = entries
            .iter()
            .filter(|entry| !entry.stream)
            .map(|entry| !entry.directory)
            .collect();
        let mut body = Vec::new();
        bits(&mut body, &empty_files);
        property(out, K_EMPTY_FILE, &body);
    }
    let mut names = vec![0u8];
    for entry in entries {
        for unit in entry.name.encode_utf16() {
            names.extend_from_slice(&unit.to_le_bytes());
        }
        names.extend_from_slice(&[0, 0]);
    }
    property(out, K_NAME, &names);
    let mut attributes = vec![1u8, 0u8];
    for entry in entries {
        let value = entry.attributes.unwrap_or(if entry.directory {
            DIRECTORY_ATTRIBUTE
        } else {
            0
        });
        attributes.extend_from_slice(&value.to_le_bytes());
    }
    property(out, K_ATTRIBUTES, &attributes);
    out.push(K_END);
}

pub fn seven_zip(folders: &[SevenFolder], folders_only: &[String]) -> Vec<u8> {
    let folders: Vec<SevenFolder> = folders
        .iter()
        .filter(|folder| !folder.files.is_empty())
        .cloned()
        .collect();
    let packed: Vec<Vec<u8>> = folders
        .iter()
        .map(|folder| {
            let joined: Vec<u8> = folder
                .files
                .iter()
                .flat_map(|file| file.data.iter().copied())
                .collect();
            encode(folder.method, &joined)
        })
        .collect();
    let mut entries: Vec<Entry> = folders
        .iter()
        .flat_map(|folder| folder.files.iter())
        .map(|file| Entry {
            name: file.name.clone(),
            attributes: file.attributes,
            stream: true,
            directory: false,
        })
        .collect();
    entries.extend(folders_only.iter().map(|name| Entry {
        name: name.clone(),
        attributes: None,
        stream: false,
        directory: true,
    }));
    let mut header = vec![K_HEADER];
    if !packed.is_empty() {
        streams(&mut header, &folders, &packed);
    }
    files(&mut header, &entries);
    header.push(K_END);
    let body: Vec<u8> = packed
        .iter()
        .flat_map(|stream| stream.iter().copied())
        .collect();
    let mut start = Vec::with_capacity(20);
    start.extend_from_slice(&u64::try_from(body.len()).unwrap_or(0).to_le_bytes());
    start.extend_from_slice(&u64::try_from(header.len()).unwrap_or(0).to_le_bytes());
    start.extend_from_slice(&crc32(&header).to_le_bytes());
    let mut archive = Vec::with_capacity(SIGNATURE_HEADER_BYTES + body.len() + header.len());
    archive.extend_from_slice(&SIGNATURE);
    archive.extend_from_slice(&VERSION);
    archive.extend_from_slice(&crc32(&start).to_le_bytes());
    archive.extend_from_slice(&start);
    archive.extend_from_slice(&body);
    archive.extend_from_slice(&header);
    archive
}
