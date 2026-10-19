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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadProblem {
    Missing,
    TooLarge,
}

#[derive(Debug)]
pub(crate) struct BankFs {
    root: PathBuf,
    listings: HashMap<PathBuf, Vec<OsString>>,
    entry_cap: u64,
    listed: u64,
    listed_cap: u64,
}

fn invalid(reason: &str) -> BankError {
    BankError::Invalid {
        reason: reason.to_owned(),
    }
}

fn unsafe_bank(reason: UnsafeBank) -> BankError {
    BankError::Unsafe { reason }
}

pub(crate) fn read_capped(path: &Path, cap: u64) -> Result<Vec<u8>, ReadProblem> {
    if !fs::metadata(path).is_ok_and(|metadata| metadata.is_file()) {
        return Err(ReadProblem::Missing);
    }
    let file = File::open(path).map_err(|_| ReadProblem::Missing)?;
    let declared = file.metadata().map_err(|_| ReadProblem::Missing)?.len();
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

fn same_name(left: &str, right: &str) -> bool {
    left.to_lowercase() == right.to_lowercase()
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
            entry_cap: limits.folder_entries,
            listed: 0,
            listed_cap: limits.listed_entries,
        })
    }

    pub(crate) fn listed(&self) -> u64 {
        self.listed
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
            let Ok(listing) = fs::read_dir(&folder) else {
                continue;
            };
            for entry in listing.flatten() {
                entries += 1;
                if entries > limits.folder_entries {
                    return Err(unsafe_bank(UnsafeBank::TooManyFiles {
                        limit: limits.folder_entries,
                    }));
                }
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
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

    fn listing(&mut self, folder: &Path) -> Result<&[OsString], PathProblem> {
        let canonical = fs::canonicalize(folder).map_err(|_| PathProblem::Missing)?;
        let canonical = self.confine(canonical)?;
        if !self.listings.contains_key(&canonical) {
            let mut names = Vec::new();
            if let Ok(entries) = fs::read_dir(&canonical) {
                for entry in entries.flatten() {
                    if names.len() as u64 >= self.entry_cap {
                        break;
                    }
                    if self.listed >= self.listed_cap {
                        return Err(PathProblem::Exhausted);
                    }
                    self.listed += 1;
                    names.push(entry.file_name());
                }
            }
            self.listings.insert(canonical.clone(), names);
        }
        Ok(self.listings.get(&canonical).map_or(&[][..], Vec::as_slice))
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
            let wanted = name.to_string_lossy().into_owned();
            let matches: Vec<OsString> = self
                .listing(&current)?
                .iter()
                .filter(|entry| same_name(&entry.to_string_lossy(), &wanted))
                .cloned()
                .collect();
            let [only] = matches.as_slice() else {
                return Err(PathProblem::Missing);
            };
            current = current.join(only);
        }
        Ok(current)
    }

    pub(crate) fn resolve(&mut self, base: &Path, text: &str) -> Result<PathBuf, PathProblem> {
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
