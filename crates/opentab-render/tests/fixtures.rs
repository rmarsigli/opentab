use std::path::PathBuf;

use opentab_gp::read_gp;
use opentab_layout::{LayoutOptions, StaffMode, layout};
use opentab_render::to_svg;

const SNAPSHOT_LIMIT: usize = 600_000;
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
const SUMMARY_TAGS: [&str; 5] = ["<path ", "<line ", "<rect ", "<g ", "</g>"];

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/gp")
        .join(format!("{name}.gp"))
}

fn digest(text: &str) -> u64 {
    text.bytes().fold(FNV_OFFSET, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
    })
}

fn summary(svg: &str) -> String {
    let mut lines = vec![
        format!("bytes {}", svg.len()),
        format!("lines {}", svg.lines().count()),
        format!("fnv1a64 {:016x}", digest(svg)),
    ];
    lines.extend(
        SUMMARY_TAGS
            .iter()
            .map(|tag| format!("{} {}", tag.trim(), svg.matches(tag).count())),
    );
    lines.join("\n")
}

fn pages(name: &str, options: &LayoutOptions) -> Vec<String> {
    let bytes = std::fs::read(fixture(name)).expect("fixture");
    let score = read_gp(&bytes).expect("fixture reads");
    let list = layout(&score, options).expect("fixture lays out");
    let first: Vec<String> = list.pages.iter().map(to_svg).collect();
    let again: Vec<String> = layout(&score, options)
        .expect("fixture lays out")
        .pages
        .iter()
        .map(to_svg)
        .collect();
    assert_eq!(first, again, "{name} renders deterministically");
    first
}

fn check(name: &str, suffix: &str, options: &LayoutOptions) {
    let svgs = pages(name, options);
    assert!(!svgs.is_empty());
    for (index, svg) in svgs.iter().enumerate() {
        assert!(svg.starts_with("<svg ") && svg.trim_end().ends_with("</svg>"));
        let page = if svgs.len() == 1 {
            String::new()
        } else {
            format!("-p{}", index + 1)
        };
        let snapshot = format!("{name}{suffix}{page}");
        let content = if svg.len() > SNAPSHOT_LIMIT {
            summary(svg)
        } else {
            svg.clone()
        };
        insta::assert_snapshot!(snapshot, content);
    }
}

fn default_options() -> LayoutOptions {
    LayoutOptions::default()
}

#[test]
fn the_digest_and_summary_are_stable_and_count_each_tag() {
    assert_eq!(digest(""), FNV_OFFSET);
    assert_eq!(digest("a"), 0xaf63_dc4c_8601_ec8c);
    let text = summary("<svg>\n<g data-id=\"1\">\n<path d=\"M0 0\"/>\n</g>\n</svg>");
    assert_eq!(
        text,
        "bytes 50\nlines 5\nfnv1a64 aa26efb0047de351\n<path 1\n<line 0\n<rect 0\n<g 1\n</g> 1"
    );
}

#[test]
fn beat_effects() {
    check("beat-effects", "", &default_options());
}

#[test]
fn durations() {
    check("durations", "", &default_options());
}

#[test]
fn durations_as_tab_only() {
    check(
        "durations",
        "-tab",
        &LayoutOptions {
            staff_mode: Some(StaffMode::Tab),
            ..LayoutOptions::default()
        },
    );
}

#[test]
fn from_scratch() {
    check("from-scratch", "", &default_options());
}

#[test]
fn metadata() {
    check("metadata", "", &default_options());
}

#[test]
fn multi_track() {
    check("multi-track", "", &default_options());
}

#[test]
fn multi_voice() {
    check("multi-voice", "", &default_options());
}

#[test]
fn note_effects() {
    check("note-effects", "", &default_options());
}

#[test]
fn note_effects_as_tab_only() {
    check(
        "note-effects",
        "-tab",
        &LayoutOptions {
            staff_mode: Some(StaffMode::Tab),
            ..LayoutOptions::default()
        },
    );
}

#[test]
fn repeats() {
    check("repeats", "", &default_options());
}

#[test]
fn signatures() {
    check("signatures", "", &default_options());
}

#[test]
fn tempo() {
    check("tempo", "", &default_options());
}

#[test]
fn tunings() {
    check("tunings", "", &default_options());
}

#[test]
fn every_committed_fixture_has_a_snapshot_test() {
    let mut names: Vec<String> = std::fs::read_dir(fixture("x").parent().unwrap())
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()
                .and_then(|name| name.strip_suffix(".gp"))
                .map(str::to_string)
        })
        .collect();
    names.sort();
    let source = include_str!("fixtures.rs");
    for name in &names {
        let test = format!("fn {}() {{", name.replace('-', "_"));
        assert!(source.contains(&test), "no snapshot test for {name}");
    }
    assert_eq!(names.len(), 11);
}
