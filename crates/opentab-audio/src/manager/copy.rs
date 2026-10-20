use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::manager::limits::ImportLimits;
use crate::{BankError, UnsafeBank};

const CHUNK_BYTES: usize = 64 * 1024;
const MIB_SHIFT: u32 = 20;
const MAC_METADATA_FOLDER: &str = "__MACOSX";
const MAC_METADATA_PREFIX: &str = "._";

pub(crate) fn storage(path: &Path, error: &io::Error) -> BankError {
    BankError::Storage {
        message: format!("{}: {error}", path.display()),
    }
}

pub(crate) fn refuse(reason: UnsafeBank) -> BankError {
    BankError::Unsafe { reason }
}

pub(crate) fn is_mac_metadata(relative: &Path) -> bool {
    relative.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        name == MAC_METADATA_FOLDER || name.starts_with(MAC_METADATA_PREFIX)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RatioCap {
    bytes: u64,
    ratio: u64,
}

#[derive(Debug)]
pub(crate) struct Budget {
    limits: ImportLimits,
    entries: u64,
    bytes: u64,
    ratio: Option<RatioCap>,
}

impl Budget {
    pub(crate) fn new(limits: ImportLimits) -> Budget {
        Budget {
            limits,
            entries: 0,
            bytes: 0,
            ratio: None,
        }
    }

    pub(crate) fn written(&self) -> u64 {
        self.bytes
    }

    pub(crate) fn unpacking(&mut self, archive_bytes: u64) {
        let ratio = self.limits.archive_ratio;
        let bytes = archive_bytes
            .saturating_mul(ratio)
            .max(self.limits.ratio_floor_bytes);
        self.ratio = Some(RatioCap { bytes, ratio });
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

    fn total(&self, total: u64) -> Result<(), BankError> {
        if total > self.limits.total_bytes {
            return Err(self.bank_too_large());
        }
        if let Some(cap) = self.ratio
            && total > cap.bytes
        {
            return Err(refuse(UnsafeBank::ArchiveRatio { ratio: cap.ratio }));
        }
        Ok(())
    }

    pub(crate) fn declared(&self, file: u64, total: u64) -> Result<(), BankError> {
        if file > self.limits.file_bytes {
            return Err(self.file_too_large());
        }
        self.total(total)
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
            self.total(self.bytes)?;
            output
                .write_all(&buffer[..read])
                .map_err(|error| storage(target, &error))?;
        }
        output.sync_all().map_err(|error| storage(target, &error))?;
        Ok(written)
    }
}

#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FileIdentity {
    device: u64,
    inode: u64,
    bytes: u64,
}

#[cfg(unix)]
pub(crate) fn identity(metadata: &Metadata) -> FileIdentity {
    use std::os::unix::fs::MetadataExt;
    FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
        bytes: metadata.len(),
    }
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FileIdentity {
    created: u64,
    written: u64,
    bytes: u64,
}

#[cfg(windows)]
pub(crate) fn identity(metadata: &Metadata) -> FileIdentity {
    use std::os::windows::fs::MetadataExt;
    FileIdentity {
        created: metadata.creation_time(),
        written: metadata.last_write_time(),
        bytes: metadata.len(),
    }
}

#[cfg(not(any(unix, windows)))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FileIdentity {
    bytes: u64,
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn identity(metadata: &Metadata) -> FileIdentity {
    FileIdentity {
        bytes: metadata.len(),
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

#[cfg(windows)]
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

#[cfg(windows)]
fn open_source(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(not(any(unix, windows)))]
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

#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

#[cfg(windows)]
fn is_link(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_link(metadata: &Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListedKind {
    File,
    Folder,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Listed {
    pub(crate) relative: PathBuf,
    pub(crate) kind: ListedKind,
    pub(crate) bytes: u64,
    pub(crate) identity: FileIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Survey {
    pub(crate) root: PathBuf,
    pub(crate) entries: Vec<Listed>,
}

impl Survey {
    pub(crate) fn find(&self, relative: &Path) -> Option<&Listed> {
        self.entries
            .binary_search_by(|entry| entry.relative.as_path().cmp(relative))
            .ok()
            .and_then(|index| self.entries.get(index))
    }
}

fn listed_kind(metadata: &Metadata) -> Result<ListedKind, BankError> {
    if is_link(metadata) {
        return Err(refuse(UnsafeBank::BankSymlink));
    }
    if metadata.is_dir() {
        return Ok(ListedKind::Folder);
    }
    if metadata.is_file() {
        return Ok(ListedKind::File);
    }
    Err(refuse(UnsafeBank::SpecialFile))
}

pub(crate) fn listed_file(path: &Path, relative: PathBuf) -> Result<Listed, BankError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| storage(path, &error))?;
    match listed_kind(&metadata)? {
        ListedKind::File => Ok(Listed {
            relative,
            kind: ListedKind::File,
            bytes: metadata.len(),
            identity: identity(&metadata),
        }),
        ListedKind::Folder => Err(refuse(UnsafeBank::SpecialFile)),
    }
}

struct Pending {
    source: PathBuf,
    relative: PathBuf,
    depth: u64,
    identity: Option<FileIdentity>,
}

fn same_folder(path: &Path, expected: FileIdentity) -> Result<(), BankError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| storage(path, &error))?;
    if is_link(&metadata) || !metadata.is_dir() || identity(&metadata) != expected {
        return Err(refuse(UnsafeBank::BankChanged));
    }
    Ok(())
}

pub(crate) fn survey(root: &Path, limits: ImportLimits) -> Result<Survey, BankError> {
    let mut budget = Budget::new(limits);
    let mut entries = Vec::new();
    let mut pending = vec![Pending {
        source: root.to_path_buf(),
        relative: PathBuf::new(),
        depth: 0,
        identity: None,
    }];
    while let Some(folder) = pending.pop() {
        let listing =
            fs::read_dir(&folder.source).map_err(|error| storage(&folder.source, &error))?;
        for entry in listing {
            let entry = entry.map_err(|error| storage(&folder.source, &error))?;
            budget.entry()?;
            let relative = folder.relative.join(entry.file_name());
            budget.name(&relative)?;
            if is_mac_metadata(Path::new(&entry.file_name())) {
                continue;
            }
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|error| storage(&path, &error))?;
            let kind = listed_kind(&metadata)?;
            if kind == ListedKind::Folder {
                budget.depth(folder.depth + 1)?;
                pending.push(Pending {
                    source: path,
                    relative: relative.clone(),
                    depth: folder.depth + 1,
                    identity: Some(identity(&metadata)),
                });
            }
            entries.push(Listed {
                relative,
                kind,
                bytes: metadata.len(),
                identity: identity(&metadata),
            });
        }
        if let Some(expected) = folder.identity {
            same_folder(&folder.source, expected)?;
        }
    }
    entries.sort_by(|left, right| left.relative.cmp(&right.relative));
    Ok(Survey {
        root: root.to_path_buf(),
        entries,
    })
}

fn budget_files(files: &[Listed], budget: &Budget) -> Result<(), BankError> {
    budget.entries(u64::try_from(files.len()).unwrap_or(u64::MAX))?;
    let mut total = 0u64;
    for file in files {
        total = total.saturating_add(file.bytes);
        budget.declared(file.bytes, total)?;
    }
    Ok(())
}

pub(crate) fn copy_listed(
    source: &Path,
    listed: &Listed,
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
    if is_link(&metadata) {
        return Err(refuse(UnsafeBank::BankSymlink));
    }
    if !metadata.is_file() {
        return Err(refuse(UnsafeBank::SpecialFile));
    }
    if identity(&metadata) != listed.identity {
        return Err(refuse(UnsafeBank::BankChanged));
    }
    budget.declared(metadata.len(), budget.bytes.saturating_add(metadata.len()))?;
    budget.write(&mut file, target)?;
    Ok(())
}

pub(crate) fn copy_files(
    source_root: &Path,
    files: &[Listed],
    target_root: &Path,
    budget: &mut Budget,
) -> Result<(), BankError> {
    budget_files(files, budget)?;
    for file in files {
        let target = target_root.join(&file.relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| storage(parent, &error))?;
        }
        copy_listed(&source_root.join(&file.relative), file, &target, budget)?;
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

    struct CapCase {
        limits: ImportLimits,
        reason: UnsafeBank,
    }

    fn limits(file_bytes: u64, total_bytes: u64) -> ImportLimits {
        ImportLimits {
            file_bytes,
            total_bytes,
            ..ImportLimits::DEFAULT
        }
    }

    fn listed_in(dir: &Path, name: &str) -> Listed {
        listed_file(&dir.join(name), PathBuf::from(name)).expect("listed")
    }

    #[cfg(unix)]
    #[test]
    fn a_link_swapped_in_after_the_listing_is_still_refused() {
        let dir = tempfile::tempdir().expect("scratch");
        fs::write(dir.path().join("link.sf2"), b"real").expect("file");
        let listed = listed_in(dir.path(), "link.sf2");
        fs::remove_file(dir.path().join("link.sf2")).expect("removed");
        let link = dir.path().join("link.sf2");
        std::os::unix::fs::symlink("/etc/hostname", &link).expect("link");
        let mut budget = Budget::new(ImportLimits::DEFAULT);
        assert_eq!(
            copy_listed(&link, &listed, &dir.path().join("copy"), &mut budget),
            Err(refuse(UnsafeBank::BankSymlink))
        );
        assert!(!dir.path().join("copy").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_pipe_swapped_in_after_the_listing_is_refused_without_blocking() {
        let dir = tempfile::tempdir().expect("scratch");
        fs::write(dir.path().join("pipe.sf2"), b"real").expect("file");
        let listed = listed_in(dir.path(), "pipe.sf2");
        fs::remove_file(dir.path().join("pipe.sf2")).expect("removed");
        let pipe = dir.path().join("pipe.sf2");
        let made = std::process::Command::new("mkfifo").arg(&pipe).status();
        assert!(
            made.is_ok_and(|status| status.success()),
            "mkfifo is available"
        );
        let mut budget = Budget::new(ImportLimits::DEFAULT);
        assert_eq!(
            copy_listed(&pipe, &listed, &dir.path().join("copy"), &mut budget),
            Err(refuse(UnsafeBank::SpecialFile))
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_swapped_for_a_link_after_the_listing_is_refused() {
        let dir = tempfile::tempdir().expect("scratch");
        let source = dir.path().join("source");
        let elsewhere = dir.path().join("elsewhere");
        fs::create_dir_all(source.join("Samples")).expect("samples");
        fs::create_dir_all(&elsewhere).expect("elsewhere");
        fs::write(source.join("Samples/a.wav"), b"bank data").expect("sample");
        fs::write(elsewhere.join("a.wav"), b"private!!").expect("private");
        let listing = survey(&source, ImportLimits::DEFAULT).expect("survey");
        let listed = listing
            .find(Path::new("Samples/a.wav"))
            .expect("listed")
            .clone();
        fs::remove_dir_all(source.join("Samples")).expect("removed");
        std::os::unix::fs::symlink(&elsewhere, source.join("Samples")).expect("link");
        let target = dir.path().join("target");
        fs::create_dir_all(&target).expect("target");
        let mut budget = Budget::new(ImportLimits::DEFAULT);
        assert_eq!(
            copy_files(&source, &[listed], &target, &mut budget),
            Err(refuse(UnsafeBank::BankChanged))
        );
        assert!(!target.join("Samples/a.wav").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_swapped_for_a_link_during_the_walk_is_refused() {
        let dir = tempfile::tempdir().expect("scratch");
        let folder = dir.path().join("Samples");
        let elsewhere = dir.path().join("elsewhere");
        fs::create_dir_all(&folder).expect("folder");
        fs::create_dir_all(&elsewhere).expect("elsewhere");
        let listed = identity(&fs::symlink_metadata(&folder).expect("folder"));
        assert_eq!(same_folder(&folder, listed), Ok(()));
        fs::remove_dir(&folder).expect("removed");
        std::os::unix::fs::symlink(&elsewhere, &folder).expect("link");
        assert_eq!(
            same_folder(&folder, listed),
            Err(refuse(UnsafeBank::BankChanged))
        );
    }

    #[test]
    fn a_file_replaced_after_the_listing_is_refused() {
        let dir = tempfile::tempdir().expect("scratch");
        fs::write(dir.path().join("a.sf2"), b"listed").expect("file");
        let listed = listed_in(dir.path(), "a.sf2");
        fs::remove_file(dir.path().join("a.sf2")).expect("removed");
        fs::write(dir.path().join("a.sf2"), b"swapped in").expect("other");
        let mut budget = Budget::new(ImportLimits::DEFAULT);
        assert_eq!(
            copy_listed(
                &dir.path().join("a.sf2"),
                &listed,
                &dir.path().join("copy"),
                &mut budget
            ),
            Err(refuse(UnsafeBank::BankChanged))
        );
    }

    #[test]
    fn writing_stops_at_the_caps_whatever_the_source_declared() {
        let dir = tempfile::tempdir().expect("scratch");
        let cases = [
            CapCase {
                limits: limits(100_000, 1 << 30),
                reason: UnsafeBank::BankFileTooLarge { limit_mib: 0 },
            },
            CapCase {
                limits: limits(1 << 30, 100_000),
                reason: UnsafeBank::BankTooLarge { limit_mib: 0 },
            },
        ];
        for (index, case) in cases.into_iter().enumerate() {
            let target = dir.path().join(format!("out-{index}"));
            let mut budget = Budget::new(case.limits);
            assert_eq!(
                budget.write(&mut Endless, &target),
                Err(BankError::Unsafe {
                    reason: case.reason
                })
            );
            let written = fs::metadata(&target).expect("partial").len();
            assert!(written <= 100_000, "{written} bytes reached the disk");
        }
    }

    #[test]
    fn unpacking_stops_at_the_ratio_cap_whatever_the_archive_declared() {
        let dir = tempfile::tempdir().expect("scratch");
        let mut budget = Budget::new(ImportLimits {
            archive_ratio: 10,
            ratio_floor_bytes: 1_000,
            ..ImportLimits::DEFAULT
        });
        budget.unpacking(5_000);
        assert_eq!(budget.declared(40_000, 50_000), Ok(()));
        assert_eq!(
            budget.declared(40_000, 50_001),
            Err(refuse(UnsafeBank::ArchiveRatio { ratio: 10 }))
        );
        let target = dir.path().join("out");
        assert_eq!(
            budget.write(&mut Endless, &target),
            Err(refuse(UnsafeBank::ArchiveRatio { ratio: 10 }))
        );
        let written = fs::metadata(&target).expect("partial").len();
        assert!(written <= 50_000, "{written} bytes reached the disk");
    }
}
