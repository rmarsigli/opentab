use std::fs;
use std::time::Duration;

use opentab_audio::{
    ArchiveKind, BankError, BankFormat, BankId, BankManager, BankSource, DownloadTimeouts,
    InstallCancel, InstallError, InstallProgress, InstallStage, InstalledBank, UnsafeBank,
    check_download_url,
};
use opentab_testkit::archives::{
    SevenFile, SevenFolder, SevenMethod, UNIX_LINK_ATTRIBUTES, seven_zip,
};
use opentab_testkit::http::{ClosedPort, Declared, Ending, Header, LoopbackServer, Pace, Reply};

use crate::bank_manager::{
    assert_clean, entry_for, listing, manager_in, manager_with, sfz_without_sidecar, tree, zip_tree,
};
use crate::support::{scratch_dir, tiny_bytes};

const PATH: &str = "/bank.bin";

fn id(text: &str) -> BankId {
    BankId::new(text).expect("an id")
}

fn ignore(_: InstallProgress) {}

struct Attempt {
    dir: tempfile::TempDir,
    result: Result<InstalledBank, InstallError>,
    manager: BankManager,
}

fn install_with(
    reply: Reply,
    archive: ArchiveKind,
    format: BankFormat,
    expected: &[u8],
    timeouts: DownloadTimeouts,
) -> Attempt {
    let dir = scratch_dir();
    let server = LoopbackServer::start(vec![reply]);
    let entry = entry_for("test-bank", format, archive, &server.url(PATH), expected);
    let mut manager = manager_with(dir.path(), vec![entry]).with_download_timeouts(timeouts);
    let result = manager.install(&id("test-bank"), &mut ignore);
    drop(server);
    Attempt {
        dir,
        result,
        manager,
    }
}

fn install_from(
    reply: Reply,
    archive: ArchiveKind,
    format: BankFormat,
    expected: &[u8],
) -> Attempt {
    install_with(reply, archive, format, expected, DownloadTimeouts::DEFAULT)
}

#[test]
fn a_raw_sf2_installs_from_the_loopback_server_with_progress() {
    let dir = scratch_dir();
    let server = LoopbackServer::start(vec![Reply::ok(PATH, tiny_bytes().to_vec())]);
    let entry = entry_for(
        "tiny",
        BankFormat::Sf2,
        ArchiveKind::Raw,
        &server.url(PATH),
        tiny_bytes(),
    );
    let mut manager = manager_with(dir.path(), vec![entry.clone()]);
    let mut seen: Vec<InstallProgress> = Vec::new();
    let bank = manager
        .install(&id("tiny"), &mut |progress| seen.push(progress))
        .expect("installs");
    assert_eq!(bank.id, entry.id);
    assert_eq!(bank.name, entry.name);
    assert_eq!(bank.format, BankFormat::Sf2);
    assert_eq!(bank.source, BankSource::Catalogue);
    assert_eq!(fs::read(&bank.path).expect("installed"), tiny_bytes());
    assert_eq!(manager.list(), vec![bank.clone()]);
    assert_eq!(listing(&manager.staging_dir()), Vec::<String>::new());
    let size = tiny_bytes().len() as u64;
    let downloads: Vec<u64> = seen
        .iter()
        .filter(|progress| progress.stage == InstallStage::Downloading)
        .map(|progress| progress.done)
        .collect();
    assert!(downloads.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(downloads.last(), Some(&size));
    let stages: Vec<InstallStage> = seen
        .iter()
        .map(|progress| progress.stage)
        .filter(|stage| *stage != InstallStage::Downloading)
        .collect();
    assert_eq!(
        stages,
        [
            InstallStage::Verifying,
            InstallStage::Unpacking,
            InstallStage::Registering
        ]
    );
    assert!(seen.iter().all(|progress| progress.total == size));
    let again = manager
        .install(&id("tiny"), &mut ignore)
        .expect("already there");
    assert_eq!(again, bank);
    assert_eq!(server.requests(), vec![String::from(PATH)]);
}

#[test]
fn a_hash_mismatch_refuses_and_leaves_nothing() {
    let mut wrong = tiny_bytes().to_vec();
    let last = wrong.len() - 1;
    wrong[last] ^= 0x55;
    let Attempt {
        dir: _dir,
        result,
        manager,
    } = install_from(
        Reply::ok(PATH, wrong),
        ArchiveKind::Raw,
        BankFormat::Sf2,
        tiny_bytes(),
    );
    match result {
        Err(InstallError::HashMismatch { expected, actual }) => assert_ne!(expected, actual),
        other => panic!("expected a hash mismatch, got {other:?}"),
    }
    assert_clean(&manager);
}

#[test]
fn a_refused_connection_is_an_offline_error() {
    let dir = scratch_dir();
    let closed = ClosedPort::reserve();
    let entry = entry_for(
        "offline",
        BankFormat::Sf2,
        ArchiveKind::Raw,
        &closed.url(PATH),
        tiny_bytes(),
    );
    let mut manager = manager_with(dir.path(), vec![entry]);
    match manager.install(&id("offline"), &mut ignore) {
        Err(InstallError::Offline { message }) => assert!(!message.is_empty()),
        other => panic!("expected offline, got {other:?}"),
    }
    assert_clean(&manager);
}

#[test]
fn a_truncated_body_refuses_and_leaves_nothing() {
    let size = tiny_bytes().len();
    for declared in [Declared::Exact, Declared::Absent] {
        let reply = Reply {
            path: PATH.to_owned(),
            status: 200,
            body: tiny_bytes().to_vec(),
            declared,
            cut: Some(size / 2),
            ..Reply::ok(PATH, Vec::new())
        };
        let Attempt {
            dir: _dir,
            result,
            manager,
        } = install_from(reply, ArchiveKind::Raw, BankFormat::Sf2, tiny_bytes());
        match result {
            Err(InstallError::Truncated { expected, received }) => {
                assert_eq!(expected, size as u64);
                assert!(received <= (size / 2) as u64, "{declared:?}");
            }
            other => panic!("expected a truncated download for {declared:?}, got {other:?}"),
        }
        assert_clean(&manager);
    }
}

#[test]
fn a_body_longer_than_the_catalogue_size_refuses() {
    let mut longer = tiny_bytes().to_vec();
    longer.extend_from_slice(&[0u8; 100]);
    for declared in [Declared::Exact, Declared::Absent] {
        let reply = Reply {
            path: PATH.to_owned(),
            status: 200,
            body: longer.clone(),
            declared,
            cut: None,
            ..Reply::ok(PATH, Vec::new())
        };
        let Attempt {
            dir: _dir,
            result,
            manager,
        } = install_from(reply, ArchiveKind::Raw, BankFormat::Sf2, tiny_bytes());
        assert_eq!(
            result,
            Err(InstallError::TooLarge {
                expected: tiny_bytes().len() as u64
            }),
            "{declared:?}"
        );
        assert_clean(&manager);
    }
}

#[test]
fn a_shorter_declared_length_refuses_before_the_body() {
    let reply = Reply {
        path: PATH.to_owned(),
        status: 200,
        body: tiny_bytes().to_vec(),
        declared: Declared::Bytes(10),
        cut: Some(10),
        ..Reply::ok(PATH, Vec::new())
    };
    let Attempt {
        dir: _dir,
        result,
        manager,
    } = install_from(reply, ArchiveKind::Raw, BankFormat::Sf2, tiny_bytes());
    assert!(matches!(
        result,
        Err(InstallError::Truncated { received: 10, .. })
    ));
    assert_clean(&manager);
}

#[test]
fn a_server_error_is_reported_with_its_status() {
    let reply = Reply {
        path: PATH.to_owned(),
        status: 404,
        body: b"missing".to_vec(),
        declared: Declared::Exact,
        cut: None,
        ..Reply::ok(PATH, Vec::new())
    };
    let Attempt {
        dir: _dir,
        result,
        manager,
    } = install_from(reply, ArchiveKind::Raw, BankFormat::Sf2, tiny_bytes());
    assert_eq!(result, Err(InstallError::Http { status: 404 }));
    assert_clean(&manager);
}

#[test]
fn plain_http_is_accepted_only_for_loopback_hosts() {
    for allowed in [
        "https://example.org/a.sf2",
        "http://127.0.0.1:9/a.sf2",
        "http://127.8.9.10/a.sf2",
        "http://[::1]:9/a.sf2",
    ] {
        assert_eq!(check_download_url(allowed), Ok(()), "{allowed}");
    }
    for refused in [
        "http://example.org/a.sf2",
        "http://localhost:8080/a.sf2",
        "http://LOCALHOST/a.sf2",
        "http://localhost./a.sf2",
        "http://10.0.0.1/a.sf2",
        "http://0.0.0.0/a.sf2",
        "http://localhost.example.org/a.sf2",
        "http://127.0.0.1.example.org/a.sf2",
        "http://127.0.0.1@example.org/a.sf2",
        "http://[::ffff:127.0.0.1]/a.sf2",
        "ftp://127.0.0.1/a.sf2",
        "file:///etc/passwd",
    ] {
        assert!(
            matches!(
                check_download_url(refused),
                Err(InstallError::Insecure { .. } | InstallError::Address { .. })
            ),
            "{refused}"
        );
    }
}

#[test]
fn an_insecure_catalogue_entry_is_refused_before_any_connection() {
    let dir = scratch_dir();
    let entry = entry_for(
        "plain",
        BankFormat::Sf2,
        ArchiveKind::Raw,
        "http://example.org/a.sf2",
        tiny_bytes(),
    );
    let mut manager = manager_with(dir.path(), vec![entry]);
    assert!(matches!(
        manager.install(&id("plain"), &mut ignore),
        Err(InstallError::Insecure { .. })
    ));
    assert_eq!(
        manager.install(&id("unknown"), &mut ignore),
        Err(InstallError::Unknown {
            id: String::from("unknown")
        })
    );
    assert_clean(&manager);
}

#[test]
fn a_zip_sfz_bank_installs_with_its_program_map() {
    let source_dir = scratch_dir();
    let source = sfz_without_sidecar(source_dir.path());
    let archive = zip_tree(&source, "Tiny/");
    let Attempt {
        dir: _dir,
        result,
        manager,
    } = install_from(
        Reply::ok(PATH, archive.clone()),
        ArchiveKind::Zip,
        BankFormat::Sfz,
        &archive,
    );
    let bank = result.expect("installs");
    assert_eq!(bank.format, BankFormat::Sfz);
    let mut installed = tree(&bank.path);
    assert!(installed.remove(opentab_audio::SIDECAR_NAME).is_some());
    assert_eq!(installed, tree(&source));
    assert_eq!(listing(&manager.staging_dir()), Vec::<String>::new());
}

#[test]
fn a_catalogue_format_mismatch_refuses() {
    let Attempt {
        dir: _dir,
        result,
        manager,
    } = install_from(
        Reply::ok(PATH, tiny_bytes().to_vec()),
        ArchiveKind::Raw,
        BankFormat::Sfz,
        tiny_bytes(),
    );
    assert!(matches!(
        result,
        Err(InstallError::Bank(BankError::Invalid { .. }))
    ));
    assert_clean(&manager);
}

fn seven_files(source: &std::path::Path) -> Vec<SevenFolder> {
    let files: Vec<SevenFile> = tree(source)
        .into_iter()
        .map(|(name, data)| SevenFile {
            name: format!("Seven/{name}"),
            data,
            attributes: None,
            declared: None,
        })
        .collect();
    let methods = [
        SevenMethod::Deflate,
        SevenMethod::Bzip2,
        SevenMethod::Lzma2,
        SevenMethod::Copy,
    ];
    let mut folders: Vec<SevenFolder> = methods
        .iter()
        .map(|method| SevenFolder {
            method: *method,
            files: Vec::new(),
        })
        .collect();
    for (index, file) in files.into_iter().enumerate() {
        folders[index % methods.len()].files.push(file);
    }
    folders
}

#[test]
fn a_seven_zip_bank_with_deflate_bzip2_and_lzma2_folders_installs() {
    let source_dir = scratch_dir();
    let source = sfz_without_sidecar(source_dir.path());
    let folders = seven_files(&source);
    assert!(folders.iter().all(|folder| folder.files.len() >= 2));
    let archive = seven_zip(&folders, &[String::from("Seven/Empty")]);
    let Attempt {
        dir: _dir,
        result,
        manager,
    } = install_from(
        Reply::ok(PATH, archive.clone()),
        ArchiveKind::SevenZip,
        BankFormat::Sfz,
        &archive,
    );
    let bank = result.expect("the 7z bank installs");
    assert!(bank.path.ends_with("Seven"));
    assert!(bank.path.join("Empty").is_dir());
    let mut installed = tree(&bank.path);
    assert!(installed.remove(opentab_audio::SIDECAR_NAME).is_some());
    assert_eq!(installed, tree(&source));
    assert_eq!(listing(&manager.staging_dir()), Vec::<String>::new());
}

fn seven_with(name: &str, attributes: Option<u32>) -> Vec<u8> {
    seven_zip(
        &[SevenFolder {
            method: SevenMethod::Deflate,
            files: vec![
                SevenFile {
                    name: String::from("ok.sfz"),
                    data: b"<region> sample=a.wav".to_vec(),
                    attributes: None,
                    declared: None,
                },
                SevenFile {
                    name: name.to_owned(),
                    data: b"payload".to_vec(),
                    attributes,
                    declared: None,
                },
            ],
        }],
        &[],
    )
}

fn unsafe_reason(result: Result<InstalledBank, InstallError>) -> UnsafeBank {
    match result {
        Err(InstallError::Bank(BankError::Unsafe { reason })) => reason,
        other => panic!("expected an unsafe refusal, got {other:?}"),
    }
}

#[test]
fn seven_zip_entries_escaping_the_bank_folder_are_refused() {
    for name in [
        "../evil.sfz",
        "/evil.sfz",
        "a/../../evil.sfz",
        "..\\evil.sfz",
        "C:evil.sfz",
    ] {
        let archive = seven_with(name, None);
        let Attempt {
            dir,
            result,
            manager,
        } = install_from(
            Reply::ok(PATH, archive.clone()),
            ArchiveKind::SevenZip,
            BankFormat::Sfz,
            &archive,
        );
        assert_eq!(unsafe_reason(result), UnsafeBank::ArchiveEscape, "{name}");
        assert_clean(&manager);
        assert!(!dir.path().join("data/evil.sfz").exists());
        assert!(!dir.path().join("data/staging/evil.sfz").exists());
    }
}

#[test]
fn seven_zip_links_are_refused() {
    let archive = seven_with("a.wav", Some(UNIX_LINK_ATTRIBUTES));
    let Attempt {
        dir: _dir,
        result,
        manager,
    } = install_from(
        Reply::ok(PATH, archive.clone()),
        ArchiveKind::SevenZip,
        BankFormat::Sfz,
        &archive,
    );
    assert_eq!(unsafe_reason(result), UnsafeBank::ArchiveLink);
    assert_clean(&manager);
}

struct BombCase {
    archive: Vec<u8>,
    reason: UnsafeBank,
}

#[test]
fn seven_zip_solid_bombs_stop_at_the_limits() {
    let limits = opentab_audio::ImportLimits {
        total_bytes: 1_000,
        file_bytes: 600,
        entries: 4,
        depth: 2,
        name_bytes: 40,
        ..opentab_audio::ImportLimits::DEFAULT
    };
    let solid = |sizes: &[usize]| {
        seven_zip(
            &[SevenFolder {
                method: SevenMethod::Lzma2,
                files: sizes
                    .iter()
                    .enumerate()
                    .map(|(index, size)| SevenFile {
                        name: format!("{index}.sfz"),
                        data: vec![0u8; *size],
                        attributes: None,
                        declared: None,
                    })
                    .collect(),
            }],
            &[],
        )
    };
    let cases = [
        BombCase {
            archive: solid(&[550, 550]),
            reason: UnsafeBank::BankTooLarge { limit_mib: 0 },
        },
        BombCase {
            archive: solid(&[601]),
            reason: UnsafeBank::BankFileTooLarge { limit_mib: 0 },
        },
        BombCase {
            archive: solid(&[1, 1, 1, 1, 1]),
            reason: UnsafeBank::TooManyFiles { limit: 4 },
        },
    ];
    for BombCase { archive, reason } in cases {
        let dir = scratch_dir();
        let server = LoopbackServer::start(vec![Reply::ok(PATH, archive.clone())]);
        let entry = entry_for(
            "bomb",
            BankFormat::Sfz,
            ArchiveKind::SevenZip,
            &server.url(PATH),
            &archive,
        );
        let mut manager = manager_with(dir.path(), vec![entry]).with_limits(limits);
        assert_eq!(
            unsafe_reason(manager.install(&id("bomb"), &mut ignore)),
            reason
        );
        assert_clean(&manager);
    }
}

#[test]
fn a_reopen_clears_a_staging_folder_left_by_a_crash() {
    let dir = scratch_dir();
    let manager = manager_in(dir.path());
    let leftover = manager.staging_dir().join("1-0");
    fs::create_dir_all(&leftover).expect("leftover");
    fs::write(leftover.join("download.part"), b"partial").expect("partial");
    let reopened = manager_in(dir.path());
    assert_eq!(listing(&reopened.staging_dir()), Vec::<String>::new());
}

#[test]
fn a_reopen_keeps_a_staging_folder_another_install_still_holds() {
    let dir = scratch_dir();
    let manager = manager_in(dir.path());
    let busy = manager.staging_dir().join("2-0");
    fs::create_dir_all(&busy).expect("busy");
    let lock = fs::File::create(busy.join(opentab_audio::LOCK_NAME)).expect("lock file");
    lock.lock().expect("held");
    fs::write(busy.join("download.part"), b"partial").expect("partial");
    let reopened = manager_in(dir.path());
    assert_eq!(listing(&reopened.staging_dir()), vec![String::from("2-0")]);
    drop(lock);
    let reopened = manager_in(dir.path());
    assert_eq!(listing(&reopened.staging_dir()), Vec::<String>::new());
}

#[test]
fn an_orphan_folder_from_a_crashed_install_is_replaced() {
    let dir = scratch_dir();
    let server = LoopbackServer::start(vec![Reply::ok(PATH, tiny_bytes().to_vec())]);
    let entry = entry_for(
        "tiny",
        BankFormat::Sf2,
        ArchiveKind::Raw,
        &server.url(PATH),
        tiny_bytes(),
    );
    let mut manager = manager_with(dir.path(), vec![entry]);
    let orphan = manager.root().join("banks/tiny");
    fs::create_dir_all(&orphan).expect("orphan");
    fs::write(orphan.join("left.sf2"), b"left over").expect("left over");
    let bank = manager.install(&id("tiny"), &mut ignore).expect("installs");
    assert_eq!(listing(&orphan), vec![String::from("tiny.sf2")]);
    assert_eq!(fs::read(bank.path).expect("bank"), tiny_bytes());
}

fn install_seven(archive: Vec<u8>) -> Attempt {
    install_from(
        Reply::ok(PATH, archive.clone()),
        ArchiveKind::SevenZip,
        BankFormat::Sfz,
        &archive,
    )
}

fn one_file_seven(method: SevenMethod, declared: Option<u64>) -> Vec<u8> {
    seven_zip(
        &[SevenFolder {
            method,
            files: vec![SevenFile {
                name: String::from("a.sfz"),
                data: b"<region> sample=a.wav".to_vec(),
                attributes: None,
                declared,
            }],
        }],
        &[],
    )
}

#[test]
fn a_seven_zip_declared_size_over_the_cap_refuses_before_decoding() {
    let Attempt {
        dir: _dir,
        result,
        manager,
    } = install_seven(one_file_seven(SevenMethod::Copy, Some(2 << 30)));
    assert_eq!(
        unsafe_reason(result),
        UnsafeBank::BankFileTooLarge { limit_mib: 1024 }
    );
    assert_clean(&manager);
}

#[test]
fn a_seven_zip_method_outside_the_allowed_list_is_refused() {
    let Attempt {
        dir: _dir,
        result,
        manager,
    } = install_seven(one_file_seven(SevenMethod::Ppmd, None));
    assert!(
        matches!(
            result,
            Err(InstallError::Bank(BankError::Unsupported { .. }))
        ),
        "{result:?}"
    );
    assert_clean(&manager);
}

fn stalling(stall_millis: u64) -> DownloadTimeouts {
    DownloadTimeouts {
        stall: Duration::from_millis(stall_millis),
        ..DownloadTimeouts::DEFAULT
    }
}

#[test]
fn a_server_that_goes_quiet_after_the_headers_is_an_offline_error() {
    let size = tiny_bytes().len();
    for cut in [0, size / 2] {
        let reply = Reply {
            cut: Some(cut),
            ending: Ending::Hold,
            ..Reply::ok(PATH, tiny_bytes().to_vec())
        };
        let attempt = install_with(
            reply,
            ArchiveKind::Raw,
            BankFormat::Sf2,
            tiny_bytes(),
            stalling(200),
        );
        assert!(
            matches!(attempt.result, Err(InstallError::Offline { .. })),
            "{cut}: {:?}",
            attempt.result
        );
        assert_clean(&attempt.manager);
    }
}

#[test]
fn a_download_slower_than_its_deadline_is_an_offline_error() {
    let size = tiny_bytes().len();
    let reply = Reply {
        pace: Some(Pace {
            chunk: size.div_ceil(30),
            pause_millis: 100,
        }),
        ..Reply::ok(PATH, tiny_bytes().to_vec())
    };
    let timeouts = DownloadTimeouts {
        deadline_floor: Duration::from_millis(100),
        min_bytes_per_second: u64::MAX,
        ..DownloadTimeouts::DEFAULT
    };
    assert_eq!(timeouts.deadline(size as u64), Duration::from_millis(1_100));
    let attempt = install_with(
        reply,
        ArchiveKind::Raw,
        BankFormat::Sf2,
        tiny_bytes(),
        timeouts,
    );
    assert!(
        matches!(attempt.result, Err(InstallError::Offline { .. })),
        "{:?}",
        attempt.result
    );
    assert_clean(&attempt.manager);
}

#[test]
fn a_cancel_during_the_download_stops_it_and_leaves_nothing() {
    let dir = scratch_dir();
    let server = LoopbackServer::start(vec![Reply::ok(PATH, tiny_bytes().to_vec())]);
    let entry = entry_for(
        "tiny",
        BankFormat::Sf2,
        ArchiveKind::Raw,
        &server.url(PATH),
        tiny_bytes(),
    );
    let mut manager = manager_with(dir.path(), vec![entry]);
    let cancel = InstallCancel::new();
    let mut stages: Vec<InstallStage> = Vec::new();
    let result = manager.install_with(
        &id("tiny"),
        &mut |progress| {
            stages.push(progress.stage);
            cancel.cancel();
        },
        &cancel,
    );
    assert_eq!(result, Err(InstallError::Cancelled));
    assert_eq!(stages, vec![InstallStage::Downloading]);
    assert_clean(&manager);
    assert_eq!(server.requests(), vec![String::from(PATH)]);
}

#[test]
fn a_cancel_before_the_download_never_connects() {
    let dir = scratch_dir();
    let server = LoopbackServer::start(vec![Reply::ok(PATH, tiny_bytes().to_vec())]);
    let entry = entry_for(
        "tiny",
        BankFormat::Sf2,
        ArchiveKind::Raw,
        &server.url(PATH),
        tiny_bytes(),
    );
    let mut manager = manager_with(dir.path(), vec![entry]);
    let cancel = InstallCancel::new();
    cancel.cancel();
    assert!(cancel.is_cancelled());
    assert_eq!(
        manager.install_with(&id("tiny"), &mut ignore, &cancel),
        Err(InstallError::Cancelled)
    );
    assert_clean(&manager);
    assert_eq!(server.requests(), Vec::<String>::new());
}

#[test]
fn a_loopback_redirect_is_not_followed() {
    let dir = scratch_dir();
    let server = LoopbackServer::start(vec![
        Reply {
            status: 302,
            headers: vec![Header {
                name: String::from("Location"),
                value: String::from("/moved.sf2"),
            }],
            ..Reply::ok(PATH, Vec::new())
        },
        Reply::ok("/moved.sf2", tiny_bytes().to_vec()),
    ]);
    let entry = entry_for(
        "tiny",
        BankFormat::Sf2,
        ArchiveKind::Raw,
        &server.url(PATH),
        tiny_bytes(),
    );
    let mut manager = manager_with(dir.path(), vec![entry]);
    assert_eq!(
        manager.install(&id("tiny"), &mut ignore),
        Err(InstallError::Http { status: 302 })
    );
    assert_clean(&manager);
    assert_eq!(server.requests(), vec![String::from(PATH)]);
}
