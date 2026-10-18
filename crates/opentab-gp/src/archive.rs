use std::collections::HashSet;
use std::io::{Cursor, Read, Seek};

use opentab_model::{Base64Bytes, ContainerEntry};
use zip::ZipArchive;

use crate::error::{GpError, LimitProblem};
use crate::limits::{MAX_COMPRESSION_RATIO, MAX_ENTRY_SIZE, MAX_TOTAL_SIZE};

pub(crate) const SCORE_ENTRY: &str = "Content/score.gpif";
const PART_CONFIGURATION: &str = "Content/PartConfiguration";
const VERSION_ENTRY: &str = "VERSION";
const VERSION: &[u8] = b"7.0";
const CENTRAL_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];
const CENTRAL_HEADER_SIZE: usize = 46;
const NAME_LENGTH_AT: usize = 28;
const EXTRA_LENGTH_AT: usize = 30;
const COMMENT_LENGTH_AT: usize = 32;

pub(crate) fn minimal_container() -> Vec<ContainerEntry> {
    vec![
        ContainerEntry::File {
            name: PART_CONFIGURATION.to_string(),
            data: Base64Bytes(Vec::new()),
        },
        ContainerEntry::Score {
            name: SCORE_ENTRY.to_string(),
        },
        ContainerEntry::File {
            name: VERSION_ENTRY.to_string(),
            data: Base64Bytes(VERSION.to_vec()),
        },
    ]
}

pub(crate) struct Archive {
    pub entries: Vec<ContainerEntry>,
    pub score: Vec<u8>,
}

pub(crate) fn read_archive(bytes: &[u8]) -> Result<Archive, GpError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|error| GpError::Archive {
        reason: error.to_string(),
    })?;
    reject_duplicate_names(bytes, &archive)?;
    let mut entries = Vec::with_capacity(archive.len().min(4096));
    let mut score = None;
    let mut total: u64 = 0;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|error| GpError::Archive {
            reason: error.to_string(),
        })?;
        let name = file.name().to_string();
        let entry_error = |reason: String| GpError::Entry {
            entry: name.clone(),
            reason,
        };
        if file.encrypted() {
            return Err(entry_error(
                "encrypted entries are not supported".to_string(),
            ));
        }
        let compressed = file.compressed_size();
        admit(&name, file.size(), compressed, total)?;
        let data = read_bounded(&mut file, &name, compressed, total)?;
        total = total.saturating_add(data.len() as u64);
        if name == SCORE_ENTRY {
            score = Some(data);
            entries.push(ContainerEntry::Score { name });
        } else {
            entries.push(ContainerEntry::File {
                name,
                data: Base64Bytes(data),
            });
        }
    }
    let score = score.ok_or(GpError::MissingScore)?;
    Ok(Archive { entries, score })
}

fn u16_at(bytes: &[u8], at: usize) -> usize {
    usize::from(u16::from_le_bytes([bytes[at], bytes[at + 1]]))
}

fn central_records(bytes: &[u8], start: usize) -> Vec<&[u8]> {
    let mut names = Vec::new();
    let mut at = start;
    while let Some(header) = bytes.get(at..at.saturating_add(CENTRAL_HEADER_SIZE)) {
        if header[..CENTRAL_SIGNATURE.len()] != CENTRAL_SIGNATURE {
            break;
        }
        let name_length = u16_at(header, NAME_LENGTH_AT);
        let extra_length = u16_at(header, EXTRA_LENGTH_AT);
        let comment_length = u16_at(header, COMMENT_LENGTH_AT);
        let name_start = at + CENTRAL_HEADER_SIZE;
        let Some(name) = bytes.get(name_start..name_start.saturating_add(name_length)) else {
            break;
        };
        names.push(name);
        at = name_start + name_length + extra_length + comment_length;
    }
    names
}

fn reject_duplicate_names<R: Read + Seek>(
    bytes: &[u8],
    archive: &ZipArchive<R>,
) -> Result<(), GpError> {
    let Ok(start) = usize::try_from(archive.central_directory_start()) else {
        return Ok(());
    };
    let mut seen = HashSet::new();
    for name in central_records(bytes, start) {
        if !seen.insert(name) {
            return Err(GpError::DuplicateEntry {
                name: String::from_utf8_lossy(name).into_owned(),
            });
        }
    }
    Ok(())
}

fn ratio_cap(compressed: u64) -> u64 {
    compressed
        .saturating_mul(MAX_COMPRESSION_RATIO)
        .max(MAX_COMPRESSION_RATIO)
}

fn admit(name: &str, size: u64, compressed: u64, total: u64) -> Result<(), GpError> {
    let limit_error = |problem: LimitProblem| GpError::Limit {
        entry: name.to_string(),
        problem,
    };
    if size > MAX_ENTRY_SIZE {
        return Err(limit_error(LimitProblem::EntrySize {
            limit: MAX_ENTRY_SIZE,
        }));
    }
    if size > ratio_cap(compressed) {
        return Err(limit_error(LimitProblem::CompressionRatio {
            limit: MAX_COMPRESSION_RATIO,
        }));
    }
    if total.saturating_add(size) > MAX_TOTAL_SIZE {
        return Err(limit_error(LimitProblem::TotalSize {
            limit: MAX_TOTAL_SIZE,
        }));
    }
    Ok(())
}

fn read_bounded<R: Read>(
    file: &mut R,
    name: &str,
    compressed: u64,
    total: u64,
) -> Result<Vec<u8>, GpError> {
    let cap = ratio_cap(compressed).min(MAX_ENTRY_SIZE);
    let mut data = Vec::new();
    let mut limited = file.take(cap.saturating_add(1));
    limited
        .read_to_end(&mut data)
        .map_err(|error| GpError::Entry {
            entry: name.to_string(),
            reason: error.to_string(),
        })?;
    admit(name, data.len() as u64, compressed, total)?;
    Ok(data)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, repeat};

    use super::*;

    fn problem(result: Result<(), GpError>) -> Option<LimitProblem> {
        match result {
            Ok(()) => None,
            Err(GpError::Limit { problem, .. }) => Some(problem),
            Err(other) => panic!("not a limit error: {other}"),
        }
    }

    #[test]
    fn entry_size_is_inclusive_at_the_limit() {
        let compressed = MAX_ENTRY_SIZE;
        assert_eq!(problem(admit("e", MAX_ENTRY_SIZE, compressed, 0)), None);
        assert_eq!(
            problem(admit("e", MAX_ENTRY_SIZE + 1, compressed, 0)),
            Some(LimitProblem::EntrySize {
                limit: MAX_ENTRY_SIZE
            })
        );
    }

    #[test]
    fn ratio_is_inclusive_at_the_limit() {
        for compressed in [1, 7, 1000, 300_000] {
            let cap = compressed * MAX_COMPRESSION_RATIO;
            assert_eq!(problem(admit("e", cap, compressed, 0)), None);
            assert_eq!(
                problem(admit("e", cap + 1, compressed, 0)),
                Some(LimitProblem::CompressionRatio {
                    limit: MAX_COMPRESSION_RATIO
                })
            );
        }
        assert_eq!(problem(admit("e", MAX_COMPRESSION_RATIO, 0, 0)), None);
        assert_eq!(
            problem(admit("e", MAX_COMPRESSION_RATIO + 1, 0, 0)),
            Some(LimitProblem::CompressionRatio {
                limit: MAX_COMPRESSION_RATIO
            })
        );
    }

    #[test]
    fn total_size_is_inclusive_at_the_limit() {
        let compressed = MAX_ENTRY_SIZE;
        for size in [0, 1, 4096, MAX_ENTRY_SIZE] {
            assert_eq!(
                problem(admit("e", size, compressed, MAX_TOTAL_SIZE - size)),
                None
            );
            assert_eq!(
                problem(admit("e", size, compressed, MAX_TOTAL_SIZE - size + 1)),
                Some(LimitProblem::TotalSize {
                    limit: MAX_TOTAL_SIZE
                })
            );
        }
    }

    #[test]
    fn totals_that_overflow_are_a_limit_error() {
        assert_eq!(
            problem(admit("e", 1, 1, u64::MAX)),
            Some(LimitProblem::TotalSize {
                limit: MAX_TOTAL_SIZE
            })
        );
    }

    #[test]
    fn bounded_reads_stop_one_byte_past_the_ratio_cap() {
        for compressed in [0u64, 1, 10, 3000] {
            let cap = ratio_cap(compressed);
            let at = read_bounded(&mut repeat(0).take(cap), "e", compressed, 0).map(|d| d.len());
            assert_eq!(at.ok(), Some(usize::try_from(cap).expect("cap")));
            let mut over = repeat(0).take(cap + 10);
            let result = read_bounded(&mut over, "e", compressed, 0);
            assert!(matches!(
                result,
                Err(GpError::Limit {
                    problem: LimitProblem::CompressionRatio { .. },
                    ..
                })
            ));
            assert_eq!(over.limit(), 9);
        }
    }

    #[test]
    fn bounded_reads_stop_one_byte_past_the_entry_cap() {
        let compressed = MAX_ENTRY_SIZE;
        let cap = usize::try_from(MAX_ENTRY_SIZE).expect("cap");
        let at = read_bounded(&mut Cursor::new(vec![0; cap]), "e", compressed, 0);
        assert_eq!(at.map(|data| data.len()).ok(), Some(cap));
        let over = read_bounded(&mut Cursor::new(vec![0; cap + 1]), "e", compressed, 0);
        assert!(matches!(
            over,
            Err(GpError::Limit {
                problem: LimitProblem::EntrySize { .. },
                ..
            })
        ));
    }

    #[test]
    fn bounded_reads_enforce_the_running_total() {
        let compressed = 1000;
        let at = read_bounded(
            &mut repeat(0).take(500),
            "e",
            compressed,
            MAX_TOTAL_SIZE - 500,
        );
        assert_eq!(at.map(|data| data.len()).ok(), Some(500));
        let over = read_bounded(
            &mut repeat(0).take(500),
            "e",
            compressed,
            MAX_TOTAL_SIZE - 499,
        );
        assert!(matches!(
            over,
            Err(GpError::Limit {
                problem: LimitProblem::TotalSize { .. },
                ..
            })
        ));
    }

    #[test]
    fn central_records_list_names_in_order() {
        let mut bytes = Vec::new();
        for name in ["a", "bc"] {
            bytes.extend_from_slice(&CENTRAL_SIGNATURE);
            bytes.extend_from_slice(&[0; CENTRAL_HEADER_SIZE - 4]);
            let at = bytes.len() - (CENTRAL_HEADER_SIZE - 4);
            bytes[at + NAME_LENGTH_AT - 4] = u8::try_from(name.len()).expect("length");
            bytes.extend_from_slice(name.as_bytes());
        }
        assert_eq!(central_records(&bytes, 0), vec![b"a".as_slice(), b"bc"]);
        assert!(central_records(&bytes[..CENTRAL_HEADER_SIZE], 0).is_empty());
        assert!(central_records(&bytes, bytes.len() + 5).is_empty());
    }

    fn record(name: &str, extra: usize, comment: usize) -> Vec<u8> {
        let mut bytes = CENTRAL_SIGNATURE.to_vec();
        bytes.resize(CENTRAL_HEADER_SIZE, 0);
        bytes[NAME_LENGTH_AT] = u8::try_from(name.len()).expect("length");
        bytes[EXTRA_LENGTH_AT] = u8::try_from(extra).expect("length");
        bytes[COMMENT_LENGTH_AT] = u8::try_from(comment).expect("length");
        bytes.extend_from_slice(name.as_bytes());
        bytes.resize(bytes.len() + extra + comment, 0xEE);
        bytes
    }

    #[test]
    fn central_records_skip_extra_fields_and_comments() {
        let mut bytes = Vec::new();
        for (name, extra, comment) in [("abc", 5, 7), ("de", 0, 3), ("fghi", 4, 0), ("j", 2, 9)] {
            bytes.extend_from_slice(&record(name, extra, comment));
        }
        assert_eq!(
            central_records(&bytes, 0),
            vec![b"abc".as_slice(), b"de", b"fghi", b"j"]
        );
    }

    #[test]
    fn little_endian_words_read_both_bytes_at_the_offset() {
        let bytes = [0x01, 0x02, 0x03, 0x04];
        assert_eq!(u16_at(&bytes, 0), 0x0201);
        assert_eq!(u16_at(&bytes, 1), 0x0302);
        assert_eq!(u16_at(&bytes, 2), 0x0403);
    }
}
