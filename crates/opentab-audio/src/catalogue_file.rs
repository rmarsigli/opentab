use std::ffi::OsString;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{ArchiveKind, BankFormat, BankId, CatalogueEntry};

pub const CATALOGUE_VARIABLE: &str = "OPENTAB_CATALOGUE";
pub const CATALOGUE_MAX_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CatalogueFileError {
    #[error("{}: {message}", path.display())]
    Io { path: PathBuf, message: String },
    #[error("{}: not a catalogue file: {message}", path.display())]
    Invalid { path: PathBuf, message: String },
}

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

fn invalid(path: &Path, message: impl Into<String>) -> CatalogueFileError {
    CatalogueFileError::Invalid {
        path: path.to_path_buf(),
        message: message.into(),
    }
}

fn override_from(value: Option<OsString>) -> Option<PathBuf> {
    value.filter(|value| !value.is_empty()).map(PathBuf::from)
}

pub fn catalogue_override() -> Option<PathBuf> {
    override_from(std::env::var_os(CATALOGUE_VARIABLE))
}

pub fn catalogue_from_file(path: &Path) -> Result<Vec<CatalogueEntry>, CatalogueFileError> {
    let io = |error: std::io::Error| CatalogueFileError::Io {
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
        serde_json::from_str(&text).map_err(|error| invalid(path, error.to_string()))?;
    records
        .into_iter()
        .map(|record| entry(path, record))
        .collect()
}

fn entry(path: &Path, record: Record) -> Result<CatalogueEntry, CatalogueFileError> {
    let id = BankId::new(&record.id).map_err(|error| invalid(path, error.to_string()))?;
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

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn record(id: &str, format: &str, archive: &str) -> Value {
        json!({
            "id": id,
            "name": "Test",
            "format": format,
            "archive": archive,
            "url": "http://127.0.0.1:9/bank.sf2",
            "sha256": "00",
            "size": 12,
            "licence_name": "CC0-1.0",
            "licence_url": "https://example.org/licence",
            "licence_text": "No rights reserved.",
            "covers": "tests"
        })
    }

    fn written(dir: &Path, contents: &[u8]) -> PathBuf {
        let path = dir.join("catalogue.json");
        std::fs::write(&path, contents).expect("write");
        path
    }

    #[test]
    fn a_catalogue_file_reads_every_field_and_format() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut zipped = record("zipped", "sfz", "zip");
        zipped["attribution"] = json!("Someone");
        let records = json!([
            record("raw-one", "sf2", "raw"),
            zipped,
            record("seven", "sf2", "7z")
        ]);
        let path = written(dir.path(), records.to_string().as_bytes());
        let entries = catalogue_from_file(&path).expect("reads");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].id.to_string(), "raw-one");
        assert_eq!(entries[0].format, BankFormat::Sf2);
        assert_eq!(entries[0].archive, ArchiveKind::Raw);
        assert_eq!(entries[0].url, "http://127.0.0.1:9/bank.sf2");
        assert_eq!(entries[0].size, 12);
        assert_eq!(entries[0].attribution, None);
        assert_eq!(entries[1].format, BankFormat::Sfz);
        assert_eq!(entries[1].archive, ArchiveKind::Zip);
        assert_eq!(entries[1].attribution.as_deref(), Some("Someone"));
        assert_eq!(entries[2].archive, ArchiveKind::SevenZip);
        assert_eq!(entries[2].licence_text, "No rights reserved.");
        assert_eq!(entries[2].covers, "tests");
    }

    #[test]
    fn a_broken_or_missing_catalogue_file_is_refused_with_its_kind() {
        let dir = tempfile::tempdir().expect("tempdir");
        for contents in [
            b"{\"not\": \"a list\"}".to_vec(),
            json!([record("Bad_Id", "sf2", "raw")])
                .to_string()
                .into_bytes(),
            json!([record("good", "sf3", "raw")])
                .to_string()
                .into_bytes(),
            json!([{"id": "x"}]).to_string().into_bytes(),
            vec![b' '; usize::try_from(CATALOGUE_MAX_BYTES).expect("size") + 1],
        ] {
            let path = written(dir.path(), &contents);
            let error = catalogue_from_file(&path).expect_err("refused");
            assert!(
                matches!(error, CatalogueFileError::Invalid { .. }),
                "{error}"
            );
            assert!(
                error.to_string().contains("not a catalogue file"),
                "{error}"
            );
        }
        let error = catalogue_from_file(&dir.path().join("gone.json")).expect_err("missing");
        assert!(matches!(error, CatalogueFileError::Io { .. }), "{error}");
        assert!(error.to_string().contains("gone.json"), "{error}");
    }

    #[test]
    fn the_override_needs_a_non_empty_value() {
        assert_eq!(override_from(None), None);
        assert_eq!(override_from(Some(OsString::new())), None);
        assert_eq!(
            override_from(Some(OsString::from("/tmp/catalogue.json"))),
            Some(PathBuf::from("/tmp/catalogue.json"))
        );
    }
}
