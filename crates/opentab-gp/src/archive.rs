use std::io::{Cursor, Read};

use opentab_model::{Base64Bytes, ContainerEntry};
use zip::ZipArchive;

use crate::error::{GpError, LimitProblem};
use crate::limits::{MAX_COMPRESSION_RATIO, MAX_ENTRY_SIZE, MAX_TOTAL_SIZE};

pub(crate) const SCORE_ENTRY: &str = "Content/score.gpif";
const PART_CONFIGURATION: &str = "Content/PartConfiguration";
const VERSION_ENTRY: &str = "VERSION";
const VERSION: &[u8] = b"7.0";

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
        let data = read_bounded(&mut file, &name, compressed, total)?;
        total = total.saturating_add(data.len() as u64);
        if name == SCORE_ENTRY {
            if score.is_some() {
                return Err(entry_error("duplicate archive entry".to_string()));
            }
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

fn read_bounded<R: Read>(
    file: &mut R,
    name: &str,
    compressed: u64,
    total: u64,
) -> Result<Vec<u8>, GpError> {
    let limit_error = |problem: LimitProblem| GpError::Limit {
        entry: name.to_string(),
        problem,
    };
    let ratio_cap = compressed
        .saturating_mul(MAX_COMPRESSION_RATIO)
        .max(MAX_COMPRESSION_RATIO);
    let cap = ratio_cap.min(MAX_ENTRY_SIZE);
    let mut data = Vec::new();
    let mut limited = file.take(cap.saturating_add(1));
    limited
        .read_to_end(&mut data)
        .map_err(|error| GpError::Entry {
            entry: name.to_string(),
            reason: error.to_string(),
        })?;
    let size = data.len() as u64;
    if size > MAX_ENTRY_SIZE {
        return Err(limit_error(LimitProblem::EntrySize {
            limit: MAX_ENTRY_SIZE,
        }));
    }
    if size > ratio_cap {
        return Err(limit_error(LimitProblem::CompressionRatio {
            limit: MAX_COMPRESSION_RATIO,
        }));
    }
    if total.saturating_add(size) > MAX_TOTAL_SIZE {
        return Err(limit_error(LimitProblem::TotalSize {
            limit: MAX_TOTAL_SIZE,
        }));
    }
    Ok(data)
}
