use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use opentab_audio::{
    ArchiveKind, BankError, BankFormat, BankId, BankManager, BankRef, BankSource, CatalogueEntry,
    ImportLimits, SIDECAR_NAME, UnsafeBank, check_download_url, default_catalogue, sidecar_for,
};
use opentab_testkit::archives::{ZipItem, zip_archive};
use opentab_testkit::banks::{SIDECAR_FILE, tiny_sfz};
use sha2::{Digest, Sha256};

use crate::support::{scratch_dir, tiny_bytes};

pub fn manager_in(dir: &Path) -> BankManager {
    BankManager::with_catalogue(&dir.join("data"), Vec::new()).expect("the manager opens")
}

pub fn manager_with(dir: &Path, entries: Vec<CatalogueEntry>) -> BankManager {
    BankManager::with_catalogue(&dir.join("data"), entries).expect("the manager opens")
}

pub fn listing(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

pub fn assert_clean(manager: &BankManager) {
    assert_eq!(
        listing(&manager.staging_dir()),
        Vec::<String>::new(),
        "staging is empty"
    );
    assert_eq!(
        listing(&manager.root().join("banks")),
        Vec::<String>::new(),
        "no bank folder is left"
    );
    assert!(manager.list().is_empty(), "nothing is registered");
}

pub fn sha_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn entry_for(
    id: &str,
    format: BankFormat,
    archive: ArchiveKind,
    url: &str,
    bytes: &[u8],
) -> CatalogueEntry {
    CatalogueEntry {
        id: BankId::new(id).expect("an id"),
        name: format!("Test {id}"),
        format,
        archive,
        url: url.to_owned(),
        sha256: sha_hex(bytes),
        size: bytes.len() as u64,
        licence_name: String::from("CC0-1.0"),
        licence_url: String::from("https://example.org/licence"),
        licence_text: String::from("No rights reserved."),
        attribution: None,
        covers: String::from("tests"),
    }
}

pub fn write_tiny_sf2(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, tiny_bytes()).expect("the tiny bank is written");
    path
}

pub fn sfz_without_sidecar(dir: &Path) -> PathBuf {
    let root = tiny_sfz(dir);
    fs::remove_file(root.join(SIDECAR_FILE)).expect("the sidecar is removed");
    root
}

pub fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut found = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(folder) = pending.pop() {
        for entry in fs::read_dir(&folder).expect("a folder") {
            let entry = entry.expect("an entry");
            let path = entry.path();
            if entry.file_type().expect("a type").is_dir() {
                pending.push(path);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .expect("inside")
                    .to_string_lossy()
                    .replace('\\', "/");
                found.insert(relative, fs::read(&path).expect("a file"));
            }
        }
    }
    found
}

pub fn zip_tree(root: &Path, prefix: &str) -> Vec<u8> {
    let items: Vec<ZipItem> = tree(root)
        .into_iter()
        .map(|(name, data)| ZipItem::File {
            name: format!("{prefix}{name}"),
            data,
        })
        .collect();
    zip_archive(&items)
}

fn refused(result: Result<opentab_audio::InstalledBank, BankError>) -> UnsafeBank {
    match result {
        Err(BankError::Unsafe { reason }) => reason,
        other => panic!("expected an unsafe refusal, got {other:?}"),
    }
}

fn small_limits() -> ImportLimits {
    ImportLimits {
        total_bytes: 1_000,
        file_bytes: 600,
        entries: 4,
        depth: 2,
        name_bytes: 40,
    }
}

#[test]
fn the_tiny_sf2_imports_as_a_copy_and_survives_a_reopen() {
    let dir = scratch_dir();
    let source = write_tiny_sf2(dir.path(), "My Tiny Bank.sf2");
    let mut manager = manager_in(dir.path());
    let bank = manager.import(&source).expect("the tiny bank imports");
    assert_eq!(bank.format, BankFormat::Sf2);
    assert_eq!(bank.source, BankSource::Imported);
    assert_eq!(bank.name, "OpenTab Tiny Test Bank");
    assert_eq!(bank.id.as_str(), "my-tiny-bank");
    assert!(!bank.instruments.is_empty());
    assert!(
        bank.path
            .starts_with(manager.root().join("banks").join("my-tiny-bank"))
    );
    assert_eq!(bank.bytes, tiny_bytes().len() as u64);
    fs::remove_file(&source).expect("the original is removed");
    assert_eq!(fs::read(&bank.path).expect("the copy"), tiny_bytes());
    assert_eq!(listing(&manager.staging_dir()), Vec::<String>::new());
    let reopened = manager_in(dir.path());
    assert_eq!(reopened.list(), vec![bank]);
}

#[test]
fn a_second_import_of_the_same_name_gets_a_new_id() {
    let dir = scratch_dir();
    let source = write_tiny_sf2(dir.path(), "bank.sf2");
    let mut manager = manager_in(dir.path());
    let first = manager.import(&source).expect("first");
    let second = manager.import(&source).expect("second");
    assert_eq!(first.id.as_str(), "bank");
    assert_eq!(second.id.as_str(), "bank-2");
    assert_eq!(manager.list().len(), 2);
}

#[test]
fn an_sfz_folder_import_writes_the_program_map() {
    let dir = scratch_dir();
    let source = sfz_without_sidecar(dir.path());
    let mut manager = manager_in(dir.path());
    let bank = manager.import(&source).expect("the tiny SFZ imports");
    assert_eq!(bank.format, BankFormat::Sfz);
    assert_eq!(bank.name, "OpenTab Tiny SFZ");
    let sidecar: serde_json::Value =
        serde_json::from_slice(&fs::read(bank.path.join(SIDECAR_NAME)).expect("a sidecar"))
            .expect("json");
    let programs: BTreeMap<u64, String> = sidecar["programs"]
        .as_array()
        .expect("programs")
        .iter()
        .map(|item| {
            (
                item["program"].as_u64().expect("program"),
                item["file"].as_str().expect("file").to_owned(),
            )
        })
        .collect();
    for program in 24..32 {
        assert_eq!(programs[&program], "Programs/guitar.sfz");
    }
    for program in 32..40 {
        assert_eq!(programs[&program], "Programs/bass.sfz");
    }
    assert_eq!(sidecar["drums"], "Programs/drum kit.sfz");
    assert_eq!(sidecar["default"], "Programs/bass.sfz");
    assert!(!programs.values().any(|file| file.contains("includes")));
    let drums = bank
        .instruments
        .iter()
        .find(|instrument| instrument.bank.msb() == 120)
        .expect("a drum instrument");
    assert_eq!(drums.name, "Programs/drum kit");
    let guitar = bank
        .instruments
        .iter()
        .find(|instrument| instrument.name == "Programs/guitar")
        .expect("the guitar");
    assert_eq!(guitar.program, 24);
}

#[test]
fn an_existing_program_map_is_kept() {
    let dir = scratch_dir();
    let source = tiny_sfz(dir.path());
    let original = fs::read(source.join(SIDECAR_FILE)).expect("the sidecar");
    let mut manager = manager_in(dir.path());
    let bank = manager.import(&source).expect("imports");
    assert_eq!(
        fs::read(bank.path.join(SIDECAR_NAME)).expect("kept"),
        original
    );
}

#[test]
fn a_zip_of_an_sfz_folder_imports_into_its_inner_folder() {
    let dir = scratch_dir();
    let source = sfz_without_sidecar(dir.path());
    let archive = dir.path().join("tiny.zip");
    fs::write(&archive, zip_tree(&source, "OpenTab Tiny SFZ/")).expect("zip");
    let mut manager = manager_in(dir.path());
    let bank = manager.import(&archive).expect("the zip imports");
    assert_eq!(bank.format, BankFormat::Sfz);
    assert_eq!(bank.id.as_str(), "tiny");
    assert!(bank.path.ends_with("OpenTab Tiny SFZ"));
    let mut copied = tree(&bank.path);
    copied.remove(SIDECAR_NAME);
    assert_eq!(copied, tree(&source));
}

#[test]
fn removing_a_bank_unregisters_it_and_deletes_its_files() {
    let dir = scratch_dir();
    let source = write_tiny_sf2(dir.path(), "gone.sf2");
    let mut manager = manager_in(dir.path());
    let bank = manager.import(&source).expect("imports");
    manager.remove(&bank.id).expect("removes");
    assert_clean(&manager);
    assert!(manager_in(dir.path()).list().is_empty());
    assert_eq!(
        manager.remove(&bank.id),
        Err(BankError::NotInstalled {
            id: String::from("gone")
        })
    );
}

#[test]
fn files_that_are_not_banks_are_refused_with_a_message() {
    let dir = scratch_dir();
    let mut manager = manager_in(dir.path());
    let text = dir.path().join("notes.txt");
    fs::write(&text, "hello").expect("text");
    let seven = dir.path().join("bank.7z");
    fs::write(&seven, b"7z\xBC\xAF\x27\x1C\x00\x04").expect("7z");
    let lone = dir.path().join("lone.sfz");
    fs::write(&lone, "<region> sample=a.wav").expect("sfz");
    for path in [text, seven, lone] {
        match manager.import(&path) {
            Err(BankError::Unsupported { message }) => assert!(!message.is_empty()),
            other => panic!("expected a refusal for {}, got {other:?}", path.display()),
        }
    }
    let empty = dir.path().join("empty");
    fs::create_dir(&empty).expect("folder");
    fs::write(empty.join("readme.txt"), "nothing").expect("file");
    assert!(matches!(
        manager.import(&empty),
        Err(BankError::Unsupported { .. })
    ));
    assert_clean(&manager);
}

#[cfg(unix)]
#[test]
fn a_symlink_inside_an_imported_folder_is_refused() {
    let dir = scratch_dir();
    let source = sfz_without_sidecar(dir.path());
    std::os::unix::fs::symlink("/etc/hostname", source.join("Samples/link.wav")).expect("link");
    let mut manager = manager_in(dir.path());
    assert_eq!(refused(manager.import(&source)), UnsafeBank::BankSymlink);
    assert_clean(&manager);
}

#[cfg(unix)]
#[test]
fn a_symlinked_folder_inside_an_import_is_refused() {
    let dir = scratch_dir();
    let source = sfz_without_sidecar(dir.path());
    std::os::unix::fs::symlink("/etc", source.join("Programs/etc")).expect("link");
    let mut manager = manager_in(dir.path());
    assert_eq!(refused(manager.import(&source)), UnsafeBank::BankSymlink);
    assert_clean(&manager);
}

#[cfg(unix)]
#[test]
fn a_special_file_inside_an_imported_folder_is_refused() {
    let dir = scratch_dir();
    let source = sfz_without_sidecar(dir.path());
    let socket = std::os::unix::net::UnixListener::bind(source.join("Samples/socket"))
        .expect("a socket file");
    let mut manager = manager_in(dir.path());
    assert_eq!(refused(manager.import(&source)), UnsafeBank::SpecialFile);
    drop(socket);
    assert_clean(&manager);
}

fn folder_of(dir: &Path, files: &[(&str, usize)]) -> PathBuf {
    let root = dir.join("bank");
    for (name, length) in files {
        let path = root.join(name);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, vec![7u8; *length]).expect("file");
    }
    root
}

#[test]
fn imports_stop_at_the_named_limits() {
    let cases: [(&[(&str, usize)], UnsafeBank); 5] = [
        (
            &[("a.sfz", 500), ("b.wav", 501)],
            UnsafeBank::BankTooLarge { limit_mib: 0 },
        ),
        (
            &[("a.sfz", 601)],
            UnsafeBank::BankFileTooLarge { limit_mib: 0 },
        ),
        (
            &[("a.sfz", 1), ("b", 1), ("c", 1), ("d", 1), ("e", 1)],
            UnsafeBank::TooManyFiles { limit: 4 },
        ),
        (
            &[("a/b/c/d.sfz", 1)],
            UnsafeBank::FolderTooDeep { limit: 2 },
        ),
        (
            &[("a-very-long-file-name-that-goes-on-and-on.sfz", 1)],
            UnsafeBank::NameTooLong { limit: 40 },
        ),
    ];
    for (files, reason) in cases {
        let dir = scratch_dir();
        let source = folder_of(dir.path(), files);
        let mut manager = manager_in(dir.path()).with_limits(small_limits());
        assert_eq!(refused(manager.import(&source)), reason, "{files:?}");
        assert_clean(&manager);
    }
}

fn zip_file(dir: &Path, items: &[ZipItem]) -> PathBuf {
    let path = dir.join("hostile.zip");
    fs::write(&path, zip_archive(items)).expect("zip");
    path
}

#[test]
fn zip_entries_escaping_the_bank_folder_are_refused() {
    let names = [
        "../evil.sfz",
        "/evil.sfz",
        "a/../../evil.sfz",
        "a/b/../../../evil.sfz",
        "..\\evil.sfz",
        "C:/evil.sfz",
        "C:evil.sfz",
        "\\evil.sfz",
    ];
    for name in names {
        let dir = scratch_dir();
        let archive = zip_file(
            dir.path(),
            &[
                ZipItem::File {
                    name: String::from("ok.sfz"),
                    data: b"<region> sample=a.wav".to_vec(),
                },
                ZipItem::File {
                    name: name.to_owned(),
                    data: b"escaped".to_vec(),
                },
            ],
        );
        let mut manager = manager_in(dir.path());
        assert_eq!(
            refused(manager.import(&archive)),
            UnsafeBank::ArchiveEscape,
            "{name}"
        );
        assert_clean(&manager);
        assert!(!dir.path().join("evil.sfz").exists());
        assert!(!dir.path().join("data/evil.sfz").exists());
        assert!(!dir.path().join("data/staging/evil.sfz").exists());
    }
}

#[test]
fn zip_symlink_entries_are_refused() {
    let dir = scratch_dir();
    let archive = zip_file(
        dir.path(),
        &[
            ZipItem::File {
                name: String::from("ok.sfz"),
                data: b"<region> sample=a.wav".to_vec(),
            },
            ZipItem::Link {
                name: String::from("a.wav"),
                target: String::from("/etc/passwd"),
            },
        ],
    );
    let mut manager = manager_in(dir.path());
    assert_eq!(refused(manager.import(&archive)), UnsafeBank::ArchiveLink);
    assert_clean(&manager);
}

#[test]
fn zip_bombs_stop_at_the_declared_limits() {
    let big = |name: &str, length: usize| ZipItem::File {
        name: name.to_owned(),
        data: vec![0u8; length],
    };
    let cases = [
        (
            vec![big("a.sfz", 550), big("b.wav", 550)],
            UnsafeBank::BankTooLarge { limit_mib: 0 },
        ),
        (
            vec![big("a.sfz", 5_000)],
            UnsafeBank::BankFileTooLarge { limit_mib: 0 },
        ),
        (
            (0..5)
                .map(|index| big(&format!("{index}.sfz"), 1))
                .collect(),
            UnsafeBank::TooManyFiles { limit: 4 },
        ),
        (
            vec![big("a/b/c/d.sfz", 1)],
            UnsafeBank::FolderTooDeep { limit: 2 },
        ),
    ];
    for (items, reason) in cases {
        let dir = scratch_dir();
        let archive = zip_file(dir.path(), &items);
        let mut manager = manager_in(dir.path()).with_limits(small_limits());
        assert_eq!(refused(manager.import(&archive)), reason);
        assert_clean(&manager);
    }
}

#[test]
fn bank_ids_follow_the_naming_rule() {
    for good in ["a", "musescore-general", "bank-2", "x9"] {
        assert_eq!(BankId::new(good).expect(good).as_str(), good);
    }
    for bad in [
        "",
        "-a",
        "a-",
        "a--b",
        "A",
        "a b",
        "a/b",
        "../a",
        "built-in",
        &"a".repeat(65),
    ] {
        assert!(BankId::new(bad).is_err(), "{bad:?}");
    }
    assert_eq!(BankRef::parse("built-in"), Ok(BankRef::BuiltIn));
    assert_eq!(
        BankRef::parse("x").map(|bank| bank.to_string()),
        Ok(String::from("x"))
    );
}

#[test]
fn the_catalogue_lists_the_nine_verified_banks() {
    let entries = default_catalogue();
    let ids: Vec<&str> = entries.iter().map(|entry| entry.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "musescore-general",
            "spanish-guitar",
            "fsbs-clean",
            "finger-bass-yr",
            "muldjordkit",
            "emilyguitar",
            "lately-bass",
            "sams-sonor",
            "meatbass"
        ]
    );
    for entry in &entries {
        assert!(entry.url.starts_with("https://"), "{}", entry.id);
        assert_eq!(check_download_url(&entry.url), Ok(()));
        assert_eq!(entry.sha256.len(), 64);
        assert!(
            entry
                .sha256
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert!(entry.size > 1_000_000);
        assert!(entry.licence_text.len() > 1_000, "{}", entry.id);
        assert!(entry.licence_url.starts_with("https://"));
    }
    let attributed: Vec<&str> = entries
        .iter()
        .filter(|entry| entry.attribution.is_some())
        .map(|entry| entry.id.as_str())
        .collect();
    assert_eq!(
        attributed,
        ["musescore-general", "muldjordkit", "sams-sonor"]
    );
    let general = &entries[0];
    assert_eq!(general.size, 215_614_036);
    assert_eq!(
        general.sha256,
        "ee51d2c4b1525e70f19a45909c4fd7a2e26d91d115fa89dbf5a6bc413d8b9bf3"
    );
    assert!(general.licence_text.contains("Frank Wen"));
    assert!(
        entries
            .iter()
            .any(|entry| entry.archive == ArchiveKind::SevenZip)
    );
    let default = BankManager::open(&scratch_dir().path().join("data")).expect("opens");
    assert_eq!(default.catalogue(), entries.as_slice());
}

#[test]
fn the_program_map_heuristic_names_families_and_drums() {
    let files = vec![
        String::from("Bassoon.sfz"),
        String::from("Fretless Bass.sfz"),
        String::from("Finger Bass.sfz"),
        String::from("Nylon Guitar.sfz"),
        String::from("Steel Guitar.sfz"),
        String::from("Rock Kit.sfz"),
    ];
    let record = sidecar_for(&files);
    let program = |number: u8| {
        record
            .programs
            .iter()
            .find(|entry| entry.program == number)
            .map(|entry| entry.file.as_str())
    };
    assert_eq!(program(24), Some("Nylon Guitar.sfz"));
    assert_eq!(program(25), Some("Steel Guitar.sfz"));
    assert_eq!(program(30), Some("Nylon Guitar.sfz"));
    assert_eq!(program(32), Some("Fretless Bass.sfz"));
    assert_eq!(program(33), Some("Finger Bass.sfz"));
    assert_eq!(program(35), Some("Fretless Bass.sfz"));
    assert_eq!(program(64), Some("Bassoon.sfz"));
    assert_eq!(program(0), None);
    assert_eq!(record.drums.as_deref(), Some("Rock Kit.sfz"));
    assert_eq!(record.default.as_deref(), Some("Bassoon.sfz"));
}
