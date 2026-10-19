use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use crate::sfz::limits::SfzLimits;
use crate::{BankError, UnsafeBank};

const SFZ_EXTENSION: &str = "sfz";
const DRIVE_SEPARATOR: char = ':';

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PathProblem {
    Rejected,
    Outside,
    Missing,
    Exhausted,
    Lookups,
}

pub(crate) fn refusal(problem: PathProblem, limits: &SfzLimits) -> Option<BankError> {
    let reason = match problem {
        PathProblem::Exhausted => UnsafeBank::ListedEntries {
            limit: limits.listed_entries,
        },
        PathProblem::Lookups => UnsafeBank::PathLookups {
            limit: limits.path_lookups,
        },
        PathProblem::Rejected | PathProblem::Outside | PathProblem::Missing => return None,
    };
    Some(unsafe_bank(reason))
}

type FolderIndex = HashMap<String, Vec<OsString>>;
type ResolveKey = (PathBuf, String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadProblem {
    Missing,
    TooLarge,
}

#[derive(Debug)]
pub(crate) struct BankFs {
    root: PathBuf,
    listings: HashMap<PathBuf, FolderIndex>,
    resolved: HashMap<ResolveKey, Result<PathBuf, PathProblem>>,
    entry_cap: u64,
    listed: u64,
    listed_cap: u64,
    lookups: u64,
    lookup_cap: u64,
    steps: u64,
}

fn invalid(reason: &str) -> BankError {
    BankError::Invalid {
        reason: reason.to_owned(),
    }
}

fn unsafe_bank(reason: UnsafeBank) -> BankError {
    BankError::Unsafe { reason }
}

#[cfg(unix)]
fn same_file(checked: &fs::Metadata, opened: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    checked.dev() == opened.dev() && checked.ino() == opened.ino()
}

#[cfg(not(unix))]
fn same_file(_checked: &fs::Metadata, opened: &fs::Metadata) -> bool {
    opened.is_file()
}

pub(crate) fn read_capped(path: &Path, cap: u64) -> Result<Vec<u8>, ReadProblem> {
    let checked = fs::symlink_metadata(path).map_err(|_| ReadProblem::Missing)?;
    if !checked.is_file() {
        return Err(ReadProblem::Missing);
    }
    let file = File::open(path).map_err(|_| ReadProblem::Missing)?;
    let opened = file.metadata().map_err(|_| ReadProblem::Missing)?;
    if !opened.is_file() || !same_file(&checked, &opened) {
        return Err(ReadProblem::Missing);
    }
    let declared = opened.len();
    if declared > cap {
        return Err(ReadProblem::TooLarge);
    }
    let mut bytes = Vec::new();
    let reserve = usize::try_from(declared).map_err(|_| ReadProblem::TooLarge)?;
    bytes
        .try_reserve_exact(reserve)
        .map_err(|_| ReadProblem::TooLarge)?;
    file.take(cap.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| ReadProblem::Missing)?;
    if bytes.len() as u64 > cap {
        return Err(ReadProblem::TooLarge);
    }
    Ok(bytes)
}

fn has_drive_prefix(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(
        (chars.next(), chars.next()),
        (Some(letter), Some(DRIVE_SEPARATOR)) if letter.is_ascii_alphabetic()
    )
}

impl BankFs {
    pub(crate) fn open(root: &Path, limits: &SfzLimits) -> Result<BankFs, BankError> {
        let root = fs::canonicalize(root).map_err(|_| invalid("the bank folder cannot be read"))?;
        if !root.is_dir() {
            return Err(invalid("an SFZ bank is a folder"));
        }
        Ok(BankFs {
            root,
            listings: HashMap::new(),
            resolved: HashMap::new(),
            entry_cap: limits.folder_entries,
            listed: 0,
            listed_cap: limits.listed_entries,
            lookups: 0,
            lookup_cap: limits.path_lookups,
            steps: 0,
        })
    }

    pub(crate) fn listed(&self) -> u64 {
        self.listed
    }

    pub(crate) fn lookups(&self) -> u64 {
        self.lookups
    }

    pub(crate) fn steps(&self) -> u64 {
        self.steps
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn relative_name(&self, path: &Path) -> String {
        let relative = path.strip_prefix(&self.root).unwrap_or(path);
        relative
            .components()
            .filter_map(|component| match component {
                Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect::<Vec<String>>()
            .join("/")
    }

    pub(crate) fn sfz_files(&self, limits: &SfzLimits) -> Result<Vec<PathBuf>, BankError> {
        let mut found = Vec::new();
        let mut pending: Vec<(PathBuf, u64)> = vec![(self.root.clone(), 0)];
        let mut entries: u64 = 0;
        while let Some((folder, depth)) = pending.pop() {
            let listing =
                fs::read_dir(&folder).map_err(|_| unsafe_bank(UnsafeBank::UnreadableFolder))?;
            for entry in listing {
                let entry = entry.map_err(|_| unsafe_bank(UnsafeBank::UnreadableFolder))?;
                entries += 1;
                if entries > limits.folder_entries {
                    return Err(unsafe_bank(UnsafeBank::TooManyFiles {
                        limit: limits.folder_entries,
                    }));
                }
                let kind = entry
                    .file_type()
                    .map_err(|_| unsafe_bank(UnsafeBank::UnreadableFolder))?;
                if kind.is_symlink() {
                    return Err(unsafe_bank(UnsafeBank::BankSymlink));
                }
                let path = entry.path();
                if kind.is_dir() {
                    if depth + 1 > limits.folder_depth {
                        return Err(unsafe_bank(UnsafeBank::FolderTooDeep {
                            limit: limits.folder_depth,
                        }));
                    }
                    pending.push((path, depth + 1));
                } else if kind.is_file()
                    && path
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case(SFZ_EXTENSION))
                {
                    found.push(path);
                    if found.len() as u64 > limits.sfz_files {
                        return Err(unsafe_bank(UnsafeBank::TooManyInstruments {
                            limit: limits.sfz_files,
                        }));
                    }
                }
            }
        }
        found.sort();
        Ok(found)
    }

    fn listing(&mut self, folder: &Path) -> Result<&FolderIndex, PathProblem> {
        let canonical = fs::canonicalize(folder).map_err(|_| PathProblem::Missing)?;
        let canonical = self.confine(canonical)?;
        if !self.listings.contains_key(&canonical) {
            let mut index = FolderIndex::new();
            let mut count: u64 = 0;
            if let Ok(entries) = fs::read_dir(&canonical) {
                for entry in entries.flatten() {
                    if count >= self.entry_cap {
                        break;
                    }
                    if self.listed >= self.listed_cap {
                        return Err(PathProblem::Exhausted);
                    }
                    self.listed += 1;
                    count += 1;
                    let name = entry.file_name();
                    let lowered = name.to_string_lossy().to_lowercase();
                    self.steps = self.steps.saturating_add(lowered.len() as u64);
                    index.entry(lowered).or_default().push(name);
                }
            }
            self.listings.insert(canonical.clone(), index);
        }
        self.listings.get(&canonical).ok_or(PathProblem::Missing)
    }

    fn find_ignoring_case(&mut self, candidate: &Path) -> Result<PathBuf, PathProblem> {
        let relative = candidate
            .strip_prefix(&self.root)
            .map_err(|_| PathProblem::Missing)?
            .to_path_buf();
        let mut current = self.root.clone();
        for component in relative.components() {
            let Component::Normal(name) = component else {
                return Err(PathProblem::Missing);
            };
            let exact = current.join(name);
            if fs::symlink_metadata(&exact).is_ok() {
                current = exact;
                continue;
            }
            let wanted = name.to_string_lossy().to_lowercase();
            self.steps = self.steps.saturating_add(wanted.len() as u64);
            let found = match self.listing(&current)?.get(&wanted).map(Vec::as_slice) {
                Some([only]) => only.clone(),
                _ => return Err(PathProblem::Missing),
            };
            current = current.join(found);
        }
        Ok(current)
    }

    pub(crate) fn resolve(&mut self, base: &Path, text: &str) -> Result<PathBuf, PathProblem> {
        let key = (base.to_path_buf(), text.to_owned());
        if let Some(found) = self.resolved.get(&key) {
            return found.clone();
        }
        self.lookups += 1;
        if self.lookups > self.lookup_cap {
            return Err(PathProblem::Lookups);
        }
        self.steps = self.steps.saturating_add(text.len() as u64);
        let outcome = self.resolve_uncached(base, text);
        if !matches!(outcome, Err(PathProblem::Exhausted)) {
            self.resolved.insert(key, outcome.clone());
        }
        outcome
    }

    fn resolve_uncached(&mut self, base: &Path, text: &str) -> Result<PathBuf, PathProblem> {
        let text = text.replace('\\', "/");
        if text.is_empty()
            || text.contains('\0')
            || text.starts_with('/')
            || has_drive_prefix(&text)
        {
            return Err(PathProblem::Rejected);
        }
        let mut candidate = base.to_path_buf();
        for part in text.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    candidate.pop();
                }
                name => candidate.push(name),
            }
        }
        let found = if fs::symlink_metadata(&candidate).is_ok() {
            candidate
        } else {
            self.find_ignoring_case(&candidate)?
        };
        let canonical = fs::canonicalize(&found).map_err(|_| PathProblem::Missing)?;
        self.confine(canonical)
    }

    fn confine(&self, canonical: PathBuf) -> Result<PathBuf, PathProblem> {
        if canonical.starts_with(&self.root) {
            Ok(canonical)
        } else {
            Err(PathProblem::Outside)
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::path::Path;

    use super::{ReadProblem, read_capped};

    #[test]
    fn a_file_that_reads_longer_than_it_declares_is_still_capped() {
        let path = Path::new("/proc/self/maps");
        assert_eq!(
            std::fs::metadata(path).map(|metadata| metadata.len()).ok(),
            Some(0)
        );
        assert_eq!(read_capped(path, 16), Err(ReadProblem::TooLarge));
    }

    #[test]
    fn a_named_pipe_is_never_opened() {
        let dir = std::env::temp_dir().join(format!("opentab-fifo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("folder");
        let pipe = dir.join("pipe.sfz");
        let made = std::process::Command::new("mkfifo").arg(&pipe).status();
        if made.is_ok_and(|status| status.success()) {
            assert_eq!(read_capped(&pipe, 16), Err(ReadProblem::Missing));
        }
        std::fs::remove_dir_all(&dir).expect("cleaned");
    }
}
