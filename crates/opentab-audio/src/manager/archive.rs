use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use sevenz_rust2::{ArchiveEntry, ArchiveReader, Password};
use zip::ZipArchive;

use crate::manager::copy::{Budget, refuse, storage};
use crate::{BankError, UnsafeBank};

const TYPE_MASK: u32 = 0o170000;
const TYPE_FILE: u32 = 0o100000;
const TYPE_FOLDER: u32 = 0o040000;
const TYPE_LINK: u32 = 0o120000;
const UNIX_EXTENSION: u32 = 0x8000;
const UNIX_SHIFT: u32 = 16;
const REPARSE_POINT: u32 = 0x400;
const ALLOWED_METHODS: [&[u8]; 5] = [
    &[0x00],
    &[0x03, 0x01, 0x01],
    &[0x21],
    &[0x04, 0x01, 0x08],
    &[0x04, 0x02, 0x02],
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    File,
    Folder,
}

fn invalid(what: &str, error: impl std::fmt::Display) -> BankError {
    BankError::Invalid {
        reason: format!("the {what} archive cannot be read: {error}"),
    }
}

fn unix_kind(mode: u32) -> Result<Option<EntryKind>, BankError> {
    match mode & TYPE_MASK {
        0 => Ok(None),
        TYPE_FILE => Ok(Some(EntryKind::File)),
        TYPE_FOLDER => Ok(Some(EntryKind::Folder)),
        TYPE_LINK => Err(refuse(UnsafeBank::ArchiveLink)),
        _ => Err(refuse(UnsafeBank::SpecialFile)),
    }
}

pub(crate) fn confine(name: &str, budget: &Budget) -> Result<Option<PathBuf>, BankError> {
    if name.contains('\0') {
        return Err(refuse(UnsafeBank::ArchiveEscape));
    }
    let name = name.replace('\\', "/");
    if name.starts_with('/') {
        return Err(refuse(UnsafeBank::ArchiveEscape));
    }
    let mut relative = PathBuf::new();
    let mut depth = 0u64;
    for part in name.split('/') {
        match part {
            "" | "." => continue,
            ".." => return Err(refuse(UnsafeBank::ArchiveEscape)),
            _ => {}
        }
        if part.contains(':') {
            return Err(refuse(UnsafeBank::ArchiveEscape));
        }
        relative.push(part);
        depth += 1;
    }
    if depth == 0 {
        return Ok(None);
    }
    budget.depth(depth - 1)?;
    budget.name(&relative)?;
    Ok(Some(relative))
}

fn inside(root: &Path, folder: &Path) -> Result<(), BankError> {
    let canonical = folder
        .canonicalize()
        .map_err(|error| storage(folder, &error))?;
    if !canonical.starts_with(root) {
        return Err(refuse(UnsafeBank::ArchiveEscape));
    }
    Ok(())
}

fn make_folder(root: &Path, relative: &Path) -> Result<(), BankError> {
    let folder = root.join(relative);
    fs::create_dir_all(&folder).map_err(|error| storage(&folder, &error))?;
    inside(root, &folder)
}

fn place(root: &Path, relative: &Path) -> Result<PathBuf, BankError> {
    let target = root.join(relative);
    let parent = target.parent().unwrap_or(root).to_path_buf();
    fs::create_dir_all(&parent).map_err(|error| storage(&parent, &error))?;
    inside(root, &parent)?;
    if fs::symlink_metadata(&target).is_ok() {
        return Err(BankError::Invalid {
            reason: format!("the archive lists {} twice", relative.display()),
        });
    }
    Ok(target)
}

fn canonical_root(target: &Path) -> Result<PathBuf, BankError> {
    target
        .canonicalize()
        .map_err(|error| storage(target, &error))
}

pub(crate) fn unzip(archive: &Path, target: &Path, budget: &mut Budget) -> Result<(), BankError> {
    let file = File::open(archive).map_err(|error| storage(archive, &error))?;
    let mut zip = ZipArchive::new(BufReader::new(file)).map_err(|error| invalid("zip", error))?;
    let count = zip.len();
    budget.entries(u64::try_from(count).unwrap_or(u64::MAX))?;
    let mut declared = 0u64;
    for index in 0..count {
        let entry = zip
            .by_index_raw(index)
            .map_err(|error| invalid("zip", error))?;
        declared = declared.saturating_add(entry.size());
        budget.declared(entry.size(), declared)?;
    }
    let root = canonical_root(target)?;
    for index in 0..count {
        let mut entry = zip.by_index(index).map_err(|error| invalid("zip", error))?;
        let mode_kind = match entry.unix_mode() {
            Some(mode) => unix_kind(mode)?,
            None => None,
        };
        let Some(relative) = confine(entry.name(), budget)? else {
            continue;
        };
        if entry.is_dir() || mode_kind == Some(EntryKind::Folder) {
            make_folder(&root, &relative)?;
        } else {
            let path = place(&root, &relative)?;
            budget.write(&mut entry, &path)?;
        }
    }
    Ok(())
}

fn sevenz_kind(entry: &ArchiveEntry) -> Result<EntryKind, BankError> {
    if entry.has_windows_attributes {
        let attributes = entry.windows_attributes;
        if attributes & REPARSE_POINT != 0 {
            return Err(refuse(UnsafeBank::ArchiveLink));
        }
        if attributes & UNIX_EXTENSION != 0
            && let Some(kind) = unix_kind(attributes >> UNIX_SHIFT)?
        {
            if kind == EntryKind::Folder || entry.is_directory {
                return Ok(EntryKind::Folder);
            }
            return Ok(kind);
        }
    }
    if entry.is_directory {
        Ok(EntryKind::Folder)
    } else {
        Ok(EntryKind::File)
    }
}

struct Planned {
    relative: Option<PathBuf>,
    kind: EntryKind,
}

pub(crate) fn unseven(archive: &Path, target: &Path, budget: &mut Budget) -> Result<(), BankError> {
    let file = File::open(archive).map_err(|error| storage(archive, &error))?;
    let mut reader = ArchiveReader::new(BufReader::new(file), Password::empty())
        .map_err(|error| invalid("7z", error))?;
    let listing = reader.archive();
    for block in &listing.blocks {
        for coder in &block.coders {
            if !ALLOWED_METHODS.contains(&coder.encoder_method_id()) {
                return Err(BankError::Unsupported {
                    message: String::from(
                        "this 7z archive uses a compression method OpenTab does not unpack",
                    ),
                });
            }
        }
    }
    budget.entries(u64::try_from(listing.files.len()).unwrap_or(u64::MAX))?;
    let mut declared = 0u64;
    let mut plan = std::collections::HashMap::new();
    for entry in &listing.files {
        declared = declared.saturating_add(entry.size);
        budget.declared(entry.size, declared)?;
        let kind = sevenz_kind(entry)?;
        let relative = if entry.is_anti_item {
            None
        } else {
            confine(&entry.name, budget)?
        };
        plan.insert(entry.name.clone(), Planned { relative, kind });
    }
    let root = canonical_root(target)?;
    let mut failure: Option<BankError> = None;
    let outcome = reader.for_each_entries(|entry, data| {
        let step = extract_seven(entry, data, &plan, &root, budget);
        match step {
            Ok(()) => Ok(true),
            Err(error) => {
                failure = Some(error);
                Err(sevenz_rust2::Error::Other(std::borrow::Cow::Borrowed(
                    "stopped",
                )))
            }
        }
    });
    if let Some(error) = failure {
        return Err(error);
    }
    outcome.map_err(|error| invalid("7z", error))
}

fn extract_seven(
    entry: &ArchiveEntry,
    data: &mut dyn Read,
    plan: &std::collections::HashMap<String, Planned>,
    root: &Path,
    budget: &mut Budget,
) -> Result<(), BankError> {
    let Some(planned) = plan.get(&entry.name) else {
        return Err(refuse(UnsafeBank::ArchiveEscape));
    };
    let Some(relative) = &planned.relative else {
        return Ok(());
    };
    match planned.kind {
        EntryKind::Folder => make_folder(root, relative),
        EntryKind::File => {
            let path = place(root, relative)?;
            budget.write(data, &path).map(|_| ())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    use super::*;
    use crate::manager::limits::ImportLimits;

    fn one_entry_zip(name: &str, data: &[u8]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(name, SimpleFileOptions::default())
            .expect("entry");
        writer.write_all(data).expect("data");
        writer.finish().expect("finished").into_inner()
    }

    #[test]
    fn lexical_confinement_refuses_every_escape() {
        let budget = Budget::new(ImportLimits::DEFAULT);
        for name in [
            "../evil.sfz",
            "a/../../evil.sfz",
            "a/..",
            "..",
            "/evil.sfz",
            "\\evil.sfz",
            "..\\evil.sfz",
            "C:/evil.sfz",
            "C:evil.sfz",
            "a/b:c.sfz",
            "a\0b.sfz",
        ] {
            assert_eq!(
                confine(name, &budget),
                Err(refuse(UnsafeBank::ArchiveEscape)),
                "{name:?}"
            );
        }
        assert_eq!(
            confine("./a//b/./c.sfz", &budget),
            Ok(Some(PathBuf::from("a/b/c.sfz")))
        );
        assert_eq!(confine("./", &budget), Ok(None));
    }

    #[cfg(unix)]
    #[test]
    fn an_entry_reaching_through_a_link_in_the_target_is_refused() {
        let dir = tempfile::tempdir().expect("scratch");
        let target = dir.path().join("target");
        let outside = dir.path().join("outside");
        fs::create_dir_all(&target).expect("target");
        fs::create_dir_all(&outside).expect("outside");
        std::os::unix::fs::symlink(&outside, target.join("a")).expect("link");
        let archive = dir.path().join("a.zip");
        fs::write(&archive, one_entry_zip("a/x.sfz", b"escaped")).expect("zip");
        let mut budget = Budget::new(ImportLimits::DEFAULT);
        assert_eq!(
            unzip(&archive, &target, &mut budget),
            Err(refuse(UnsafeBank::ArchiveEscape))
        );
        assert!(!outside.join("x.sfz").exists());
    }

    #[test]
    fn a_declared_size_over_the_cap_refuses_before_unpacking() {
        let dir = tempfile::tempdir().expect("scratch");
        let mut bytes = one_entry_zip("a.sfz", b"small");
        let central = bytes
            .windows(4)
            .position(|window| window == b"PK\x01\x02")
            .expect("a central directory");
        bytes[central + 24..central + 28].copy_from_slice(&0xFFFF_FF00u32.to_le_bytes());
        let archive = dir.path().join("a.zip");
        fs::write(&archive, bytes).expect("zip");
        let target = dir.path().join("target");
        fs::create_dir_all(&target).expect("target");
        let mut budget = Budget::new(ImportLimits::DEFAULT);
        assert_eq!(
            unzip(&archive, &target, &mut budget),
            Err(refuse(UnsafeBank::BankFileTooLarge { limit_mib: 1024 }))
        );
        assert!(!target.join("a.sfz").exists());
    }
}
