use crate::oracle::error::OracleError;
use crate::oracle::hash::sha256_hex;
use crate::oracle::layout::{
    MANIFEST_FILE, RESAVE_GP, RESAVE_GPIF, is_byte_identical, is_published_copy,
};
use crate::oracle::manifest::{GoldenSet, Manifest, ManifestEntry, OutputEntry};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

pub const SCORE_ENTRY: &str = "Content/score.gpif";
const MAX_SCORE_BYTES: u64 = 64 * 1024 * 1024;
const NOTATION_PATCH: &str = "NotationPatch";

#[derive(Debug, Clone)]
pub struct PublishOptions {
    pub from: PathBuf,
    pub out: PathBuf,
}

fn io<T>(result: std::io::Result<T>, context: impl FnOnce() -> String) -> Result<T, OracleError> {
    result.map_err(|error| OracleError::io(context(), &error))
}

pub fn score_gpif(container: &[u8]) -> Result<Vec<u8>, OracleError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(container))
        .map_err(|error| OracleError::new(format!("resave.gp is not a zip archive: {error}")))?;
    let entry = archive
        .by_name(SCORE_ENTRY)
        .map_err(|error| OracleError::new(format!("resave.gp has no {SCORE_ENTRY}: {error}")))?;
    let mut bytes = Vec::new();
    entry
        .take(MAX_SCORE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| OracleError::new(format!("cannot inflate {SCORE_ENTRY}: {error}")))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_SCORE_BYTES {
        return Err(OracleError::new(format!(
            "{SCORE_ENTRY} is larger than {MAX_SCORE_BYTES} bytes"
        )));
    }
    Ok(bytes)
}

fn starts_element(rest: &str, name: &str) -> bool {
    rest.strip_prefix('<')
        .and_then(|tag| tag.strip_prefix(name))
        .and_then(|after| after.chars().next())
        .is_some_and(|next| next == '>' || next == '/' || next.is_whitespace())
}

pub fn empty_notation_patches(gpif: &[u8]) -> Result<Vec<u8>, OracleError> {
    let text = std::str::from_utf8(gpif)
        .map_err(|error| OracleError::new(format!("{SCORE_ENTRY} is not UTF-8: {error}")))?;
    let close = format!("</{NOTATION_PATCH}>");
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        if !starts_element(rest, NOTATION_PATCH) {
            out.push('<');
            rest = &rest[1..];
            continue;
        }
        let head_end = rest
            .find('>')
            .ok_or_else(|| OracleError::new(format!("an unterminated <{NOTATION_PATCH}> tag")))?;
        let head = &rest[..head_end];
        let opening = head.strip_suffix('/').unwrap_or(head).trim_end();
        out.push_str(opening);
        out.push_str("/>");
        rest = if head.ends_with('/') {
            &rest[head_end + 1..]
        } else {
            let end = rest.find(&close).ok_or_else(|| {
                OracleError::new(format!("<{NOTATION_PATCH}> has no closing tag"))
            })?;
            &rest[end + close.len()..]
        };
    }
    out.push_str(rest);
    Ok(out.into_bytes())
}

fn same_directory(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn publish_entry(
    from: &Path,
    out: &Path,
    entry: &ManifestEntry,
) -> Result<ManifestEntry, OracleError> {
    let source = from.join(&entry.fixture);
    let target = out.join(&entry.fixture);
    if target.exists() {
        io(std::fs::remove_dir_all(&target), || {
            format!("cannot replace {}", target.display())
        })?;
    }
    let mut outputs = Vec::new();
    for output in &entry.outputs {
        let raw_path = source.join(&output.path);
        let (path, bytes) = if output.path == RESAVE_GP {
            let raw = io(std::fs::read(&raw_path), || {
                format!("cannot read {}", raw_path.display())
            })?;
            (
                RESAVE_GPIF.to_owned(),
                empty_notation_patches(&score_gpif(&raw)?)?,
            )
        } else if is_published_copy(&output.path) {
            let raw = io(std::fs::read(&raw_path), || {
                format!("cannot read {}", raw_path.display())
            })?;
            (output.path.clone(), raw)
        } else {
            continue;
        };
        let written = target.join(&path);
        if let Some(parent) = written.parent() {
            io(std::fs::create_dir_all(parent), || {
                format!("cannot create {}", parent.display())
            })?;
        }
        io(std::fs::write(&written, &bytes), || {
            format!("cannot write {}", written.display())
        })?;
        let identical = is_byte_identical(&path);
        outputs.push(OutputEntry {
            sha256: identical.then(|| sha256_hex(&bytes)),
            byte_identical: identical,
            path,
        });
    }
    Ok(ManifestEntry {
        fixture: entry.fixture.clone(),
        sha256: entry.sha256.clone(),
        outputs,
    })
}

pub fn publish(options: &PublishOptions) -> Result<usize, OracleError> {
    let source = Manifest::load(&options.from.join(MANIFEST_FILE))?;
    if source.set != GoldenSet::Full {
        return Err(OracleError::new(format!(
            "{} is already a published set; publish from a full harvest",
            options.from.display()
        )));
    }
    io(std::fs::create_dir_all(&options.out), || {
        format!("cannot create {}", options.out.display())
    })?;
    if same_directory(&options.from, &options.out) {
        return Err(OracleError::new(String::from(
            "the published set must go to a directory other than the full harvest",
        )));
    }
    let mut published = Manifest::new(source.gp_version.clone());
    published.set = GoldenSet::Published;
    for entry in &source.entries {
        published.upsert(publish_entry(&options.from, &options.out, entry)?);
    }
    published.save(&options.out.join(MANIFEST_FILE))?;
    Ok(published.entries.len())
}

#[cfg(test)]
mod tests {
    use super::{PublishOptions, SCORE_ENTRY, empty_notation_patches, publish};
    use crate::oracle::check::{Finding, check};
    use crate::oracle::hash::sha256_hex;
    use crate::oracle::layout::{NON_BYTE_IDENTICAL_OUTPUTS, REQUIRED_OUTPUTS};
    use crate::oracle::manifest::{GoldenSet, Manifest, ManifestEntry, OutputEntry};
    use std::io::Write;
    use std::path::Path;

    const GPIF: &[u8] = b"<GPIF>ours</GPIF>";

    fn container() -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in [
            ("Content/BinaryStylesheet", &b"vendor"[..]),
            (SCORE_ENTRY, GPIF),
            ("VERSION", b"7.0"),
        ] {
            writer.start_file(name, options).expect("entry");
            writer.write_all(data).expect("data");
        }
        writer.finish().expect("zip").into_inner()
    }

    fn full_harvest(root: &Path, input: &Path) {
        std::fs::create_dir_all(input).expect("mkdir");
        std::fs::write(input.join("a.gp"), b"fixture").expect("write");
        let dir = root.join("a.gp");
        std::fs::create_dir_all(dir.join("png")).expect("mkdir");
        let mut outputs = Vec::new();
        let mut paths: Vec<String> = REQUIRED_OUTPUTS.iter().map(|p| (*p).to_owned()).collect();
        paths.push(String::from("png/a 1.png"));
        for path in paths {
            let content = if path == "resave.gp" {
                container()
            } else {
                format!("content of {path}").into_bytes()
            };
            std::fs::write(dir.join(&path), &content).expect("write");
            let identical = !NON_BYTE_IDENTICAL_OUTPUTS.contains(&path.as_str());
            outputs.push(OutputEntry {
                sha256: identical.then(|| sha256_hex(&content)),
                byte_identical: identical,
                path,
            });
        }
        let mut manifest = Manifest::new("8.1.5.31");
        manifest.upsert(ManifestEntry {
            fixture: String::from("a.gp"),
            sha256: sha256_hex(b"fixture"),
            outputs,
        });
        manifest.save(&root.join("manifest.json")).expect("save");
    }

    #[test]
    fn publishing_keeps_only_the_committed_set_and_checks_clean() {
        let dir = tempfile::tempdir().expect("tempdir");
        let full = dir.path().join("full");
        let input = dir.path().join("in");
        let out = dir.path().join("published");
        full_harvest(&full, &input);
        assert_eq!(check(&input, &full).expect("check"), Vec::new());
        let count = publish(&PublishOptions {
            from: full.clone(),
            out: out.clone(),
        })
        .expect("publish");
        assert_eq!(count, 1);
        let golden = out.join("a.gp");
        assert_eq!(
            std::fs::read(golden.join("resave.gpif")).expect("gpif"),
            GPIF
        );
        for absent in ["resave.gp", "export.pdf", "export.gpx"] {
            assert!(!golden.join(absent).exists(), "{absent}");
        }
        for present in ["export.mid", "export.musicxml", "export.gp5", "png/a 1.png"] {
            assert!(golden.join(present).is_file(), "{present}");
        }
        let manifest = Manifest::load(&out.join("manifest.json")).expect("load");
        assert_eq!(manifest.set, GoldenSet::Published);
        assert_eq!(check(&input, &out).expect("check"), Vec::new());
        std::fs::remove_file(golden.join("resave.gpif")).expect("remove");
        assert_eq!(
            check(&input, &out).expect("check"),
            vec![Finding::Missing(String::from("a.gp: resave.gpif"))]
        );
    }

    #[test]
    fn publishing_empties_every_notation_patch_and_keeps_the_rest() {
        let gpif = concat!(
            "<GPIF><Track><NotationPatch>\n<Name>Vendor kit</Name><Elements><Element>",
            "<Name>Vendor drum</Name></Element></Elements>\n</NotationPatch><Name>Ours</Name>",
            "</Track><Track><NotationPatch version=\"2\" ><LineCount>5</LineCount></NotationPatch>",
            "<NotationPatchless/><NotationPatch/></Track></GPIF>"
        );
        let emptied = String::from_utf8(empty_notation_patches(gpif.as_bytes()).expect("empty"))
            .expect("utf8");
        assert_eq!(
            emptied,
            concat!(
                "<GPIF><Track><NotationPatch/><Name>Ours</Name></Track>",
                "<Track><NotationPatch version=\"2\"/><NotationPatchless/><NotationPatch/></Track></GPIF>"
            )
        );
        assert!(!emptied.contains("Vendor"));
        assert_eq!(
            empty_notation_patches(b"<GPIF>ours</GPIF>").expect("plain"),
            b"<GPIF>ours</GPIF>"
        );
        assert!(empty_notation_patches(b"<GPIF><NotationPatch><Name>x</Name></GPIF>").is_err());
    }

    #[test]
    fn a_published_set_cannot_be_published_again_or_in_place() {
        let dir = tempfile::tempdir().expect("tempdir");
        let full = dir.path().join("full");
        full_harvest(&full, &dir.path().join("in"));
        let same = PublishOptions {
            from: full.clone(),
            out: full.clone(),
        };
        assert!(publish(&same).is_err());
        let out = dir.path().join("published");
        publish(&PublishOptions {
            from: full,
            out: out.clone(),
        })
        .expect("publish");
        let again = PublishOptions {
            from: out,
            out: dir.path().join("other"),
        };
        assert!(publish(&again).is_err());
    }
}
