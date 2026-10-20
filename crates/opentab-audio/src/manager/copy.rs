use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::manager::limits::ImportLimits;
use crate::{BankError, UnsafeBank};

const CHUNK_BYTES: usize = 64 * 1024;
const MIB_SHIFT: u32 = 20;

pub(crate) fn storage(path: &Path, error: &io::Error) -> BankError {
    BankError::Storage {
        message: format!("{}: {error}", path.display()),
    }
}

pub(crate) fn refuse(reason: UnsafeBank) -> BankError {
    BankError::Unsafe { reason }
}

#[derive(Debug)]
pub(crate) struct Budget {
    limits: ImportLimits,
    entries: u64,
    bytes: u64,
}

impl Budget {
    pub(crate) fn new(limits: ImportLimits) -> Budget {
        Budget {
            limits,
            entries: 0,
            bytes: 0,
        }
    }

    pub(crate) fn written(&self) -> u64 {
        self.bytes
    }

    pub(crate) fn entry(&mut self) -> Result<(), BankError> {
        self.entries += 1;
        if self.entries > self.limits.entries {
            return Err(refuse(UnsafeBank::TooManyFiles {
                limit: self.limits.entries,
            }));
        }
        Ok(())
    }

    pub(crate) fn entries(&self, count: u64) -> Result<(), BankError> {
        if count > self.limits.entries {
            return Err(refuse(UnsafeBank::TooManyFiles {
                limit: self.limits.entries,
            }));
        }
        Ok(())
    }

    pub(crate) fn depth(&self, depth: u64) -> Result<(), BankError> {
        if depth > self.limits.depth {
            return Err(refuse(UnsafeBank::FolderTooDeep {
                limit: self.limits.depth,
            }));
        }
        Ok(())
    }

    pub(crate) fn name(&self, relative: &Path) -> Result<(), BankError> {
        let length = u64::try_from(relative.as_os_str().len()).unwrap_or(u64::MAX);
        if length > self.limits.name_bytes {
            return Err(refuse(UnsafeBank::NameTooLong {
                limit: self.limits.name_bytes,
            }));
        }
        Ok(())
    }

    fn file_too_large(&self) -> BankError {
        refuse(UnsafeBank::BankFileTooLarge {
            limit_mib: self.limits.file_bytes >> MIB_SHIFT,
        })
    }

    fn bank_too_large(&self) -> BankError {
        refuse(UnsafeBank::BankTooLarge {
            limit_mib: self.limits.total_bytes >> MIB_SHIFT,
        })
    }

    pub(crate) fn declared(&self, file: u64, total: u64) -> Result<(), BankError> {
        if file > self.limits.file_bytes {
            return Err(self.file_too_large());
        }
        if total > self.limits.total_bytes {
            return Err(self.bank_too_large());
        }
        Ok(())
    }

    pub(crate) fn write(&mut self, source: &mut dyn Read, target: &Path) -> Result<u64, BankError> {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)
            .map_err(|error| storage(target, &error))?;
        let mut buffer = vec![0u8; CHUNK_BYTES];
        let mut written = 0u64;
        loop {
            let read = match source.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    return Err(BankError::Invalid {
                        reason: format!("a file of the bank cannot be read: {error}"),
                    });
                }
            };
            let grown = u64::try_from(read).unwrap_or(u64::MAX);
            written = written.saturating_add(grown);
            self.bytes = self.bytes.saturating_add(grown);
            if written > self.limits.file_bytes {
                return Err(self.file_too_large());
            }
            if self.bytes > self.limits.total_bytes {
                return Err(self.bank_too_large());
            }
            output
                .write_all(&buffer[..read])
                .map_err(|error| storage(target, &error))?;
        }
        output.sync_all().map_err(|error| storage(target, &error))?;
        Ok(written)
    }
}

#[cfg(unix)]
fn open_source(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
}

#[cfg(not(unix))]
fn open_source(path: &Path) -> io::Result<File> {
    File::open(path)
}

#[cfg(unix)]
fn is_link_refusal(error: &io::Error) -> bool {
    error.raw_os_error() == Some(libc::ELOOP)
}

#[cfg(not(unix))]
fn is_link_refusal(_error: &io::Error) -> bool {
    false
}

pub(crate) fn copy_file(
    source: &Path,
    target: &Path,
    budget: &mut Budget,
) -> Result<(), BankError> {
    let mut file = open_source(source).map_err(|error| {
        if is_link_refusal(&error) {
            refuse(UnsafeBank::BankSymlink)
        } else {
            storage(source, &error)
        }
    })?;
    let metadata = file.metadata().map_err(|error| storage(source, &error))?;
    if !metadata.is_file() {
        return Err(refuse(UnsafeBank::SpecialFile));
    }
    budget.declared(metadata.len(), budget.bytes.saturating_add(metadata.len()))?;
    budget.write(&mut file, target)?;
    Ok(())
}

struct Pending {
    source: PathBuf,
    target: PathBuf,
    relative: PathBuf,
    depth: u64,
}

pub(crate) fn copy_tree(
    source: &Path,
    target: &Path,
    budget: &mut Budget,
) -> Result<(), BankError> {
    let mut pending = vec![Pending {
        source: source.to_path_buf(),
        target: target.to_path_buf(),
        relative: PathBuf::new(),
        depth: 0,
    }];
    while let Some(folder) = pending.pop() {
        let listing =
            fs::read_dir(&folder.source).map_err(|error| storage(&folder.source, &error))?;
        for entry in listing {
            let entry = entry.map_err(|error| storage(&folder.source, &error))?;
            budget.entry()?;
            let name = entry.file_name();
            let relative = folder.relative.join(&name);
            budget.name(&relative)?;
            let kind = entry
                .file_type()
                .map_err(|error| storage(&entry.path(), &error))?;
            let destination = folder.target.join(&name);
            if kind.is_symlink() {
                return Err(refuse(UnsafeBank::BankSymlink));
            } else if kind.is_dir() {
                budget.depth(folder.depth + 1)?;
                fs::create_dir(&destination).map_err(|error| storage(&destination, &error))?;
                pending.push(Pending {
                    source: entry.path(),
                    target: destination,
                    relative,
                    depth: folder.depth + 1,
                });
            } else if kind.is_file() {
                copy_file(&entry.path(), &destination, budget)?;
            } else {
                return Err(refuse(UnsafeBank::SpecialFile));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Endless;

    impl Read for Endless {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            buffer.fill(1);
            Ok(buffer.len())
        }
    }

    fn limits(file_bytes: u64, total_bytes: u64) -> ImportLimits {
        ImportLimits {
            file_bytes,
            total_bytes,
            ..ImportLimits::DEFAULT
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_link_swapped_in_after_the_listing_is_still_refused() {
        let dir = tempfile::tempdir().expect("scratch");
        let link = dir.path().join("link.sf2");
        std::os::unix::fs::symlink("/etc/hostname", &link).expect("link");
        let mut budget = Budget::new(ImportLimits::DEFAULT);
        assert_eq!(
            copy_file(&link, &dir.path().join("copy"), &mut budget),
            Err(refuse(UnsafeBank::BankSymlink))
        );
        assert!(!dir.path().join("copy").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_pipe_swapped_in_after_the_listing_is_refused_without_blocking() {
        let dir = tempfile::tempdir().expect("scratch");
        let pipe = dir.path().join("pipe.sf2");
        let made = std::process::Command::new("mkfifo").arg(&pipe).status();
        assert!(
            made.is_ok_and(|status| status.success()),
            "mkfifo is available"
        );
        let mut budget = Budget::new(ImportLimits::DEFAULT);
        assert_eq!(
            copy_file(&pipe, &dir.path().join("copy"), &mut budget),
            Err(refuse(UnsafeBank::SpecialFile))
        );
    }

    #[test]
    fn writing_stops_at_the_caps_whatever_the_source_declared() {
        let dir = tempfile::tempdir().expect("scratch");
        let cases = [
            (
                limits(100_000, 1 << 30),
                UnsafeBank::BankFileTooLarge { limit_mib: 0 },
            ),
            (
                limits(1 << 30, 100_000),
                UnsafeBank::BankTooLarge { limit_mib: 0 },
            ),
        ];
        for (index, (limits, reason)) in cases.into_iter().enumerate() {
            let target = dir.path().join(format!("out-{index}"));
            let mut budget = Budget::new(limits);
            assert_eq!(
                budget.write(&mut Endless, &target),
                Err(BankError::Unsafe { reason })
            );
            let written = fs::metadata(&target).expect("partial").len();
            assert!(written <= 100_000, "{written} bytes reached the disk");
        }
    }
}
