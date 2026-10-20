use std::ops::Range;

use super::source::Source;
use crate::error::SF3_UNSUPPORTED;
use crate::{BankError, Sf2Limits, UnsafeBank};

const HEADER_BYTES: usize = 8;
const FORM_BYTES: usize = 4;
const VERSION_BYTES: usize = 4;
const SF3_MAJOR: u16 = 3;
const OGG_MAGIC: &[u8; 4] = b"OggS";
const MIN_SAMPLE_BYTES: usize = 4;
const LISTS: [&[u8; 4]; 3] = [b"INFO", b"sdta", b"pdta"];
const INFO_TEXTS: [&[u8; 4]; 9] = [
    b"isng", b"INAM", b"irom", b"ICRD", b"IENG", b"IPRD", b"ICOP", b"ICMT", b"ISFT",
];
const MIB_SHIFT: u32 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TableSpec {
    pub(crate) id: &'static [u8; 4],
    pub(crate) width: usize,
    pub(crate) minimum: usize,
}

pub(crate) const PHDR: TableSpec = TableSpec {
    id: b"phdr",
    width: 38,
    minimum: 2,
};
pub(crate) const PBAG: TableSpec = TableSpec {
    id: b"pbag",
    width: 4,
    minimum: 2,
};
pub(crate) const PMOD: TableSpec = TableSpec {
    id: b"pmod",
    width: 10,
    minimum: 1,
};
pub(crate) const PGEN: TableSpec = TableSpec {
    id: b"pgen",
    width: 4,
    minimum: 1,
};
pub(crate) const INST: TableSpec = TableSpec {
    id: b"inst",
    width: 22,
    minimum: 2,
};
pub(crate) const IBAG: TableSpec = TableSpec {
    id: b"ibag",
    width: 4,
    minimum: 2,
};
pub(crate) const IMOD: TableSpec = TableSpec {
    id: b"imod",
    width: 10,
    minimum: 1,
};
pub(crate) const IGEN: TableSpec = TableSpec {
    id: b"igen",
    width: 4,
    minimum: 1,
};
pub(crate) const SHDR: TableSpec = TableSpec {
    id: b"shdr",
    width: 46,
    minimum: 1,
};
pub(crate) const TABLES: [TableSpec; 9] = [PHDR, PBAG, PMOD, PGEN, INST, IBAG, IMOD, IGEN, SHDR];

struct ChunkHeader {
    id: [u8; 4],
    body: usize,
    size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Table {
    pub(crate) range: Range<usize>,
    pub(crate) count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Layout {
    pub(crate) file_bytes: u64,
    pub(crate) info_bytes: u64,
    pub(crate) sample_bytes: u64,
    pub(crate) extra_sample_bytes: u64,
    pub(crate) tables: [Table; 9],
}

#[derive(Default)]
struct Walk {
    info_bytes: u64,
    samples: Option<usize>,
    extra_samples: Option<usize>,
    tables: [Option<Range<usize>>; 9],
}

fn refuse(reason: UnsafeBank) -> BankError {
    BankError::Unsafe { reason }
}

fn invalid(reason: &str) -> BankError {
    BankError::Invalid {
        reason: reason.to_owned(),
    }
}

fn unsupported() -> BankError {
    BankError::Unsupported {
        message: SF3_UNSUPPORTED.to_owned(),
    }
}

fn widen(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn header<S: Source + ?Sized>(bytes: &S, at: usize, end: usize) -> Result<ChunkHeader, BankError> {
    let fields = at
        .checked_add(HEADER_BYTES)
        .filter(|fields| *fields <= end)
        .and_then(|fields| bytes.slice(at..fields))
        .ok_or(refuse(UnsafeBank::ChunkOverflow))?;
    let mut id = [0u8; 4];
    id.copy_from_slice(&fields[..4]);
    let mut size = [0u8; 4];
    size.copy_from_slice(&fields[4..]);
    let size =
        usize::try_from(u32::from_le_bytes(size)).map_err(|_| refuse(UnsafeBank::ChunkOverflow))?;
    let body = at + HEADER_BYTES;
    if size > end - body {
        return Err(refuse(UnsafeBank::ChunkOverflow));
    }
    Ok(ChunkHeader { id, body, size })
}

fn check_info<S: Source + ?Sized>(
    bytes: &S,
    chunk: &ChunkHeader,
    walk: &mut Walk,
) -> Result<(), BankError> {
    if &chunk.id == b"ifil" || &chunk.id == b"iver" {
        if chunk.size != VERSION_BYTES {
            return Err(refuse(UnsafeBank::VersionSize));
        }
        let version = bytes
            .slice(chunk.body..chunk.body + 2)
            .ok_or(refuse(UnsafeBank::ChunkOverflow))?;
        let major = u16::from_le_bytes([version[0], version[1]]);
        if &chunk.id == b"ifil" && major == SF3_MAJOR {
            return Err(unsupported());
        }
        return Ok(());
    }
    if !INFO_TEXTS.contains(&&chunk.id) {
        return Err(refuse(UnsafeBank::UnknownChunk));
    }
    walk.info_bytes = walk.info_bytes.saturating_add(widen(chunk.size));
    Ok(())
}

fn once(slot: &mut Option<usize>, size: usize) -> Result<(), BankError> {
    if slot.is_some() {
        return Err(refuse(UnsafeBank::DuplicateChunk));
    }
    *slot = Some(size);
    Ok(())
}

fn check_samples<S: Source + ?Sized>(
    bytes: &S,
    chunk: &ChunkHeader,
    walk: &mut Walk,
) -> Result<(), BankError> {
    match &chunk.id {
        b"smpl" => {
            if bytes.slice(chunk.body..chunk.body + chunk.size.min(OGG_MAGIC.len()))
                == Some(OGG_MAGIC)
            {
                return Err(unsupported());
            }
            if !chunk.size.is_multiple_of(2) || chunk.size < MIN_SAMPLE_BYTES {
                return Err(refuse(UnsafeBank::OddSamples));
            }
            once(&mut walk.samples, chunk.size)
        }
        b"sm24" => once(&mut walk.extra_samples, chunk.size),
        _ => Err(refuse(UnsafeBank::UnknownChunk)),
    }
}

fn check_table(chunk: &ChunkHeader, walk: &mut Walk) -> Result<(), BankError> {
    let index = TABLES
        .iter()
        .position(|table| *table.id == chunk.id)
        .ok_or(refuse(UnsafeBank::UnknownChunk))?;
    let slot = &mut walk.tables[index];
    if slot.is_some() {
        return Err(refuse(UnsafeBank::DuplicateChunk));
    }
    *slot = Some(chunk.body..chunk.body + chunk.size);
    Ok(())
}

fn check_list<S: Source + ?Sized>(
    bytes: &S,
    list: &ChunkHeader,
    expected: &[u8; 4],
    walk: &mut Walk,
) -> Result<(), BankError> {
    if list.size < FORM_BYTES {
        return Err(refuse(UnsafeBank::ListType));
    }
    if bytes.slice(list.body..list.body + FORM_BYTES) != Some(expected.as_slice()) {
        return Err(refuse(UnsafeBank::MissingChunk));
    }
    let end = list.body + list.size;
    let mut at = list.body + FORM_BYTES;
    while at < end {
        let chunk = header(bytes, at, end)?;
        match expected {
            b"INFO" => check_info(bytes, &chunk, walk)?,
            b"sdta" => check_samples(bytes, &chunk, walk)?,
            _ => check_table(&chunk, walk)?,
        }
        at = chunk.body + chunk.size;
    }
    Ok(())
}

fn table(range: Option<Range<usize>>, spec: &TableSpec) -> Result<Table, BankError> {
    let range = range.ok_or(refuse(UnsafeBank::MissingChunk))?;
    let size = range.len();
    if !size.is_multiple_of(spec.width) || size / spec.width < spec.minimum {
        return Err(refuse(UnsafeBank::MalformedTable));
    }
    Ok(Table {
        count: size / spec.width,
        range,
    })
}

pub(crate) fn check<S: Source + ?Sized>(
    bytes: &S,
    limits: &Sf2Limits,
) -> Result<Layout, BankError> {
    let file_bytes = widen(bytes.size());
    if file_bytes > limits.file_bytes {
        return Err(refuse(UnsafeBank::FileTooLarge {
            limit_mib: limits.file_bytes >> MIB_SHIFT,
        }));
    }
    if bytes.slice(0..FORM_BYTES) != Some(b"RIFF".as_slice())
        || bytes.slice(HEADER_BYTES..HEADER_BYTES + FORM_BYTES) != Some(b"sfbk".as_slice())
    {
        return Err(invalid("the file is not a SoundFont (no RIFF sfbk header)"));
    }
    let end = bytes.size();
    let mut at = HEADER_BYTES + FORM_BYTES;
    let mut walk = Walk::default();
    for expected in LISTS {
        let chunk = header(bytes, at, end)?;
        if &chunk.id != b"LIST" {
            return Err(refuse(UnsafeBank::MissingChunk));
        }
        check_list(bytes, &chunk, expected, &mut walk)?;
        at = chunk.body + chunk.size;
    }
    let sample_bytes = walk.samples.ok_or(refuse(UnsafeBank::MissingChunk))?;
    let [phdr, pbag, pmod, pgen, inst, ibag, imod, igen, shdr] = walk.tables;
    Ok(Layout {
        file_bytes,
        info_bytes: walk.info_bytes,
        sample_bytes: widen(sample_bytes),
        extra_sample_bytes: walk.extra_samples.map_or(0, widen),
        tables: [
            table(phdr, &PHDR)?,
            table(pbag, &PBAG)?,
            table(pmod, &PMOD)?,
            table(pgen, &PGEN)?,
            table(inst, &INST)?,
            table(ibag, &IBAG)?,
            table(imod, &IMOD)?,
            table(igen, &IGEN)?,
            table(shdr, &SHDR)?,
        ],
    })
}
