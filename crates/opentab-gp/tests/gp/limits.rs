use opentab_gp::{GpError, GpifProblem, LimitProblem, read_gp};
use opentab_model::{ContainerEntry, Score};

use crate::rawzip::{RawEntry, build};
use crate::support::{Doc, SCORE_ENTRY, gp, read_gpif_error};

const MIB: usize = 1024 * 1024;
const ENTRY_LIMIT: usize = 64 * MIB;
const TOTAL_LIMIT: usize = 256 * MIB;
const XML_DEPTH_LIMIT: usize = 256;
const XML_ELEMENT_LIMIT: usize = 1_000_000;
const MODEL_ELEMENT_LIMIT: u64 = 1_000_000;
const RETAINED_LIMIT: usize = 64 * MIB;
const PAYLOAD_OVERHEAD: usize = 64;

struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn runs(length: usize) -> Vec<u8> {
    let mut random = Xorshift(0x9e37_79b9_7f4a_7c15);
    let mut data = Vec::with_capacity(length + 256);
    while data.len() < length {
        let value = u8::try_from(random.next() & 0xff).unwrap_or_default();
        let run = usize::try_from(random.next() % 200).unwrap_or_default() + 1;
        data.resize(data.len() + run, value);
    }
    data.truncate(length);
    data
}

fn score_entry() -> RawEntry {
    RawEntry::deflated(SCORE_ENTRY, Doc::default().render().as_bytes())
}

fn limit_of(result: Result<Score, GpError>) -> LimitProblem {
    match result {
        Err(GpError::Limit { problem, .. }) => problem,
        Err(other) => panic!("not a limit error: {other}"),
        Ok(_) => panic!("read unexpectedly succeeded"),
    }
}

fn container_bytes(score: &Score) -> usize {
    score
        .container
        .iter()
        .map(|entry| match entry {
            ContainerEntry::File { data, .. } => data.0.len(),
            ContainerEntry::Score { .. } => 0,
        })
        .sum()
}

#[test]
fn a_small_file_declaring_a_huge_size_is_refused_before_inflating() {
    let zeros = vec![0u8; MIB];
    for declared in [u32::MAX, u32::try_from(ENTRY_LIMIT).unwrap_or(u32::MAX) + 1] {
        let bomb = RawEntry::deflated("bomb", &zeros).declaring(declared);
        let bytes = build(&[score_entry(), bomb]);
        assert!(bytes.len() < 4096);
        assert_eq!(
            limit_of(read_gp(&bytes)),
            LimitProblem::EntrySize {
                limit: ENTRY_LIMIT as u64
            }
        );
    }
}

#[test]
fn a_declared_size_just_beyond_the_ratio_is_refused_before_inflating() {
    let zeros = vec![0u8; MIB];
    let compressed = u64::from(RawEntry::deflated("bomb", &zeros).compressed_size);
    let declared = u32::try_from(compressed * 200 + 1).expect("declared");
    let bomb = RawEntry::deflated("bomb", &zeros).declaring(declared);
    assert_eq!(
        limit_of(read_gp(&build(&[score_entry(), bomb]))),
        LimitProblem::CompressionRatio { limit: 200 }
    );
}

#[test]
fn an_honest_bomb_is_refused_by_ratio() {
    let zeros = vec![0u8; 10 * MIB];
    let bytes = build(&[score_entry(), RawEntry::deflated("bomb", &zeros)]);
    assert!(bytes.len() < 64 * 1024);
    assert_eq!(
        limit_of(read_gp(&bytes)),
        LimitProblem::CompressionRatio { limit: 200 }
    );
}

#[test]
fn an_honest_bomb_beyond_the_entry_limit_is_refused() {
    let zeros = vec![0u8; ENTRY_LIMIT + MIB];
    let bytes = build(&[score_entry(), RawEntry::deflated("bomb", &zeros)]);
    assert!(bytes.len() < 512 * 1024);
    assert_eq!(
        limit_of(read_gp(&bytes)),
        LimitProblem::EntrySize {
            limit: ENTRY_LIMIT as u64
        }
    );
}

#[test]
fn an_entry_that_lies_about_its_size_is_refused_within_bounds() {
    let zeros = vec![0u8; 100 * MIB];
    for declared in [0u32, 10, 1000, 1 << 20] {
        let bomb = RawEntry::deflated("liar", &zeros).declaring(declared);
        let bytes = build(&[score_entry(), bomb]);
        assert!(bytes.len() < 512 * 1024);
        assert!(matches!(
            limit_of(read_gp(&bytes)),
            LimitProblem::CompressionRatio { limit: 200 } | LimitProblem::EntrySize { .. }
        ));
    }
}

#[test]
fn an_inflated_compressed_size_cannot_widen_the_entry_cap() {
    let zeros = vec![0u8; 100 * MIB];
    let mut bomb = RawEntry::deflated("liar", &zeros).declaring(1000);
    let real = bomb.compressed_size;
    bomb.compressed_size = u32::try_from(MIB).expect("lie");
    assert!(bomb.compressed_size > real);
    let padding = RawEntry::stored("padding", &vec![7u8; 2 * MIB]);
    let bytes = build(&[score_entry(), bomb, padding]);
    assert_eq!(
        limit_of(read_gp(&bytes)),
        LimitProblem::EntrySize {
            limit: ENTRY_LIMIT as u64
        }
    );
}

#[test]
fn an_entry_that_lies_and_stays_inside_the_ratio_hits_the_entry_limit() {
    let data = runs(ENTRY_LIMIT + MIB);
    let bomb = RawEntry::deflated("liar", &data).declaring(1000);
    assert!(u64::from(bomb.compressed_size) * 200 > data.len() as u64);
    let bytes = build(&[score_entry(), bomb]);
    assert_eq!(
        limit_of(read_gp(&bytes)),
        LimitProblem::EntrySize {
            limit: ENTRY_LIMIT as u64
        }
    );
}

#[test]
fn an_entry_of_exactly_the_limit_is_read_and_one_byte_more_is_not() {
    let data = runs(ENTRY_LIMIT + 1);
    let at = RawEntry::deflated("big", &data[..ENTRY_LIMIT]);
    assert!(u64::from(at.compressed_size) * 200 > ENTRY_LIMIT as u64);
    let score = read_gp(&build(&[score_entry(), at])).expect("entry at the limit");
    assert_eq!(container_bytes(&score), ENTRY_LIMIT);
    drop(score);
    let over = RawEntry::deflated("big", &data);
    assert_eq!(
        limit_of(read_gp(&build(&[score_entry(), over]))),
        LimitProblem::EntrySize {
            limit: ENTRY_LIMIT as u64
        }
    );
}

#[test]
fn the_archive_total_is_inclusive_at_the_limit() {
    let score = score_entry();
    let score_size = Doc::default().render().len();
    let part = runs(63 * MIB);
    let part_entry = RawEntry::deflated("part", &part);
    assert!(u64::from(part_entry.compressed_size) * 200 > part.len() as u64);
    let rest = TOTAL_LIMIT - score_size - 4 * part.len();
    let rest_data = runs(rest);
    let mut entries = vec![score];
    for index in 0..4 {
        let mut copy = part_entry.clone();
        copy.name = format!("part{index}").into_bytes();
        entries.push(copy);
    }
    entries.push(RawEntry::deflated("rest", &rest_data));
    let exact = read_gp(&build(&entries)).expect("total at the limit");
    assert_eq!(container_bytes(&exact) + score_size, TOTAL_LIMIT);
    drop(exact);
    entries.push(RawEntry::stored("one-more", b"x"));
    assert_eq!(
        limit_of(read_gp(&build(&entries))),
        LimitProblem::TotalSize {
            limit: TOTAL_LIMIT as u64
        }
    );
}

fn elements_in(text: &str) -> usize {
    text.as_bytes()
        .windows(2)
        .filter(|pair| pair[0] == b'<' && pair[1].is_ascii_alphabetic())
        .count()
}

fn padded_elements(total: usize) -> String {
    let baseline = elements_in(&Doc::default().render());
    let pad = total - baseline - 1;
    let root = format!("<Extra>{}</Extra>", "<a/>".repeat(pad));
    let text = Doc {
        root,
        ..Doc::default()
    }
    .render();
    assert_eq!(elements_in(&text), total);
    text
}

#[test]
fn the_element_limit_is_inclusive() {
    let at = padded_elements(XML_ELEMENT_LIMIT);
    read_gp(&gp(&at)).expect("elements at the limit");
    let over = padded_elements(XML_ELEMENT_LIMIT + 1);
    assert!(matches!(
        read_gpif_error(&over),
        GpError::Gpif {
            problem: GpifProblem::ElementLimit {
                limit: XML_ELEMENT_LIMIT
            },
            ..
        }
    ));
}

fn nested(levels: usize) -> String {
    let root = format!(
        "<Extra>{}{}</Extra>",
        "<a>".repeat(levels),
        "</a>".repeat(levels)
    );
    Doc {
        root,
        ..Doc::default()
    }
    .render()
}

#[test]
fn the_depth_limit_is_inclusive() {
    let at = nested(XML_DEPTH_LIMIT - 2);
    read_gp(&gp(&at)).expect("depth at the limit");
    let over = nested(XML_DEPTH_LIMIT - 1);
    assert!(matches!(
        read_gpif_error(&over),
        GpError::Gpif {
            problem: GpifProblem::DepthLimit {
                limit: XML_DEPTH_LIMIT
            },
            ..
        }
    ));
}

fn with_beat_references(count: usize) -> String {
    let references = vec!["0"; count].join(" ");
    Doc::default()
        .render()
        .replace("<Beats>0</Beats>", &format!("<Beats>{references}</Beats>"))
}

fn ids_after(count: usize) -> u64 {
    let score = read_gp(&gp(&with_beat_references(count))).expect("read");
    score.ids.peek().0
}

#[test]
fn the_expansion_limit_is_inclusive() {
    let first = ids_after(1);
    let per_reference = ids_after(11) - first;
    assert_eq!(per_reference % 10, 0);
    let per_reference = per_reference / 10;
    assert!(per_reference > 0);
    let base = first - per_reference;
    let allowed = usize::try_from((MODEL_ELEMENT_LIMIT + 1 - base) / per_reference).unwrap();
    assert!(ids_after(allowed) <= MODEL_ELEMENT_LIMIT + 1);
    assert!(base + per_reference * (allowed as u64 + 1) > MODEL_ELEMENT_LIMIT + 1);
    assert!(matches!(
        read_gpif_error(&with_beat_references(allowed + 1)),
        GpError::Gpif {
            problem: GpifProblem::ExpansionLimit {
                limit: MODEL_ELEMENT_LIMIT
            },
            ..
        }
    ));
}

fn retained_bytes(score: &Score) -> usize {
    score
        .extensions
        .0
        .values()
        .flatten()
        .chain(score.document_extensions.iter())
        .map(|payload| payload.data.len() + PAYLOAD_OVERHEAD)
        .sum()
}

fn retained_document(references: usize, ornament: usize, extra: usize) -> String {
    let refs = vec!["0"; references].join(" ");
    Doc {
        note: format!("<Ornament>{}</Ornament>", "x".repeat(ornament)),
        root: format!("<Extra>{}</Extra>", "y".repeat(extra)),
        ..Doc::default()
    }
    .render()
    .replace("<Notes>0</Notes>", &format!("<Notes>{refs}</Notes>"))
}

#[test]
fn the_retained_budget_is_inclusive() {
    const REFERENCES: usize = 1000;
    const ORNAMENT: usize = 60_000;
    let one = read_gp(&gp(&retained_document(1, ORNAMENT, 1))).expect("one");
    let two = read_gp(&gp(&retained_document(2, ORNAMENT, 1))).expect("two");
    let per_reference = retained_bytes(&two) - retained_bytes(&one);
    let extra_base = retained_bytes(&one) - per_reference - 1;
    let extra = RETAINED_LIMIT - REFERENCES * per_reference - extra_base;
    let at = read_gp(&gp(&retained_document(REFERENCES, ORNAMENT, extra))).expect("at the limit");
    assert_eq!(retained_bytes(&at), RETAINED_LIMIT);
    drop(at);
    assert!(matches!(
        read_gpif_error(&retained_document(REFERENCES, ORNAMENT, extra + 1)),
        GpError::Gpif {
            problem: GpifProblem::RetainedLimit { limit: 67_108_864 },
            ..
        }
    ));
}
