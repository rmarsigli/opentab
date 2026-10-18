use crate::ModelError;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Base64Bytes(pub Vec<u8>);

impl Serialize for Base64Bytes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for Base64Bytes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        STANDARD
            .decode(text.as_bytes())
            .map(Base64Bytes)
            .map_err(|error| D::Error::custom(format!("invalid base64: {error}")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContainerEntry {
    File { name: String, data: Base64Bytes },
    Score { name: String },
}

impl ContainerEntry {
    pub fn name(&self) -> &str {
        match self {
            ContainerEntry::File { name, .. } | ContainerEntry::Score { name } => name,
        }
    }

    pub fn is_score(&self) -> bool {
        matches!(self, ContainerEntry::Score { .. })
    }
}

pub(crate) fn check_container(entries: &[ContainerEntry]) -> Result<(), ModelError> {
    let scores = entries.iter().filter(|entry| entry.is_score()).count();
    if scores > 1 {
        return Err(ModelError::InvalidContainer { scores });
    }
    Ok(())
}
