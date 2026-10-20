use std::fs::File;
use std::io::Read;
use std::path::Path;

use opentab_audio::{ArchiveKind, BankFormat, BankId, CatalogueEntry};
use serde::Deserialize;

use crate::error::CliError;
use crate::text::escape_controls;

const CATALOGUE_MAX_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum FormatName {
    Sf2,
    Sfz,
}

#[derive(Deserialize)]
enum ArchiveName {
    #[serde(rename = "raw")]
    Raw,
    #[serde(rename = "zip")]
    Zip,
    #[serde(rename = "7z")]
    SevenZip,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    id: String,
    name: String,
    format: FormatName,
    archive: ArchiveName,
    url: String,
    sha256: String,
    size: u64,
    licence_name: String,
    licence_url: String,
    licence_text: String,
    #[serde(default)]
    attribution: Option<String>,
    covers: String,
}

fn invalid(path: &Path, message: &str) -> CliError {
    CliError::InvalidInput {
        message: format!(
            "{}: not a catalogue file: {}",
            escape_controls(&path.display().to_string()),
            escape_controls(message)
        ),
    }
}

pub fn read(path: &Path) -> Result<Vec<CatalogueEntry>, CliError> {
    let io = |error: std::io::Error| CliError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    };
    let mut text = String::new();
    File::open(path)
        .map_err(io)?
        .take(CATALOGUE_MAX_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(io)?;
    if text.len() as u64 > CATALOGUE_MAX_BYTES {
        return Err(invalid(path, "the file is larger than 4 MiB"));
    }
    let records: Vec<Record> =
        serde_json::from_str(&text).map_err(|error| invalid(path, &error.to_string()))?;
    records
        .into_iter()
        .map(|record| entry(path, record))
        .collect()
}

fn entry(path: &Path, record: Record) -> Result<CatalogueEntry, CliError> {
    let id = BankId::new(&record.id).map_err(|error| invalid(path, &error.to_string()))?;
    Ok(CatalogueEntry {
        id,
        name: record.name,
        format: match record.format {
            FormatName::Sf2 => BankFormat::Sf2,
            FormatName::Sfz => BankFormat::Sfz,
        },
        archive: match record.archive {
            ArchiveName::Raw => ArchiveKind::Raw,
            ArchiveName::Zip => ArchiveKind::Zip,
            ArchiveName::SevenZip => ArchiveKind::SevenZip,
        },
        url: record.url,
        sha256: record.sha256,
        size: record.size,
        licence_name: record.licence_name,
        licence_url: record.licence_url,
        licence_text: record.licence_text,
        attribution: record.attribution,
        covers: record.covers,
    })
}
