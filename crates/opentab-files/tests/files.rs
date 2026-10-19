use std::fs;
use std::path::{Path, PathBuf};

use opentab_files::{
    FileError, Format, Overwrite, SUFFIX, SUPPORTED, format_of, read_score, read_text,
    require_json, write_bytes, write_score,
};
use opentab_gp::{read_gp, write_gp};
use opentab_model::{Score, to_json};
use tempfile::TempDir;

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/gp/multi-track.gp")
}

fn fixture() -> Score {
    read_gp(&fs::read(fixture_path()).unwrap()).unwrap()
}

fn retitled(title: &str) -> Score {
    let mut score = fixture();
    score.meta.title = title.to_string();
    score
}

fn temp() -> TempDir {
    tempfile::tempdir().unwrap()
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn unsupported(path: &str, supported: &'static str) -> FileError {
    FileError::UnsupportedFormat {
        path: PathBuf::from(path),
        supported,
    }
}

#[test]
fn the_format_follows_the_suffix_in_any_case() {
    for (name, format) in [
        ("a.gp", Format::Gp),
        ("Song.GP", Format::Gp),
        ("a.opentab.json", Format::Json),
        ("x.OpenTab.JSON", Format::Json),
        ("dir.gp/a.opentab.json", Format::Json),
        ("dir.opentab.json/a.gp", Format::Gp),
    ] {
        assert_eq!(format_of(Path::new(name)), Ok(format), "{name}");
    }
}

#[test]
fn a_suffix_without_a_stem_or_an_unknown_suffix_is_unsupported() {
    for name in [
        ".gp",
        ".opentab.json",
        "song.txt",
        "a.json",
        "a.gpx",
        "gp",
        "a.opentab.jsonx",
        "",
    ] {
        assert_eq!(
            format_of(Path::new(name)),
            Err(unsupported(name, SUPPORTED)),
            "{name}"
        );
    }
}

#[test]
fn only_opentab_json_passes_the_json_requirement() {
    assert_eq!(require_json(Path::new("a.opentab.json")), Ok(()));
    for name in ["a.gp", "a.txt", ".opentab.json"] {
        assert_eq!(
            require_json(Path::new(name)),
            Err(unsupported(name, SUFFIX)),
            "{name}"
        );
    }
}

#[test]
fn scores_are_read_by_their_suffix() {
    let dir = temp();
    assert_eq!(read_score(&fixture_path()), Ok(fixture()));
    let score = retitled("Read back");
    let json = dir.path().join("a.opentab.json");
    fs::write(&json, to_json(&score).unwrap()).unwrap();
    assert_eq!(read_score(&json), Ok(score));
}

#[test]
fn unreadable_scores_name_what_went_wrong() {
    let dir = temp();
    let missing = dir.path().join("missing.gp");
    assert_eq!(read_score(&missing).unwrap_err().kind(), "io");
    let not_utf8 = dir.path().join("bytes.opentab.json");
    fs::write(&not_utf8, b"\xff\xfe").unwrap();
    assert_eq!(
        read_score(&not_utf8),
        Err(FileError::InvalidScore {
            path: not_utf8.clone(),
            message: "file is not valid UTF-8".to_string(),
        })
    );
    let not_a_score = dir.path().join("empty.opentab.json");
    fs::write(&not_a_score, b"{}").unwrap();
    assert_eq!(
        read_score(&not_a_score).unwrap_err().kind(),
        "invalid_score"
    );
    let not_a_zip = dir.path().join("broken.gp");
    fs::write(&not_a_zip, b"not a zip").unwrap();
    assert_eq!(read_score(&not_a_zip).unwrap_err().kind(), "invalid_input");
    assert_eq!(
        read_score(Path::new("song.txt")),
        Err(unsupported("song.txt", SUPPORTED))
    );
}

#[test]
fn text_is_read_whole_and_must_be_utf8() {
    let dir = temp();
    let path = dir.path().join("commands.json");
    fs::write(&path, "[{\"op\": \"undo\"}]\n").unwrap();
    assert_eq!(read_text(&path), Ok("[{\"op\": \"undo\"}]\n".to_string()));
    fs::write(&path, b"\xc3").unwrap();
    assert_eq!(
        read_text(&path),
        Err(FileError::InvalidInput {
            message: format!("{} is not valid UTF-8", path.display()),
        })
    );
    assert_eq!(
        read_text(&dir.path().join("missing.json"))
            .unwrap_err()
            .kind(),
        "io"
    );
}

#[test]
fn a_refused_overwrite_writes_a_new_file_once_and_keeps_it() {
    let dir = temp();
    let path = dir.path().join("a.opentab.json");
    let first = retitled("First");
    write_score(&path, &first, Overwrite::Refused).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), to_json(&first).unwrap());
    let refused = write_score(&path, &retitled("Second"), Overwrite::Refused);
    assert_eq!(refused, Err(FileError::OutputExists { path: path.clone() }));
    assert_eq!(fs::read_to_string(&path).unwrap(), to_json(&first).unwrap());
    assert_eq!(names(dir.path()), ["a.opentab.json"]);
}

#[test]
fn an_allowed_overwrite_replaces_the_file_whole() {
    let dir = temp();
    let path = dir.path().join("a.opentab.json");
    fs::write(&path, "old").unwrap();
    let score = retitled("Replaced");
    write_score(&path, &score, Overwrite::Allowed).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), to_json(&score).unwrap());
    let fresh = dir.path().join("fresh.opentab.json");
    write_score(&fresh, &score, Overwrite::Allowed).unwrap();
    assert_eq!(read_score(&fresh), Ok(score));
    assert_eq!(names(dir.path()), ["a.opentab.json", "fresh.opentab.json"]);
}

#[test]
fn a_gp_target_gets_the_gp_encoding() {
    let dir = temp();
    let score = fixture();
    let allowed = dir.path().join("allowed.gp");
    write_score(&allowed, &score, Overwrite::Allowed).unwrap();
    assert_eq!(fs::read(&allowed).unwrap(), write_gp(&score).unwrap());
    let refused = dir.path().join("refused.gp");
    write_score(&refused, &score, Overwrite::Refused).unwrap();
    assert_eq!(fs::read(&refused).unwrap(), write_gp(&score).unwrap());
}

#[test]
fn writing_an_unknown_suffix_or_into_a_missing_directory_fails() {
    let dir = temp();
    let score = fixture();
    let text = dir.path().join("a.txt");
    assert_eq!(
        write_score(&text, &score, Overwrite::Allowed)
            .unwrap_err()
            .kind(),
        "unsupported_format"
    );
    let nested = dir.path().join("missing").join("a.opentab.json");
    for overwrite in [Overwrite::Allowed, Overwrite::Refused] {
        assert_eq!(
            write_score(&nested, &score, overwrite).unwrap_err().kind(),
            "io"
        );
    }
    assert!(names(dir.path()).is_empty());
}

#[test]
fn bytes_replace_the_target_and_leave_no_temporary_file() {
    let dir = temp();
    let path = dir.path().join("page.svg");
    write_bytes(&path, b"<svg/>").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"<svg/>");
    write_bytes(&path, b"<svg></svg>").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"<svg></svg>");
    assert_eq!(names(dir.path()), ["page.svg"]);
    let nested = dir.path().join("missing").join("page.svg");
    assert_eq!(write_bytes(&nested, b"x").unwrap_err().kind(), "io");
}

#[cfg(unix)]
#[test]
fn bytes_written_through_a_link_replace_its_target_and_keep_the_link() {
    let dir = temp();
    let target = dir.path().join("page.svg");
    fs::write(&target, "old").unwrap();
    let link = dir.path().join("link.svg");
    std::os::unix::fs::symlink("page.svg", &link).unwrap();
    write_bytes(&link, b"new").unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"new");
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(names(dir.path()), ["link.svg", "page.svg"]);
}
