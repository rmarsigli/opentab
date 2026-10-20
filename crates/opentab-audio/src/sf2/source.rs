use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::ops::Range;

use crate::{BankError, Sf2Limits, UnsafeBank};

const HEADER_BYTES: u64 = 8;
const FORM_BYTES: u64 = 4;
const RIFF_PREFIX: u64 = 12;
const TOP_LISTS: usize = 3;
const SAMPLE_CHUNKS: usize = 3;
const MAGIC_BYTES: u64 = 4;
const MIB_SHIFT: u32 = 20;
const CHANGED_SINCE_CHECK: &str = "a checked part of the SoundFont changed after it was checked";

pub(crate) trait Source {
    fn size(&self) -> usize;
    fn slice(&self, range: Range<usize>) -> Option<&[u8]>;
}

impl Source for [u8] {
    fn size(&self) -> usize {
        self.len()
    }

    fn slice(&self, range: Range<usize>) -> Option<&[u8]> {
        self.get(range)
    }
}

struct Segment {
    start: usize,
    bytes: Vec<u8>,
}

pub(crate) struct Sparse {
    size: usize,
    segments: Vec<Segment>,
}

impl Source for Sparse {
    fn size(&self) -> usize {
        self.size
    }

    fn slice(&self, range: Range<usize>) -> Option<&[u8]> {
        self.segments.iter().find_map(|segment| {
            let from = range.start.checked_sub(segment.start)?;
            let to = range.end.checked_sub(segment.start)?;
            segment.bytes.get(from..to)
        })
    }
}

fn io_invalid(error: &std::io::Error) -> BankError {
    BankError::Invalid {
        reason: format!("the file cannot be read: {error}"),
    }
}

struct RawHeader {
    id: [u8; 4],
    size: u64,
}

struct Reader<'a> {
    file: &'a mut File,
    size: u64,
    segments: Vec<Segment>,
}

impl Reader<'_> {
    fn load(&mut self, start: u64, length: u64) -> Result<Option<Vec<u8>>, BankError> {
        let Some(end) = start.checked_add(length).filter(|end| *end <= self.size) else {
            return Ok(None);
        };
        let length = usize::try_from(end - start).map_err(|_| refuse_overflow())?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(length)
            .map_err(|_| refuse_overflow())?;
        self.file
            .seek(SeekFrom::Start(start))
            .map_err(|error| io_invalid(&error))?;
        self.file
            .by_ref()
            .take(u64::try_from(length).unwrap_or(u64::MAX))
            .read_to_end(&mut bytes)
            .map_err(|error| io_invalid(&error))?;
        if bytes.len() != length {
            return Ok(None);
        }
        Ok(Some(bytes))
    }

    fn keep(&mut self, start: u64, length: u64) -> Result<Option<usize>, BankError> {
        let Some(bytes) = self.load(start, length)? else {
            return Ok(None);
        };
        let at = usize::try_from(start).map_err(|_| refuse_overflow())?;
        let index = self.segments.len();
        self.segments.push(Segment { start: at, bytes });
        Ok(Some(index))
    }

    fn header(&mut self, at: u64) -> Result<Option<RawHeader>, BankError> {
        let Some(index) = self.keep(at, HEADER_BYTES)? else {
            return Ok(None);
        };
        let bytes = &self.segments[index].bytes;
        let mut id = [0u8; 4];
        id.copy_from_slice(&bytes[..4]);
        let size = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        Ok(Some(RawHeader {
            id,
            size: u64::from(size),
        }))
    }

    fn samples(&mut self, body: u64, end: u64) -> Result<(), BankError> {
        let mut at = body;
        for _ in 0..SAMPLE_CHUNKS {
            if at.saturating_add(HEADER_BYTES) > end {
                break;
            }
            let Some(chunk) = self.header(at)? else {
                break;
            };
            if &chunk.id == b"smpl" {
                self.keep(at + HEADER_BYTES, chunk.size.min(MAGIC_BYTES))?;
            }
            at = at.saturating_add(HEADER_BYTES).saturating_add(chunk.size);
        }
        Ok(())
    }
}

fn refuse_overflow() -> BankError {
    BankError::Unsafe {
        reason: UnsafeBank::ChunkOverflow,
    }
}

pub(crate) fn sparse(file: &mut File, limits: &Sf2Limits) -> Result<Sparse, BankError> {
    let size = file.metadata().map_err(|error| io_invalid(&error))?.len();
    if size > limits.file_bytes {
        return Err(BankError::Unsafe {
            reason: UnsafeBank::FileTooLarge {
                limit_mib: limits.file_bytes >> MIB_SHIFT,
            },
        });
    }
    let total = usize::try_from(size).map_err(|_| refuse_overflow())?;
    let mut reader = Reader {
        file,
        size,
        segments: Vec::new(),
    };
    reader.keep(0, RIFF_PREFIX.min(size))?;
    let mut at = RIFF_PREFIX;
    for _ in 0..TOP_LISTS {
        let Some(list) = reader.header(at)? else {
            break;
        };
        let list_size = list.size;
        let body = at + HEADER_BYTES;
        let end = body.saturating_add(list_size).min(size);
        let Some(form) = reader.load(body, FORM_BYTES)? else {
            break;
        };
        if &form == b"sdta" {
            reader.keep(body, FORM_BYTES)?;
            reader.samples(body + FORM_BYTES, end)?;
        } else if &form == b"INFO" || &form == b"pdta" {
            reader.keep(body, end - body)?;
        } else {
            reader.keep(body, FORM_BYTES)?;
        }
        at = body.saturating_add(list_size);
    }
    Ok(Sparse {
        size: total,
        segments: reader.segments,
    })
}

pub(crate) struct Verified<'a, R> {
    inner: R,
    checked: &'a Sparse,
    at: usize,
    changed: bool,
}

impl<'a, R: Read> Verified<'a, R> {
    pub(crate) fn new(inner: R, checked: &'a Sparse) -> Verified<'a, R> {
        Verified {
            inner,
            checked,
            at: 0,
            changed: false,
        }
    }

    pub(crate) fn changed(&self) -> bool {
        self.changed
    }
}

impl<R: Read> Read for Verified<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let read = self.inner.read(buffer)?;
        let end = self.at.saturating_add(read);
        for segment in &self.checked.segments {
            let from = self.at.max(segment.start);
            let to = end.min(segment.start.saturating_add(segment.bytes.len()));
            if from < to
                && buffer[from - self.at..to - self.at]
                    != segment.bytes[from - segment.start..to - segment.start]
            {
                self.changed = true;
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    CHANGED_SINCE_CHECK,
                ));
            }
        }
        self.at = end;
        Ok(read)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn sparse_of(bytes: &[u8]) -> Sparse {
        let mut file = tempfile::tempfile().expect("scratch");
        file.write_all(bytes).expect("bank");
        sparse(&mut file, &Sf2Limits::DEFAULT).expect("sparse")
    }

    fn read_all(bytes: &[u8], checked: &Sparse) -> bool {
        let mut verified = Verified::new(io::Cursor::new(bytes.to_vec()), checked);
        let mut sink = [0u8; 7];
        while let Ok(read) = verified.read(&mut sink) {
            if read == 0 {
                break;
            }
        }
        verified.changed()
    }

    fn offset_of(bytes: &[u8], tag: &[u8]) -> usize {
        bytes
            .windows(tag.len())
            .position(|window| window == tag)
            .expect("the tag is present")
    }

    #[test]
    fn a_change_to_a_checked_part_is_caught_and_sample_data_is_not_checked() {
        let original = opentab_testkit::banks::tiny_sf2();
        let checked = sparse_of(&original);
        assert!(!read_all(&original, &checked));
        let mut preset = original.clone();
        let phdr = offset_of(&preset, b"phdr") + 8;
        preset[phdr] ^= 0x20;
        assert!(read_all(&preset, &checked));
        let mut sample = original.clone();
        let smpl = offset_of(&sample, b"smpl") + 8 + MAGIC_BYTES as usize + 10;
        sample[smpl] ^= 0x01;
        assert!(!read_all(&sample, &checked));
        let mut magic = original;
        let first = offset_of(&magic, b"smpl") + 8;
        magic[first] ^= 0x01;
        assert!(read_all(&magic, &checked));
    }
}
