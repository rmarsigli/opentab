use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use opentab_audio::{
    ArchiveKind, BankError, BankFormat, BankId, BankManager, BankRef, BankSource, CatalogueEntry,
    DAMAGED_SUFFIX, ImportLimits, InstalledBank, PREFERENCES_NAME, REGISTRY_NAME, SIDECAR_NAME,
    UnsafeBank, check_download_url, default_catalogue, sidecar_for,
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

fn refused(result: Result<InstalledBank, BankError>) -> UnsafeBank {
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
        ..ImportLimits::DEFAULT
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

#[derive(Debug, Clone, Copy)]
struct SourceFile {
    name: &'static str,
    bytes: usize,
}

const fn file(name: &'static str, bytes: usize) -> SourceFile {
    SourceFile { name, bytes }
}

const REGION: &[u8] = b"<region> sample=b.wav\n";

fn folder_of(dir: &Path, files: &[SourceFile]) -> PathBuf {
    let root = dir.join("bank");
    for item in files {
        let path = root.join(item.name);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        let mut data = vec![b' '; item.bytes];
        if item.name.ends_with(".sfz") && item.bytes >= REGION.len() {
            data[..REGION.len()].copy_from_slice(REGION);
        }
        fs::write(path, data).expect("file");
    }
    root
}

struct LimitCase {
    files: Vec<SourceFile>,
    reason: UnsafeBank,
}

#[test]
fn imports_stop_at_the_named_limits() {
    let cases = [
        LimitCase {
            files: vec![file("a.sfz", 500), file("b.wav", 501)],
            reason: UnsafeBank::BankTooLarge { limit_mib: 0 },
        },
        LimitCase {
            files: vec![file("a.sfz", 601)],
            reason: UnsafeBank::BankFileTooLarge { limit_mib: 0 },
        },
        LimitCase {
            files: vec![
                file("a.sfz", 1),
                file("b", 1),
                file("c", 1),
                file("d", 1),
                file("e", 1),
            ],
            reason: UnsafeBank::TooManyFiles { limit: 4 },
        },
        LimitCase {
            files: vec![file("a/b/c/d.sfz", 1)],
            reason: UnsafeBank::FolderTooDeep { limit: 2 },
        },
        LimitCase {
            files: vec![file("a-very-long-file-name-that-goes-on-and-on.sfz", 1)],
            reason: UnsafeBank::NameTooLong { limit: 40 },
        },
    ];
    for case in cases {
        let dir = scratch_dir();
        let source = folder_of(dir.path(), &case.files);
        let mut manager = manager_in(dir.path()).with_limits(small_limits());
        assert_eq!(
            refused(manager.import(&source)),
            case.reason,
            "{:?}",
            case.files
        );
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

struct ZipCase {
    items: Vec<ZipItem>,
    reason: UnsafeBank,
}

#[test]
fn zip_bombs_stop_at_the_declared_limits() {
    let big = |name: &str, length: usize| ZipItem::File {
        name: name.to_owned(),
        data: vec![0u8; length],
    };
    let cases = [
        ZipCase {
            items: vec![big("a.sfz", 550), big("b.wav", 550)],
            reason: UnsafeBank::BankTooLarge { limit_mib: 0 },
        },
        ZipCase {
            items: vec![big("a.sfz", 5_000)],
            reason: UnsafeBank::BankFileTooLarge { limit_mib: 0 },
        },
        ZipCase {
            items: (0..5)
                .map(|index| big(&format!("{index}.sfz"), 1))
                .collect(),
            reason: UnsafeBank::TooManyFiles { limit: 4 },
        },
        ZipCase {
            items: vec![big("a/b/c/d.sfz", 1)],
            reason: UnsafeBank::FolderTooDeep { limit: 2 },
        },
    ];
    for case in cases {
        let dir = scratch_dir();
        let archive = zip_file(dir.path(), &case.items);
        let mut manager = manager_in(dir.path()).with_limits(small_limits());
        assert_eq!(refused(manager.import(&archive)), case.reason);
        assert_clean(&manager);
    }
}

#[test]
fn bank_ids_follow_the_naming_rule() {
    for good in [
        "a",
        "musescore-general",
        "bank-2",
        "x9",
        "console",
        "com10",
        "aux-bank",
    ] {
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
        "con",
        "nul",
        "aux",
        "prn",
        "com1",
        "com9",
        "lpt1",
        "lpt9",
    ] {
        assert!(BankId::new(bad).is_err(), "{bad:?}");
    }
    assert_eq!(BankRef::parse("built-in"), Ok(BankRef::BuiltIn));
    assert_eq!(
        BankRef::parse("x").map(|bank| bank.to_string()),
        Ok(String::from("x"))
    );
}

struct Pinned {
    id: &'static str,
    url: &'static str,
    size: u64,
    sha256: &'static str,
}

const PINNED: [Pinned; 9] = [
    Pinned {
        id: "musescore-general",
        url: "https://ftp.osuosl.org/pub/musescore/soundfont/MuseScore_General/MuseScore_General.sf2",
        size: 215_614_036,
        sha256: "ee51d2c4b1525e70f19a45909c4fd7a2e26d91d115fa89dbf5a6bc413d8b9bf3",
    },
    Pinned {
        id: "spanish-guitar",
        url: "https://github.com/freepats/spanish-classical-guitar/releases/download/v1.0.0/SpanishClassicalGuitar-20190618.zip",
        size: 4_943_846,
        sha256: "173f775222991061291767724d57c7c21242a3c3254407b266aabd548df23dbb",
    },
    Pinned {
        id: "fsbs-clean",
        url: "https://github.com/freepats/electric-guitar-FSBS-clean/releases/download/2026-08-07/EGuitarFSBS-clean-bridge-small-SFZ%2BFLAC-20260807.7z",
        size: 3_096_261,
        sha256: "07c8cdf94dc9b4875ff6c43e51440d84f939a45530060c0761093a56d4a1f032",
    },
    Pinned {
        id: "finger-bass-yr",
        url: "https://github.com/freepats/electric-bass-YR/releases/download/2019-09-30/FingerBassYR-SFZ%2BFLAC-20190930.7z",
        size: 3_252_347,
        sha256: "36f87ec9eb086ef25050312522aacf71213c582f46989e0c5a12681304596587",
    },
    Pinned {
        id: "muldjordkit",
        url: "https://github.com/freepats/muldjordkit/releases/download/2020-10-18/MuldjordKit-SF2-20201018.7z",
        size: 54_877_940,
        sha256: "aef4c9e0a96f9a899a7838f65ff07565f6af247bbb4ff31e24dba3331a671a96",
    },
    Pinned {
        id: "emilyguitar",
        url: "https://github.com/sfzinstruments/karoryfer.emilyguitar/releases/download/v1.001/Karoryfer.Emilyguitar.v1.001.zip",
        size: 103_484_350,
        sha256: "ffef3b28972763820bee73660a25cf47ea6e13b184342fd5d325a5631598cacb",
    },
    Pinned {
        id: "lately-bass",
        url: "https://github.com/freepats/lately-bass/releases/download/2024-04-09/LatelyBass-SFZ%2BFLAC-20240409.7z",
        size: 1_181_315,
        sha256: "3cb6d4081b39cea265011a21f05a424fc72845abfd17d7409ac16acc7122a3e0",
    },
    Pinned {
        id: "sams-sonor",
        url: "https://github.com/sfzinstruments/SamsSonor/archive/ef3e32058924ed1c3335e86d03094d3f310a9104.zip",
        size: 36_203_868,
        sha256: "fcbca4dd398666f2908995218394c3af938817580304316f618dead41eaaff2c",
    },
    Pinned {
        id: "meatbass",
        url: "https://github.com/sfzinstruments/karoryfer.meatbass/releases/download/v1.001/Karoryfer.Meatbass.v1.001.zip",
        size: 255_425_351,
        sha256: "bc053061d4f39fb76ba56bc1f323efa228d1f63757c331738812479bcf04fe96",
    },
];

#[test]
fn the_catalogue_lists_the_nine_verified_banks() {
    let entries = default_catalogue();
    assert_eq!(entries.len(), PINNED.len());
    for (entry, pinned) in entries.iter().zip(PINNED.iter()) {
        assert_eq!(entry.id.as_str(), pinned.id);
        assert_eq!(entry.url, pinned.url, "{}", pinned.id);
        assert_eq!(entry.size, pinned.size, "{}", pinned.id);
        assert_eq!(entry.sha256, pinned.sha256, "{}", pinned.id);
        assert_eq!(check_download_url(&entry.url), Ok(()));
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
    assert!(entries[0].licence_text.contains("Frank Wen"));
    let sevens: Vec<&str> = entries
        .iter()
        .filter(|entry| entry.archive == ArchiveKind::SevenZip)
        .map(|entry| entry.id.as_str())
        .collect();
    assert_eq!(
        sevens,
        ["fsbs-clean", "finger-bass-yr", "muldjordkit", "lately-bass"]
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

fn bank_files(manager: &BankManager, bank: &InstalledBank) -> BTreeMap<String, Vec<u8>> {
    tree(&manager.root().join("banks").join(bank.id.as_str()))
}

#[test]
fn a_folder_import_copies_only_the_sf2_it_finds() {
    let dir = scratch_dir();
    let downloads = dir.path().join("Downloads");
    fs::create_dir_all(downloads.join("photos")).expect("folders");
    fs::write(downloads.join("notes.txt"), "private notes").expect("notes");
    fs::write(downloads.join("photos/cat.jpg"), vec![9u8; 4_000]).expect("photo");
    write_tiny_sf2(&downloads, "My Bank.sf2");
    fs::write(downloads.join("._My Bank.sf2"), [0x00, 0x05, 0x16, 0x07]).expect("metadata");
    let mut manager = manager_in(dir.path());
    let bank = manager.import(&downloads).expect("the bank imports");
    assert_eq!(bank.format, BankFormat::Sf2);
    let copied = bank_files(&manager, &bank);
    assert_eq!(
        copied.keys().cloned().collect::<Vec<String>>(),
        [String::from("My Bank.sf2")]
    );
    assert_eq!(copied["My Bank.sf2"], tiny_bytes());
    assert_eq!(bank.bytes, tiny_bytes().len() as u64);
}

#[test]
fn a_folder_import_copies_only_the_files_the_sfz_instruments_reference() {
    let dir = scratch_dir();
    let source = sfz_without_sidecar(dir.path());
    let expected = tree(&source);
    fs::write(source.join("notes.pdf"), vec![1u8; 3_000]).expect("pdf");
    fs::create_dir_all(source.join("private")).expect("private");
    fs::write(source.join("private/letter.txt"), "dear").expect("letter");
    fs::write(source.join("Samples/unused.wav"), vec![2u8; 100]).expect("unused");
    let mut manager = manager_in(dir.path());
    let bank = manager.import(&source).expect("the bank imports");
    assert_eq!(bank.format, BankFormat::Sfz);
    let mut copied = tree(&bank.path);
    assert!(copied.remove(SIDECAR_NAME).is_some());
    assert_eq!(copied, expected);
    let bytes: u64 = expected.values().map(|data| data.len() as u64).sum();
    assert_eq!(bank.bytes, bytes);
}

#[cfg(unix)]
#[test]
fn an_over_budget_folder_is_refused_before_anything_is_copied() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch_dir();
    let source = folder_of(
        dir.path(),
        &[
            file("a.sfz", 300),
            file("b.wav", 400),
            file("unrelated.txt", 50),
        ],
    );
    let instrument = b"<region> sample=a-locked.wav\n<region> sample=b.wav\n";
    let mut text = vec![b' '; 300];
    text[..instrument.len()].copy_from_slice(instrument);
    fs::write(source.join("a.sfz"), text).expect("instrument");
    let locked = source.join("a-locked.wav");
    fs::write(&locked, vec![3u8; 400]).expect("locked");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("chmod");
    let mut manager = manager_in(dir.path()).with_limits(small_limits());
    let outcome = manager.import(&source);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o644)).expect("chmod back");
    assert_eq!(refused(outcome), UnsafeBank::BankTooLarge { limit_mib: 0 });
    assert_clean(&manager);
}

#[test]
fn an_import_named_after_a_windows_device_gets_a_portable_id() {
    let dir = scratch_dir();
    let source = write_tiny_sf2(dir.path(), "Aux.sf2");
    let mut manager = manager_in(dir.path());
    let bank = manager.import(&source).expect("imports");
    assert_eq!(bank.id.as_str(), "aux-bank");
}

#[test]
fn zip_entries_with_names_windows_reserves_are_refused() {
    for name in [
        "NUL",
        "Samples/con.wav",
        "aux",
        "COM1.sfz",
        "lpt9",
        "prn.txt",
        "Samples/a.wav.",
        "Samples/a.wav ",
        "folder./a.sfz",
    ] {
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
                    data: b"data".to_vec(),
                },
            ],
        );
        let mut manager = manager_in(dir.path());
        assert_eq!(
            refused(manager.import(&archive)),
            UnsafeBank::ReservedName,
            "{name:?}"
        );
        assert_clean(&manager);
    }
}

#[test]
fn a_macos_zip_imports_without_its_metadata_files() {
    let dir = scratch_dir();
    let source = sfz_without_sidecar(dir.path());
    let mut items: Vec<ZipItem> = tree(&source)
        .into_iter()
        .map(|(name, data)| ZipItem::File {
            name: format!("Tiny/{name}"),
            data,
        })
        .collect();
    for name in [
        "__MACOSX/Tiny/Programs/._guitar.sfz",
        "__MACOSX/._Tiny",
        "Tiny/Programs/._bass.sfz",
        "Tiny/._Samples",
    ] {
        items.push(ZipItem::File {
            name: name.to_owned(),
            data: vec![0x00, 0x05, 0x16, 0x07, 0xFF, 0xFE],
        });
    }
    let archive = dir.path().join("mac.zip");
    fs::write(&archive, zip_archive(&items)).expect("zip");
    let mut manager = manager_in(dir.path());
    let bank = manager.import(&archive).expect("imports");
    assert!(bank.path.ends_with("Tiny"));
    let mut copied = tree(&bank.path);
    assert!(copied.remove(SIDECAR_NAME).is_some());
    assert_eq!(copied, tree(&source));
}

#[test]
fn a_damaged_registry_is_set_aside_and_the_manager_opens_empty() {
    let dir = scratch_dir();
    let source = write_tiny_sf2(dir.path(), "kept.sf2");
    let mut manager = manager_in(dir.path());
    manager.import(&source).expect("imports");
    let registry = manager.root().join(REGISTRY_NAME);
    let preferences = manager.root().join(PREFERENCES_NAME);
    fs::write(&registry, b"{ not json").expect("damaged");
    fs::write(&preferences, b"{\"version\": 99, \"families\": {}}").expect("future");
    let reopened = manager_in(dir.path());
    assert!(reopened.list().is_empty());
    assert!(reopened.assignments().is_empty());
    let problems = reopened.problems();
    assert_eq!(problems.len(), 2, "{problems:?}");
    let aside = |path: &Path| {
        let mut name = path.as_os_str().to_owned();
        name.push(DAMAGED_SUFFIX);
        PathBuf::from(name)
    };
    assert_eq!(problems[0].path, registry);
    assert_eq!(problems[0].moved_to, Some(aside(&registry)));
    assert_eq!(problems[1].moved_to, Some(aside(&preferences)));
    assert!(problems.iter().all(|problem| !problem.message.is_empty()));
    assert_eq!(fs::read(aside(&registry)).expect("kept"), b"{ not json");
    assert!(!registry.exists());
    fs::write(&registry, b"[]").expect("damaged again");
    let again = manager_in(dir.path());
    let mut second = registry.as_os_str().to_owned();
    second.push(DAMAGED_SUFFIX);
    second.push("-2");
    assert_eq!(again.problems()[0].moved_to, Some(PathBuf::from(second)));
    assert_eq!(
        fs::read(aside(&registry)).expect("first kept"),
        b"{ not json"
    );
    let mut fresh = manager_in(dir.path());
    assert!(fresh.problems().is_empty());
    fresh.import(&source).expect("imports again");
    assert_eq!(manager_in(dir.path()).list().len(), 1);
}

#[test]
fn a_zip_that_unpacks_far_beyond_its_size_is_refused() {
    let dir = scratch_dir();
    let archive = zip_file(
        dir.path(),
        &[
            ZipItem::File {
                name: String::from("a.sfz"),
                data: b"<region> sample=a.wav".to_vec(),
            },
            ZipItem::File {
                name: String::from("a.wav"),
                data: vec![0u8; 200_000],
            },
        ],
    );
    let packed = fs::metadata(&archive).expect("zip").len();
    assert!(packed * 20 < 200_000, "{packed} bytes packed");
    let limits = ImportLimits {
        archive_ratio: 20,
        ratio_floor_bytes: 1_000,
        ..ImportLimits::DEFAULT
    };
    let mut manager = manager_in(dir.path()).with_limits(limits);
    assert_eq!(
        refused(manager.import(&archive)),
        UnsafeBank::ArchiveRatio { ratio: 20 }
    );
    assert_clean(&manager);
}
