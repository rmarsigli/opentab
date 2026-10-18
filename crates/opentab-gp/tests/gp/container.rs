use opentab_gp::{GpError, read_gp};
use opentab_model::{Base64Bytes, ContainerEntry};

use crate::support::{Doc, SCORE_ENTRY, archive};

#[test]
fn entries_are_kept_in_order_with_the_score_in_place() {
    let gpif = Doc::default().render();
    let binary: &[u8] = &[0, 159, 146, 150, 255, 0, 1];
    let bytes = archive(&[
        ("Content/", b""),
        ("Content/BinaryStylesheet", binary),
        (SCORE_ENTRY, gpif.as_bytes()),
        ("VERSION", b"7.0"),
        ("meta.json", b"{}"),
    ]);
    let score = read_gp(&bytes).expect("read");
    assert_eq!(
        score.container,
        vec![
            ContainerEntry::File {
                name: "Content/".to_string(),
                data: Base64Bytes(Vec::new()),
            },
            ContainerEntry::File {
                name: "Content/BinaryStylesheet".to_string(),
                data: Base64Bytes(binary.to_vec()),
            },
            ContainerEntry::Score {
                name: SCORE_ENTRY.to_string(),
            },
            ContainerEntry::File {
                name: "VERSION".to_string(),
                data: Base64Bytes(b"7.0".to_vec()),
            },
            ContainerEntry::File {
                name: "meta.json".to_string(),
                data: Base64Bytes(b"{}".to_vec()),
            },
        ]
    );
}

#[test]
fn an_archive_without_score_is_rejected() {
    let bytes = archive(&[("VERSION", b"7.0")]);
    assert_eq!(read_gp(&bytes), Err(GpError::MissingScore));
}

#[test]
fn bytes_that_are_not_a_zip_are_rejected() {
    assert!(matches!(
        read_gp(b"not a zip archive"),
        Err(GpError::Archive { .. })
    ));
    assert!(matches!(read_gp(b""), Err(GpError::Archive { .. })));
}

#[test]
fn a_byte_order_mark_is_accepted() {
    let mut gpif = b"\xEF\xBB\xBF".to_vec();
    gpif.extend_from_slice(Doc::default().render().as_bytes());
    let bytes = archive(&[(SCORE_ENTRY, &gpif)]);
    assert_eq!(read_gp(&bytes).expect("read").meta.title, "Song");
}
