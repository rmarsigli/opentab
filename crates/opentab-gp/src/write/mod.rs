mod beat;
mod document;
mod note;
mod payload;
mod state;
mod track;
mod xml;

use std::collections::BTreeSet;
use std::io::{Cursor, Write};

use opentab_model::{ContainerEntry, Score};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use crate::archive::{SCORE_ENTRY, minimal_container};
use crate::error::{GpError, Refusal};

const DIRECTORY_SUFFIX: char = '/';

pub fn write_gp(score: &Score) -> Result<Vec<u8>, GpError> {
    let entries = container(score)?;
    let gpif = document::write_gpif(score).map_err(xml::Fail::into_error)?;
    write_archive(&entries, gpif.as_bytes())
}

fn refuse(index: usize, reason: Refusal) -> GpError {
    GpError::Unsupported {
        path: format!("container[{index}]"),
        reason,
    }
}

fn container(score: &Score) -> Result<Vec<ContainerEntry>, GpError> {
    if score.container.is_empty() {
        return Ok(minimal_container());
    }
    let count = score
        .container
        .iter()
        .filter(|entry| entry.is_score())
        .count();
    if count != 1 {
        return Err(GpError::Unsupported {
            path: "container".to_string(),
            reason: Refusal::ScoreEntries { count },
        });
    }
    let mut names = BTreeSet::new();
    for (index, entry) in score.container.iter().enumerate() {
        let name = entry.name();
        match entry {
            ContainerEntry::Score { .. } if name != SCORE_ENTRY => {
                return Err(refuse(
                    index,
                    Refusal::ScoreEntryName {
                        name: name.to_string(),
                    },
                ));
            }
            ContainerEntry::File { data, .. } => {
                if name.is_empty() || name == SCORE_ENTRY {
                    return Err(refuse(
                        index,
                        Refusal::EntryName {
                            name: name.to_string(),
                        },
                    ));
                }
                if name.ends_with(DIRECTORY_SUFFIX) && !data.0.is_empty() {
                    return Err(refuse(
                        index,
                        Refusal::DirectoryData {
                            name: name.to_string(),
                        },
                    ));
                }
            }
            ContainerEntry::Score { .. } => {}
        }
        if !names.insert(name) {
            return Err(refuse(
                index,
                Refusal::DuplicateEntry {
                    name: name.to_string(),
                },
            ));
        }
    }
    Ok(score.container.clone())
}

fn write_failure(error: impl std::fmt::Display) -> GpError {
    GpError::Write {
        reason: error.to_string(),
    }
}

fn write_archive(entries: &[ContainerEntry], gpif: &[u8]) -> Result<Vec<u8>, GpError> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let deflated = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default());
    let stored = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(DateTime::default());
    for (index, entry) in entries.iter().enumerate() {
        let name = entry.name();
        let rejected = |_| {
            refuse(
                index,
                Refusal::EntryName {
                    name: name.to_string(),
                },
            )
        };
        let data: &[u8] = match entry {
            ContainerEntry::Score { .. } => gpif,
            ContainerEntry::File { data, .. } => &data.0,
        };
        if entry.is_score() || !name.ends_with(DIRECTORY_SUFFIX) {
            writer.start_file(name, deflated).map_err(rejected)?;
            writer.write_all(data).map_err(write_failure)?;
        } else {
            writer.add_directory(name, stored).map_err(rejected)?;
        }
    }
    let bytes = writer.finish().map_err(write_failure)?.into_inner();
    check_names(entries, &bytes)?;
    Ok(bytes)
}

fn check_names(entries: &[ContainerEntry], bytes: &[u8]) -> Result<(), GpError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(write_failure)?;
    if archive.len() != entries.len() {
        return Err(write_failure("the archive lost entries"));
    }
    for (index, entry) in entries.iter().enumerate() {
        let file = archive.by_index(index).map_err(write_failure)?;
        if file.name() != entry.name() {
            return Err(refuse(
                index,
                Refusal::EntryName {
                    name: entry.name().to_string(),
                },
            ));
        }
    }
    Ok(())
}
