use std::collections::BTreeMap;
use std::ops::Bound;

use crate::{BankError, UnsafeBank};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SfzScope {
    File,
    Control,
    Global,
    Master,
    Group,
    Region,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IgnoreReason {
    Unsupported,
    InvalidValue,
    Clamped,
    UnresolvedVariable,
    PathOutsideBank,
    MissingFile,
    UnsupportedAudioFormat,
    ReleaseTrigger,
    Generator,
    DisabledRegion,
    MissingSample,
    MissingInclude,
    UnknownDirective,
    OutsideHeader,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IgnoredOpcode {
    pub name: String,
    pub scope: SfzScope,
    pub reason: IgnoreReason,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct ListingKey {
    name: String,
    scope: SfzScope,
    reason: IgnoreReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Listing {
    entries: BTreeMap<ListingKey, u32>,
    bytes: u64,
    names: u64,
    distinct: u64,
    allowance: u64,
    refused: Option<UnsafeBank>,
}

const ENTRY_OVERHEAD_BYTES: u64 = 96;

pub(crate) fn fold_digits(name: &str) -> String {
    let mut folded = String::with_capacity(name.len());
    let mut in_digits = false;
    for character in name.chars() {
        if character.is_ascii_digit() {
            if !in_digits {
                folded.push('N');
            }
            in_digits = true;
        } else {
            folded.push(character);
            in_digits = false;
        }
    }
    folded
}

impl Listing {
    pub(crate) fn capped(names: u64, allowance: u64) -> Listing {
        Listing {
            entries: BTreeMap::new(),
            bytes: 0,
            names,
            distinct: 0,
            allowance,
            refused: None,
        }
    }

    pub(crate) fn allow(&mut self, allowance: u64) {
        self.allowance = allowance;
    }

    fn admit(&mut self, key: ListingKey, count: u32) {
        if let Some(total) = self.entries.get_mut(&key) {
            *total = total.saturating_add(count);
            return;
        }
        if self.refused.is_some() {
            return;
        }
        let known = self.knows_name(&key);
        if !known && self.distinct >= self.names {
            self.refused = Some(UnsafeBank::IgnoredNames { limit: self.names });
            return;
        }
        let cost = key.name.len() as u64 + ENTRY_OVERHEAD_BYTES;
        if self.bytes.saturating_add(cost) > self.allowance {
            self.refused = Some(UnsafeBank::LoaderBudget);
            return;
        }
        self.bytes += cost;
        if !known {
            self.distinct += 1;
        }
        self.entries.insert(key, count);
    }

    fn knows_name(&self, key: &ListingKey) -> bool {
        let before = self
            .entries
            .range((Bound::Unbounded, Bound::Excluded(key)))
            .next_back();
        let after = self
            .entries
            .range((Bound::Excluded(key), Bound::Unbounded))
            .next();
        before
            .into_iter()
            .chain(after)
            .any(|(other, _)| other.name == key.name)
    }

    pub(crate) fn note(&mut self, name: &str, scope: SfzScope, reason: IgnoreReason) {
        self.admit(
            ListingKey {
                name: fold_digits(name),
                scope,
                reason,
            },
            1,
        );
    }

    pub(crate) fn check(&self) -> Result<(), BankError> {
        match self.refused {
            Some(reason) => Err(BankError::Unsafe { reason }),
            None => Ok(()),
        }
    }

    pub(crate) fn bytes(&self) -> u64 {
        self.bytes
    }

    pub(crate) fn merge(&mut self, other: Listing) -> Result<(), BankError> {
        for (key, count) in other.entries {
            self.admit(key, count);
        }
        self.check()
    }

    pub(crate) fn entries(&self) -> Vec<IgnoredOpcode> {
        self.entries
            .iter()
            .map(|(key, count)| IgnoredOpcode {
                name: key.name.clone(),
                scope: key.scope,
                reason: key.reason,
                count: *count,
            })
            .collect()
    }
}
