use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use opentab_model::{InstrumentFamily, MidiBank};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::manager::copy::storage;
use crate::manager::id::{BankId, BankRef};
use crate::manager::limits::STORE_MAX_BYTES;
use crate::{BankError, BankFormat, BankInstrument};

pub const STORE_VERSION: u32 = 1;
pub const REGISTRY_NAME: &str = "banks.json";
pub const PREFERENCES_NAME: &str = "preferences.json";
const TEMPORARY_SUFFIX: &str = ".tmp";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StoredFormat {
    Sf2,
    Sfz,
}

impl StoredFormat {
    pub(crate) fn of(format: BankFormat) -> Option<StoredFormat> {
        match format {
            BankFormat::Sf2 => Some(StoredFormat::Sf2),
            BankFormat::Sfz => Some(StoredFormat::Sfz),
            BankFormat::BuiltIn => None,
        }
    }

    pub(crate) fn format(self) -> BankFormat {
        match self {
            StoredFormat::Sf2 => BankFormat::Sf2,
            StoredFormat::Sfz => BankFormat::Sfz,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BankSource {
    Imported,
    Catalogue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredInstrument {
    pub(crate) msb: u8,
    pub(crate) lsb: u8,
    pub(crate) program: u8,
    pub(crate) name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredBank {
    pub(crate) id: BankId,
    pub(crate) name: String,
    pub(crate) format: StoredFormat,
    pub(crate) path: String,
    pub(crate) bytes: u64,
    pub(crate) source: BankSource,
    pub(crate) instruments: Vec<StoredInstrument>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Registry {
    pub(crate) version: u32,
    pub(crate) banks: Vec<StoredBank>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Preferences {
    pub(crate) version: u32,
    pub(crate) families: BTreeMap<InstrumentFamily, BankRef>,
}

impl Default for Registry {
    fn default() -> Registry {
        Registry {
            version: STORE_VERSION,
            banks: Vec::new(),
        }
    }
}

impl Default for Preferences {
    fn default() -> Preferences {
        Preferences {
            version: STORE_VERSION,
            families: BTreeMap::new(),
        }
    }
}

pub(crate) fn stored_instruments(instruments: &[BankInstrument]) -> Vec<StoredInstrument> {
    instruments
        .iter()
        .map(|instrument| StoredInstrument {
            msb: instrument.bank.msb(),
            lsb: instrument.bank.lsb(),
            program: instrument.program,
            name: instrument.name.clone(),
        })
        .collect()
}

pub(crate) fn listed_instruments(stored: &[StoredInstrument]) -> Vec<BankInstrument> {
    stored
        .iter()
        .filter_map(|instrument| {
            MidiBank::new(instrument.msb, instrument.lsb)
                .ok()
                .map(|bank| BankInstrument {
                    bank,
                    program: instrument.program,
                    name: instrument.name.clone(),
                })
        })
        .collect()
}

fn broken(path: &Path, reason: impl std::fmt::Display) -> BankError {
    BankError::Storage {
        message: format!("{} cannot be read: {reason}", path.display()),
    }
}

pub(crate) fn read<T: DeserializeOwned + Default>(
    path: &Path,
    version: fn(&T) -> u32,
) -> Result<T, BankError> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(T::default()),
        Err(error) => return Err(storage(path, &error)),
    };
    let mut bytes = Vec::new();
    file.take(STORE_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| storage(path, &error))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > STORE_MAX_BYTES {
        return Err(broken(path, "the file is too large"));
    }
    let value: T = serde_json::from_slice(&bytes).map_err(|error| broken(path, error))?;
    if version(&value) != STORE_VERSION {
        return Err(broken(
            path,
            format!(
                "it has version {}, and OpenTab reads version {STORE_VERSION}",
                version(&value)
            ),
        ));
    }
    Ok(value)
}

pub(crate) fn write<T: Serialize>(path: &Path, value: &T) -> Result<(), BankError> {
    let text = serde_json::to_vec_pretty(value).map_err(|error| BankError::Storage {
        message: error.to_string(),
    })?;
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(TEMPORARY_SUFFIX);
    let temporary = PathBuf::from(temporary);
    let outcome = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temporary)
        .and_then(|mut file| {
            file.write_all(&text)?;
            file.sync_all()
        })
        .and_then(|()| fs::rename(&temporary, path));
    if let Err(error) = outcome {
        let _ = fs::remove_file(&temporary);
        return Err(storage(path, &error));
    }
    Ok(())
}
