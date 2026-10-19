use std::path::{Path, PathBuf};

const FLOAT_TYPES: [&str; 2] = ["f32", "f64"];
const FLOAT_CALLS: [&str; 1] = [".bpm()"];
const LINT: &str = "#![deny(clippy::float_arithmetic)]";
const MIN_SOURCES: usize = 7;

fn sources(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("source directory") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

fn float_literal(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    bytes.windows(3).enumerate().find_map(|(index, window)| {
        let in_word =
            index > 0 && (bytes[index - 1].is_ascii_alphabetic() || bytes[index - 1] == b'_');
        let decimal = window[0].is_ascii_digit() && window[1] == b'.' && window[2].is_ascii_digit();
        let exponent = window[0].is_ascii_digit()
            && (window[1] == b'e' || window[1] == b'E')
            && (window[2].is_ascii_digit() || window[2] == b'-');
        ((decimal || exponent) && !in_word).then(|| String::from_utf8_lossy(window).into_owned())
    })
}

#[test]
fn the_playback_sources_hold_no_float() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    sources(&root, &mut found);
    assert!(found.len() >= MIN_SOURCES, "{found:?}");
    for path in &found {
        let text = std::fs::read_to_string(path).expect("source");
        for word in FLOAT_TYPES.iter().chain(FLOAT_CALLS.iter()) {
            assert!(!text.contains(word), "{} uses {word}", path.display());
        }
        assert_eq!(float_literal(&text), None, "{}", path.display());
    }
    let lib = std::fs::read_to_string(root.join("lib.rs")).expect("lib");
    assert!(lib.contains(LINT));
}

#[test]
fn the_scan_finds_floats_when_they_are_there() {
    assert_eq!(float_literal("let x = 1.5;"), Some("1.5".to_string()));
    assert_eq!(float_literal("let x = 2e3;"), Some("2e3".to_string()));
    assert_eq!(float_literal("for i in 0..4 {}"), None);
    assert_eq!(float_literal("tempo.exact()"), None);
}
