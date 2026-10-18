use opentab_gp::{GpError, read_gp, write_gp};
use opentab_model::ContainerEntry;

use crate::rawzip::{CENTRAL_HEADER_SIZE, CENTRAL_SIGNATURE, METHOD_DEFLATED, RawEntry, build};
use crate::support::{Doc, SCORE_ENTRY};

fn gpif() -> String {
    Doc::default().render()
}

#[test]
fn stored_and_deflated_entries_read_alike() {
    let text = gpif();
    let stored = build(&[RawEntry::stored(SCORE_ENTRY, text.as_bytes())]);
    let deflated = build(&[RawEntry::deflated(SCORE_ENTRY, text.as_bytes())]);
    assert_eq!(read_gp(&stored), read_gp(&deflated));
    assert!(read_gp(&stored).is_ok());
}

#[test]
fn data_descriptors_are_read() {
    let text = gpif();
    let entries = [
        RawEntry::stored(SCORE_ENTRY, text.as_bytes()).with_descriptor(),
        RawEntry::deflated("a.bin", b"payload bytes payload bytes").with_descriptor(),
    ];
    let score = read_gp(&build(&entries)).expect("descriptors");
    assert_eq!(
        score.container,
        vec![
            ContainerEntry::Score {
                name: SCORE_ENTRY.to_string()
            },
            ContainerEntry::File {
                name: "a.bin".to_string(),
                data: opentab_model::Base64Bytes(b"payload bytes payload bytes".to_vec()),
            },
        ]
    );
}

#[test]
fn a_missing_score_entry_is_a_typed_error() {
    let text = gpif();
    let cases: Vec<Vec<RawEntry>> = vec![
        Vec::new(),
        vec![RawEntry::stored("VERSION", b"7.0")],
        vec![RawEntry::stored("Content/", b"")],
        vec![RawEntry::stored("Content/score.gpif/", b"")],
        vec![RawEntry::stored("content/score.gpif", text.as_bytes())],
        vec![RawEntry::stored("Content/score.gpif ", text.as_bytes())],
        vec![RawEntry::stored("/Content/score.gpif", text.as_bytes())],
        vec![RawEntry::stored("Content\\score.gpif", text.as_bytes())],
    ];
    for entries in cases {
        assert_eq!(read_gp(&build(&entries)), Err(GpError::MissingScore));
    }
}

#[test]
fn backslash_names_round_trip_verbatim() {
    let text = gpif();
    let entries = [
        RawEntry::stored(SCORE_ENTRY, text.as_bytes()),
        RawEntry::stored("Content\\Assets\\x.bin", b"123"),
    ];
    let score = read_gp(&build(&entries)).expect("backslash name");
    assert_eq!(
        score.container[1],
        ContainerEntry::File {
            name: "Content\\Assets\\x.bin".to_string(),
            data: opentab_model::Base64Bytes(b"123".to_vec()),
        }
    );
    let bytes = write_gp(&score).expect("write");
    assert_eq!(read_gp(&bytes), Ok(score));
}

#[test]
fn duplicate_entry_names_are_a_typed_error() {
    let text = gpif();
    let score = || RawEntry::stored(SCORE_ENTRY, text.as_bytes());
    let cases: Vec<(Vec<RawEntry>, &str)> = vec![
        (vec![score(), score()], SCORE_ENTRY),
        (
            vec![
                score(),
                RawEntry::stored("a", b"1"),
                RawEntry::stored("a", b"2"),
            ],
            "a",
        ),
        (
            vec![
                RawEntry::stored("a", b"1"),
                score(),
                RawEntry::deflated("a", b"1"),
            ],
            "a",
        ),
        (
            vec![
                score(),
                RawEntry::stored("d/", b""),
                RawEntry::stored("d/", b""),
            ],
            "d/",
        ),
    ];
    for (entries, name) in cases {
        match read_gp(&build(&entries)) {
            Err(GpError::DuplicateEntry { name: found }) => assert_eq!(found, name),
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn names_that_only_differ_in_case_or_slashes_are_not_duplicates() {
    let text = gpif();
    let entries = [
        RawEntry::stored(SCORE_ENTRY, text.as_bytes()),
        RawEntry::stored("a", b"1"),
        RawEntry::stored("A", b"2"),
        RawEntry::stored("a/", b""),
        RawEntry::stored("a\\", b""),
    ];
    assert_eq!(read_gp(&build(&entries)).expect("read").container.len(), 5);
}

#[test]
fn duplicates_are_found_behind_leading_junk() {
    let text = gpif();
    let mut bytes = b"MZ junk before the archive".to_vec();
    bytes.extend(build(&[
        RawEntry::stored(SCORE_ENTRY, text.as_bytes()),
        RawEntry::stored("a", b"1"),
        RawEntry::stored("a", b"2"),
    ]));
    assert!(matches!(
        read_gp(&bytes),
        Err(GpError::DuplicateEntry { .. })
    ));
}

#[test]
fn duplicates_are_found_when_the_central_directory_is_the_only_witness() {
    let text = gpif();
    let mut entries = vec![RawEntry::stored(SCORE_ENTRY, text.as_bytes())];
    entries.push(RawEntry::stored("x", b"1"));
    entries.push(RawEntry::stored("y", b"2"));
    let mut bytes = build(&entries);
    let at = bytes
        .windows(4)
        .rposition(|window| window == CENTRAL_SIGNATURE)
        .expect("last record");
    assert_eq!(bytes[at + CENTRAL_HEADER_SIZE], b'y');
    bytes[at + CENTRAL_HEADER_SIZE] = b'x';
    assert!(matches!(
        read_gp(&bytes),
        Err(GpError::DuplicateEntry { .. })
    ));
}

fn corrupt_entry(mutate: impl FnOnce(&mut RawEntry)) -> Result<opentab_model::Score, GpError> {
    let text = gpif();
    let mut entry = RawEntry::stored("c", b"payload");
    mutate(&mut entry);
    let entries = [
        RawEntry::stored(SCORE_ENTRY, text.as_bytes()),
        entry,
        RawEntry::stored("d", b"2"),
    ];
    read_gp(&build(&entries))
}

#[test]
fn a_wrong_checksum_is_an_entry_error() {
    let result = corrupt_entry(|entry| entry.crc ^= 1);
    assert!(
        matches!(result, Err(GpError::Entry { ref entry, .. }) if entry == "c"),
        "{result:?}"
    );
}

#[test]
fn unsupported_compression_methods_are_archive_errors() {
    for method in [12, 14, 93, 99] {
        let result = corrupt_entry(|entry| entry.method = method);
        assert!(
            matches!(result, Err(GpError::Archive { .. })),
            "{method}: {result:?}"
        );
    }
}

#[test]
fn a_garbage_deflate_stream_is_an_entry_error() {
    let result = corrupt_entry(|entry| {
        entry.method = METHOD_DEFLATED;
        entry.payload = vec![0xff; 7];
        entry.compressed_size = 7;
    });
    assert!(
        matches!(result, Err(GpError::Entry { ref entry, .. }) if entry == "c"),
        "{result:?}"
    );
}
