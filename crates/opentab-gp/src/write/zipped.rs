use miniz_oxide::deflate::compress_to_vec;

const LOCAL_SIGNATURE: u32 = 0x0403_4b50;
const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
const END_SIGNATURE: u32 = 0x0605_4b50;
const DEFLATE_LEVEL: u8 = 6;
const VERSION_NEEDED: u16 = 20;
const MADE_BY: u16 = (3 << 8) | VERSION_NEEDED;
const DEFLATED: u16 = 8;
const STORED: u16 = 0;
const UTF8_NAME: u16 = 1 << 11;
const MODIFIED_TIME: u16 = 0;
const MODIFIED_DATE: u16 = (1 << 5) | 1;
const FILE_MODE: u32 = 0o100_644;
const DIRECTORY_MODE: u32 = 0o040_755;
const MODE_SHIFT: u32 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ZipKind {
    File,
    Directory,
}

pub(crate) struct ZipEntry<'d> {
    pub name: &'d str,
    pub data: &'d [u8],
    pub kind: ZipKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ZipFailure {
    Name { index: usize },
    TooLarge,
}

struct Record {
    flags: u16,
    method: u16,
    crc: u32,
    compressed: u32,
    size: u32,
    mode: u32,
    offset: u32,
}

fn method_of(kind: ZipKind) -> u16 {
    match kind {
        ZipKind::File => DEFLATED,
        ZipKind::Directory => STORED,
    }
}

fn mode_of(kind: ZipKind) -> u32 {
    match kind {
        ZipKind::File => FILE_MODE,
        ZipKind::Directory => DIRECTORY_MODE,
    }
}

fn push16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn small(value: usize) -> Result<u32, ZipFailure> {
    u32::try_from(value)
        .ok()
        .filter(|value| *value < u32::MAX)
        .ok_or(ZipFailure::TooLarge)
}

fn push_common(out: &mut Vec<u8>, record: &Record, name: &[u8]) -> Result<(), ZipFailure> {
    push16(out, VERSION_NEEDED);
    push16(out, record.flags);
    push16(out, record.method);
    push16(out, MODIFIED_TIME);
    push16(out, MODIFIED_DATE);
    push32(out, record.crc);
    push32(out, record.compressed);
    push32(out, record.size);
    let length = u16::try_from(name.len()).map_err(|_| ZipFailure::TooLarge)?;
    push16(out, length);
    push16(out, 0);
    Ok(())
}

pub(crate) fn zip(entries: &[ZipEntry<'_>]) -> Result<Vec<u8>, ZipFailure> {
    let count = u16::try_from(entries.len()).map_err(|_| ZipFailure::TooLarge)?;
    let mut out = Vec::new();
    let mut records = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let name = entry.name.as_bytes();
        if u16::try_from(name.len()).is_err() {
            return Err(ZipFailure::Name { index });
        }
        let body = match entry.kind {
            ZipKind::File => compress_to_vec(entry.data, DEFLATE_LEVEL),
            ZipKind::Directory => entry.data.to_vec(),
        };
        let record = Record {
            flags: if entry.name.is_ascii() { 0 } else { UTF8_NAME },
            method: method_of(entry.kind),
            crc: crc32fast::hash(entry.data),
            compressed: small(body.len())?,
            size: small(entry.data.len())?,
            mode: mode_of(entry.kind),
            offset: small(out.len())?,
        };
        push32(&mut out, LOCAL_SIGNATURE);
        push_common(&mut out, &record, name)?;
        out.extend_from_slice(name);
        out.extend_from_slice(&body);
        records.push(record);
    }
    let directory = small(out.len())?;
    for (entry, record) in entries.iter().zip(&records) {
        let name = entry.name.as_bytes();
        push32(&mut out, CENTRAL_SIGNATURE);
        push16(&mut out, MADE_BY);
        push_common(&mut out, record, name)?;
        push16(&mut out, 0);
        push16(&mut out, 0);
        push16(&mut out, 0);
        push32(&mut out, record.mode << MODE_SHIFT);
        push32(&mut out, record.offset);
        out.extend_from_slice(name);
    }
    let size = small(out.len())? - directory;
    push32(&mut out, END_SIGNATURE);
    push16(&mut out, 0);
    push16(&mut out, 0);
    push16(&mut out, count);
    push16(&mut out, count);
    push32(&mut out, size);
    push32(&mut out, directory);
    push16(&mut out, 0);
    Ok(out)
}
