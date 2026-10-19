use std::collections::BTreeMap;

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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Listing {
    entries: BTreeMap<ListingKey, u32>,
    bytes: u64,
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
    pub(crate) fn note(&mut self, name: &str, scope: SfzScope, reason: IgnoreReason) {
        let key = ListingKey {
            name: fold_digits(name),
            scope,
            reason,
        };
        let cost = key.name.len() as u64 + ENTRY_OVERHEAD_BYTES;
        let count = self.entries.entry(key).or_insert_with(|| {
            self.bytes += cost;
            0
        });
        *count = count.saturating_add(1);
    }

    pub(crate) fn bytes(&self) -> u64 {
        self.bytes
    }

    pub(crate) fn merge(&mut self, other: Listing) {
        for (key, count) in other.entries {
            let cost = key.name.len() as u64 + ENTRY_OVERHEAD_BYTES;
            let total = self.entries.entry(key).or_insert_with(|| {
                self.bytes += cost;
                0
            });
            *total = total.saturating_add(count);
        }
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
