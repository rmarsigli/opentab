use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{BUILT_IN_BANK_NAME, BankError};

pub const BANK_ID_MAX_BYTES: usize = 64;
const RESERVED_DEVICES: [&str; 6] = ["con", "prn", "aux", "nul", "conin$", "conout$"];
const NUMBERED_DEVICES: [&str; 2] = ["com", "lpt"];
const DEVICE_DIGITS: [&str; 13] = [
    "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "\u{b9}", "\u{b2}", "\u{b3}",
];
const RESERVED_SUFFIX: &str = "-bank";

pub(crate) fn windows_reserved(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim_end_matches(' ');
    let lowered = stem.to_lowercase();
    RESERVED_DEVICES.contains(&lowered.as_str())
        || NUMBERED_DEVICES.iter().any(|prefix| {
            lowered
                .strip_prefix(prefix)
                .is_some_and(|digit| DEVICE_DIGITS.contains(&digit))
        })
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct BankId(String);

fn bad_name(text: &str) -> BankError {
    BankError::BadName {
        text: text.to_owned(),
    }
}

fn allowed(character: char) -> bool {
    character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
}

impl BankId {
    pub fn new(text: &str) -> Result<BankId, BankError> {
        let shaped = !text.is_empty()
            && text.len() <= BANK_ID_MAX_BYTES
            && text.chars().all(allowed)
            && !text.starts_with('-')
            && !text.ends_with('-')
            && !text.contains("--");
        if !shaped || text == BUILT_IN_BANK_NAME || windows_reserved(text) {
            return Err(bad_name(text));
        }
        Ok(BankId(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn slug(text: &str) -> String {
        let mut slug = String::new();
        for character in text.chars().flat_map(char::to_lowercase) {
            if character.is_ascii_lowercase() || character.is_ascii_digit() {
                slug.push(character);
            } else if !slug.is_empty() && !slug.ends_with('-') {
                slug.push('-');
            }
        }
        let mut slug: String = slug.chars().take(BANK_ID_MAX_BYTES - 8).collect();
        while slug.ends_with('-') {
            slug.pop();
        }
        if slug.is_empty() || slug == BUILT_IN_BANK_NAME {
            slug = String::from("bank");
        }
        if windows_reserved(&slug) {
            slug.push_str(RESERVED_SUFFIX);
        }
        slug
    }
}

impl TryFrom<String> for BankId {
    type Error = BankError;

    fn try_from(text: String) -> Result<BankId, BankError> {
        BankId::new(&text)
    }
}

impl From<BankId> for String {
    fn from(id: BankId) -> String {
        id.0
    }
}

impl fmt::Display for BankId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum BankRef {
    BuiltIn,
    Installed(BankId),
}

impl BankRef {
    pub fn parse(text: &str) -> Result<BankRef, BankError> {
        if text == BUILT_IN_BANK_NAME {
            return Ok(BankRef::BuiltIn);
        }
        BankId::new(text).map(BankRef::Installed)
    }

    pub fn as_str(&self) -> &str {
        match self {
            BankRef::BuiltIn => BUILT_IN_BANK_NAME,
            BankRef::Installed(id) => id.as_str(),
        }
    }
}

impl TryFrom<String> for BankRef {
    type Error = BankError;

    fn try_from(text: String) -> Result<BankRef, BankError> {
        BankRef::parse(&text)
    }
}

impl From<BankRef> for String {
    fn from(bank: BankRef) -> String {
        bank.as_str().to_owned()
    }
}

impl fmt::Display for BankRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}
