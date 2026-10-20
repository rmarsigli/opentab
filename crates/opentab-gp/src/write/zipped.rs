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
const SIZE_LIMIT: u32 = u32::MAX;

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

fn small(value: usize, limit: u32) -> Result<u32, ZipFailure> {
    u32::try_from(value)
        .ok()
        .filter(|value| *value < limit)
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
    zip_within(entries, SIZE_LIMIT)
}

fn zip_within(entries: &[ZipEntry<'_>], limit: u32) -> Result<Vec<u8>, ZipFailure> {
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
            compressed: small(body.len(), limit)?,
            size: small(entry.data.len(), limit)?,
            mode: mode_of(entry.kind),
            offset: small(out.len(), limit)?,
        };
        push32(&mut out, LOCAL_SIGNATURE);
        push_common(&mut out, &record, name)?;
        out.extend_from_slice(name);
        out.extend_from_slice(&body);
        records.push(record);
    }
    let directory = small(out.len(), limit)?;
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
    let size = small(out.len(), limit)? - directory;
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

#[cfg(test)]
mod tests {
    use super::{SIZE_LIMIT, UTF8_NAME, ZipEntry, ZipFailure, ZipKind, small, zip, zip_within};
    use std::io::{Cursor, Read};

    const LARGE_BYTES: usize = 200 * 1024;
    const FLAGS_AT: usize = 6;
    const TINY_LIMIT: u32 = 128;
    const LOCAL_HEADER: usize = 30;

    struct ReadBack {
        name: String,
        data: Vec<u8>,
        directory: bool,
    }

    fn read_back(bytes: &[u8]) -> Vec<ReadBack> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("archive");
        let mut entries = Vec::new();
        for index in 0..archive.len() {
            let mut file = archive.by_index(index).expect("entry");
            let mut data = Vec::new();
            file.read_to_end(&mut data).expect("data");
            entries.push(ReadBack {
                name: file.name().to_string(),
                data,
                directory: file.is_dir(),
            });
        }
        entries
    }

    fn flags(bytes: &[u8], offset: usize) -> u16 {
        u16::from_le_bytes([bytes[offset + FLAGS_AT], bytes[offset + FLAGS_AT + 1]])
    }

    #[test]
    fn utf8_names_empty_entries_and_large_entries_read_back_with_the_zip_crate() {
        let large: Vec<u8> = (0..LARGE_BYTES)
            .map(|index| u8::try_from(index * 7 % 251).expect("byte"))
            .collect();
        let entries = [
            ZipEntry {
                name: "Content/",
                data: &[],
                kind: ZipKind::Directory,
            },
            ZipEntry {
                name: "Content/Sérénade 日本語.txt",
                data: b"name",
                kind: ZipKind::File,
            },
            ZipEntry {
                name: "Content/empty",
                data: &[],
                kind: ZipKind::File,
            },
            ZipEntry {
                name: "Content/large",
                data: &large,
                kind: ZipKind::File,
            },
        ];
        let bytes = zip(&entries).expect("written");
        let read = read_back(&bytes);
        assert_eq!(read.len(), entries.len());
        for (back, entry) in read.iter().zip(&entries) {
            assert_eq!(back.name, entry.name);
            assert_eq!(back.data, entry.data);
            assert_eq!(back.directory, entry.kind == ZipKind::Directory);
        }
        let ascii = flags(&bytes, 0);
        let second = LOCAL_HEADER + "Content/".len();
        assert_eq!(ascii & UTF8_NAME, 0);
        assert_eq!(flags(&bytes, second) & UTF8_NAME, UTF8_NAME);
    }

    #[test]
    fn sizes_and_offsets_reaching_the_limit_are_refused() {
        assert_eq!(SIZE_LIMIT, u32::MAX);
        let limit = usize::try_from(SIZE_LIMIT).expect("usize holds u32");
        assert_eq!(small(limit, SIZE_LIMIT), Err(ZipFailure::TooLarge));
        assert_eq!(small(limit - 1, SIZE_LIMIT), Ok(SIZE_LIMIT - 1));
        assert_eq!(small(limit + 1, SIZE_LIMIT), Err(ZipFailure::TooLarge));
        let tiny = usize::try_from(TINY_LIMIT).expect("usize");
        let data = vec![0u8; tiny];
        let refused = [ZipEntry {
            name: "a",
            data: &data,
            kind: ZipKind::File,
        }];
        assert_eq!(zip_within(&refused, TINY_LIMIT), Err(ZipFailure::TooLarge));
        let long_archive = [ZipEntry {
            name: "a",
            data: &data[..tiny - 1],
            kind: ZipKind::Directory,
        }];
        assert_eq!(
            zip_within(&long_archive, TINY_LIMIT),
            Err(ZipFailure::TooLarge)
        );
        let fits = [ZipEntry {
            name: "a",
            data: b"x",
            kind: ZipKind::File,
        }];
        let written = zip_within(&fits, TINY_LIMIT).expect("fits");
        assert_eq!(read_back(&written)[0].data, b"x");
    }
}
