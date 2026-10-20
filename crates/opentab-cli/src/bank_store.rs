use std::path::Path;

use opentab_audio::{BankManager, default_data_dir};

use crate::catalogue_file;
use crate::error::CliError;

pub fn open(catalogue: Option<&Path>) -> Result<BankManager, CliError> {
    let Some(root) = default_data_dir() else {
        return Err(CliError::SoundBank {
            message: "OpenTab cannot find a folder for its data; set the OPENTAB_DATA_DIR environment variable to one".to_string(),
        });
    };
    match catalogue {
        None => Ok(BankManager::open(&root)?),
        Some(path) => {
            let entries = catalogue_file::read(path)?;
            Ok(BankManager::with_catalogue(&root, entries)?)
        }
    }
}
