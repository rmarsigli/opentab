use std::ops::Range;

use crate::error::SF3_UNSUPPORTED;
use crate::{BankError, Sf2Limits, UnsafeBank};

const HEADER_BYTES: usize = 8;
const FORM_BYTES: usize = 4;
const VERSION_BYTES: usize = 4;
const SF3_MAJOR: u16 = 3;
const OGG_MAGIC: &[u8; 4] = b"OggS";
const MIN_SAMPLE_BYTES: usize = 4;
const TOP_LISTS: usize = 3;

struct ChunkHeader {
    id: [u8; 4],
    body: usize,
    size: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PdtaChunks {
    pub(crate) phdr: Option<Range<usize>>,
    pub(crate) pbag: Option<Range<usize>>,
    pub(crate) pmod: Option<Range<usize>>,
    pub(crate) pgen: Option<Range<usize>>,
    pub(crate) inst: Option<Range<usize>>,
    pub(crate) ibag: Option<Range<usize>>,
    pub(crate) imod: Option<Range<usize>>,
    pub(crate) igen: Option<Range<usize>>,
}

impl PdtaChunks {
    fn record(&mut self, id: &[u8; 4], body: Range<usize>) {
        let slot = match id {
            b"phdr" => &mut self.phdr,
            b"pbag" => &mut self.pbag,
            b"pmod" => &mut self.pmod,
            b"pgen" => &mut self.pgen,
            b"inst" => &mut self.inst,
            b"ibag" => &mut self.ibag,
            b"imod" => &mut self.imod,
            b"igen" => &mut self.igen,
            _ => return,
        };
        *slot = Some(body);
    }
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

fn header(bytes: &[u8], at: usize, end: usize) -> Result<ChunkHeader, BankError> {
    let fields = at
        .checked_add(HEADER_BYTES)
        .filter(|fields| *fields <= end)
        .and_then(|fields| bytes.get(at..fields))
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

fn check_info(bytes: &[u8], chunk: &ChunkHeader) -> Result<(), BankError> {
    if &chunk.id != b"ifil" && &chunk.id != b"iver" {
        return Ok(());
    }
    if chunk.size != VERSION_BYTES {
        return Err(refuse(UnsafeBank::VersionSize));
    }
    if &chunk.id == b"iver" {
        return Ok(());
    }
    let major = u16::from_le_bytes([bytes[chunk.body], bytes[chunk.body + 1]]);
    if major == SF3_MAJOR {
        return Err(unsupported());
    }
    Ok(())
}

fn check_samples(bytes: &[u8], chunk: &ChunkHeader) -> Result<(), BankError> {
    if &chunk.id != b"smpl" {
        return Ok(());
    }
    if bytes.get(chunk.body..chunk.body + chunk.size.min(OGG_MAGIC.len())) == Some(OGG_MAGIC) {
        return Err(unsupported());
    }
    if !chunk.size.is_multiple_of(2) || chunk.size < MIN_SAMPLE_BYTES {
        return Err(refuse(UnsafeBank::OddSamples));
    }
    Ok(())
}

fn check_list(bytes: &[u8], list: &ChunkHeader, pdta: &mut PdtaChunks) -> Result<(), BankError> {
    if list.size < FORM_BYTES {
        return Err(refuse(UnsafeBank::ListType));
    }
    let kind = &bytes[list.body..list.body + FORM_BYTES];
    let end = list.body + list.size;
    let mut at = list.body + FORM_BYTES;
    while at < end {
        let chunk = header(bytes, at, end)?;
        match kind {
            b"INFO" => check_info(bytes, &chunk)?,
            b"sdta" => check_samples(bytes, &chunk)?,
            b"pdta" => pdta.record(&chunk.id, chunk.body..chunk.body + chunk.size),
            _ => {}
        }
        at = chunk.body + chunk.size;
    }
    Ok(())
}

pub(crate) fn check(bytes: &[u8], limits: &Sf2Limits) -> Result<PdtaChunks, BankError> {
    if u64::try_from(bytes.len()).map_or(true, |length| length > limits.file_bytes) {
        return Err(refuse(UnsafeBank::FileTooLarge));
    }
    if bytes.get(..FORM_BYTES) != Some(b"RIFF")
        || bytes.get(HEADER_BYTES..HEADER_BYTES + FORM_BYTES) != Some(b"sfbk")
    {
        return Err(invalid("the file is not a SoundFont (no RIFF sfbk header)"));
    }
    let end = bytes.len();
    let mut at = HEADER_BYTES + FORM_BYTES;
    let mut pdta = PdtaChunks::default();
    for _ in 0..TOP_LISTS {
        let chunk = header(bytes, at, end)?;
        if &chunk.id != b"LIST" {
            return Err(invalid("a SoundFont section is missing"));
        }
        check_list(bytes, &chunk, &mut pdta)?;
        at = chunk.body + chunk.size;
    }
    Ok(pdta)
}
