use std::cell::Cell;
use std::io::Read;
use std::sync::LazyLock;

use opentab_gp::{GpError, read_gp, write_gp};
use opentab_model::{from_json, to_json};
use opentab_testkit::{ScoreGenConfig, arb_score_with};
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::{Config, FileFailurePersistence, TestCaseError, TestRunner};
use zip::ZipArchive;

use crate::corpus::corpus_paths;
use crate::rawzip::{CENTRAL_HEADER_SIZE, END_RECORD_SIZE, LOCAL_HEADER_SIZE, RawEntry, build};
use crate::support::{SCORE_ENTRY, documented};

const CASES: u32 = 256;
const LONG_CASES: u32 = 20_000;
const SEED_POOL: usize = 24;
const SEED_ATTEMPTS: usize = 400;
const MEMBER_LIMIT: u64 = 16 * 1024 * 1024;
const LOCAL_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x03, 0x04];
const CENTRAL_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];
const END_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
const NAMES: [&str; 9] = [
    "Content/score.gpif",
    "content/score.gpif",
    "Content\\score.gpif",
    "",
    "a",
    "../x",
    "Content/",
    "Content/score.gpif/",
    "VERSION",
];
const SNIPPETS: [&str; 16] = [
    "<X/>",
    "<![CDATA[",
    "]]>",
    "<!--",
    "&amp;",
    "&#0;",
    "&#x110000;",
    "<?pi?>",
    "\u{0}",
    "<a>",
    "</a>",
    " ",
    "<Note id=\"99\"/>",
    "<Bars>-1</Bars>",
    "<!DOCTYPE x>",
    "\"",
];
const NUMBERS: [&str; 14] = [
    "-1",
    "0",
    "1",
    "2",
    "99999999999999999999",
    "4294967296",
    "-0",
    "1e999",
    "0.5",
    "",
    "NaN",
    " ",
    "1/0",
    "0/0",
];
const TAGS: [&str; 8] = [
    "Note",
    "Beat",
    "Bar",
    "Voice",
    "Track",
    "MasterBar",
    "Rhythm",
    "Property",
];

#[derive(Debug, Clone, Copy)]
struct ByteFlip {
    at: usize,
    xor: u8,
}

#[derive(Debug, Clone, Copy)]
enum HeaderPart {
    Local,
    Central,
    End,
}

#[derive(Debug, Clone, Copy)]
struct Layout {
    deflate: bool,
    descriptor: bool,
}

#[derive(Debug, Clone, Copy)]
enum EntryOp {
    Keep,
    Delete(usize),
    Duplicate(usize),
    Rename(usize, usize),
    Swap(usize, usize),
}

#[derive(Debug, Clone, Copy)]
enum SpanAction {
    Delete,
    Duplicate,
}

#[derive(Debug, Clone, Copy)]
enum XmlEdit {
    Delete {
        at: usize,
        length: usize,
    },
    Duplicate {
        at: usize,
        length: usize,
        to: usize,
    },
    Insert {
        at: usize,
        snippet: usize,
    },
    Number {
        nth: usize,
        value: usize,
    },
    Attribute {
        nth: usize,
    },
    CloseTag {
        nth: usize,
    },
    Swap {
        at: usize,
    },
    Element {
        tag: usize,
        nth: usize,
        action: SpanAction,
    },
    Truncate {
        at: usize,
    },
}

#[derive(Debug, Clone)]
enum Mutation {
    Flip(Vec<ByteFlip>),
    Truncate(usize),
    Append(Vec<u8>),
    Rebuild {
        layout: Layout,
        op: EntryOp,
    },
    Header {
        part: HeaderPart,
        which: usize,
        field: usize,
        value: u8,
    },
    Xml {
        layout: Layout,
        edit: XmlEdit,
    },
}

struct Member {
    name: Vec<u8>,
    data: Vec<u8>,
}

fn writable() -> ScoreGenConfig {
    ScoreGenConfig {
        gp_writable: true,
        ..ScoreGenConfig::default()
    }
}

fn generated_seeds() -> Vec<Vec<u8>> {
    let strategy = arb_score_with(writable());
    let mut runner = TestRunner::deterministic();
    let mut seeds = Vec::new();
    for _ in 0..SEED_ATTEMPTS {
        if seeds.len() == SEED_POOL {
            break;
        }
        let Ok(tree) = strategy.new_tree(&mut runner) else {
            continue;
        };
        if let Ok(bytes) = write_gp(&tree.current()) {
            seeds.push(bytes);
        }
    }
    seeds
}

fn corpus_seeds() -> Vec<Vec<u8>> {
    corpus_paths()
        .unwrap_or_default()
        .iter()
        .filter_map(|path| std::fs::read(path).ok())
        .collect()
}

static SEEDS: LazyLock<Vec<Vec<u8>>> = LazyLock::new(|| {
    let mut seeds = generated_seeds();
    seeds.extend(corpus_seeds());
    seeds
});

fn members_of(bytes: &[u8]) -> Option<Vec<Member>> {
    let mut archive = ZipArchive::new(std::io::Cursor::new(bytes)).ok()?;
    let mut members = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).ok()?;
        let name = file.name_raw().to_vec();
        let mut data = Vec::new();
        (&mut file).take(MEMBER_LIMIT).read_to_end(&mut data).ok()?;
        members.push(Member { name, data });
    }
    Some(members)
}

fn rebuilt(members: &[Member], layout: Layout) -> Vec<u8> {
    let entries: Vec<RawEntry> = members
        .iter()
        .map(|member| {
            let mut entry = if layout.deflate {
                RawEntry::deflated("", &member.data)
            } else {
                RawEntry::stored("", &member.data)
            };
            entry.name = member.name.clone();
            if layout.descriptor {
                entry.with_descriptor()
            } else {
                entry
            }
        })
        .collect();
    build(&entries)
}

fn offsets_of(bytes: &[u8], signature: [u8; 4]) -> Vec<usize> {
    bytes
        .windows(signature.len())
        .enumerate()
        .filter(|(_, window)| *window == signature)
        .map(|(at, _)| at)
        .collect()
}

fn flip(mut bytes: Vec<u8>, flips: &[ByteFlip]) -> Vec<u8> {
    if bytes.is_empty() {
        return bytes;
    }
    for flip in flips {
        let at = flip.at % bytes.len();
        bytes[at] ^= flip.xor;
    }
    bytes
}

fn corrupt_header(
    mut bytes: Vec<u8>,
    part: HeaderPart,
    which: usize,
    field: usize,
    value: u8,
) -> Vec<u8> {
    let (signature, size) = match part {
        HeaderPart::Local => (LOCAL_SIGNATURE, LOCAL_HEADER_SIZE),
        HeaderPart::Central => (CENTRAL_SIGNATURE, CENTRAL_HEADER_SIZE),
        HeaderPart::End => (END_SIGNATURE, END_RECORD_SIZE),
    };
    let offsets = offsets_of(&bytes, signature);
    if offsets.is_empty() {
        return bytes;
    }
    let at = offsets[which % offsets.len()] + field % size;
    if let Some(byte) = bytes.get_mut(at) {
        *byte = value;
    }
    bytes
}

fn rebuild_with(bytes: Vec<u8>, layout: Layout, op: EntryOp) -> Vec<u8> {
    let Some(mut members) = members_of(&bytes) else {
        return bytes;
    };
    let count = members.len().max(1);
    match op {
        EntryOp::Keep => {}
        EntryOp::Delete(index) => {
            if !members.is_empty() {
                members.remove(index % count);
            }
        }
        EntryOp::Duplicate(index) => {
            if !members.is_empty() {
                let source = &members[index % count];
                let copy = Member {
                    name: source.name.clone(),
                    data: source.data.clone(),
                };
                members.push(copy);
            }
        }
        EntryOp::Rename(index, name) => {
            if !members.is_empty() {
                members[index % count].name = NAMES[name % NAMES.len()].as_bytes().to_vec();
            }
        }
        EntryOp::Swap(first, second) => {
            if !members.is_empty() {
                members.swap(first % count, second % count);
            }
        }
    }
    rebuilt(&members, layout)
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from > haystack.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|at| at + from)
}

fn nth_match(data: &[u8], needle: &[u8], nth: usize) -> Option<usize> {
    let mut positions = Vec::new();
    let mut from = 0;
    while let Some(at) = find(data, needle, from) {
        positions.push(at);
        from = at + 1;
    }
    (!positions.is_empty()).then(|| positions[nth % positions.len()])
}

fn digit_runs(data: &[u8]) -> Vec<(usize, usize)> {
    let mut runs = Vec::new();
    let mut start = None;
    for (at, byte) in data.iter().enumerate() {
        match (byte.is_ascii_digit(), start) {
            (true, None) => start = Some(at),
            (false, Some(begin)) => {
                runs.push((begin, at));
                start = None;
            }
            _ => {}
        }
    }
    runs
}

fn element_span(data: &[u8], tag: usize, nth: usize) -> Option<(usize, usize)> {
    let name = TAGS[tag % TAGS.len()];
    let open = format!("<{name} ");
    let start = nth_match(data, open.as_bytes(), nth)?;
    let close = format!("</{name}>");
    let end = find(data, close.as_bytes(), start)? + close.len();
    Some((start, end))
}

fn perturb_xml(mut data: Vec<u8>, edit: XmlEdit) -> Vec<u8> {
    let size = data.len();
    match edit {
        XmlEdit::Delete { at, length } => {
            let start = at % (size + 1);
            let end = (start + length % 200 + 1).min(size);
            data.drain(start..end);
        }
        XmlEdit::Duplicate { at, length, to } => {
            let start = at % (size + 1);
            let end = (start + length % 200 + 1).min(size);
            let copy = data[start..end].to_vec();
            let target = to % (size + 1);
            data.splice(target..target, copy);
        }
        XmlEdit::Insert { at, snippet } => {
            let target = at % (size + 1);
            let text = SNIPPETS[snippet % SNIPPETS.len()].as_bytes().to_vec();
            data.splice(target..target, text);
        }
        XmlEdit::Number { nth, value } => {
            let runs = digit_runs(&data);
            if !runs.is_empty() {
                let (start, end) = runs[nth % runs.len()];
                let text = NUMBERS[value % NUMBERS.len()].as_bytes().to_vec();
                data.splice(start..end, text);
            }
        }
        XmlEdit::Attribute { nth } => {
            if let Some(at) = nth_match(&data, b"=\"", nth)
                && let Some(start) = data[..at].iter().rposition(|byte| *byte == b' ')
                && let Some(end) = find(&data, b"\"", at + 2)
            {
                data.drain(start..=end);
            }
        }
        XmlEdit::CloseTag { nth } => {
            if let Some(start) = nth_match(&data, b"</", nth)
                && let Some(end) = find(&data, b">", start)
            {
                data.drain(start..=end);
            }
        }
        XmlEdit::Swap { at } => {
            if size >= 2 {
                data.swap(at % (size - 1), at % (size - 1) + 1);
            }
        }
        XmlEdit::Element { tag, nth, action } => {
            if let Some((start, end)) = element_span(&data, tag, nth) {
                match action {
                    SpanAction::Delete => {
                        data.drain(start..end);
                    }
                    SpanAction::Duplicate => {
                        let copy = data[start..end].to_vec();
                        data.splice(end..end, copy);
                    }
                }
            }
        }
        XmlEdit::Truncate { at } => data.truncate(at % (size + 1)),
    }
    data
}

fn xml_mutation(bytes: Vec<u8>, layout: Layout, edit: XmlEdit) -> Vec<u8> {
    let Some(mut members) = members_of(&bytes) else {
        return bytes;
    };
    let score = SCORE_ENTRY.as_bytes();
    let Some(member) = members.iter_mut().find(|member| member.name == score) else {
        return bytes;
    };
    member.data = perturb_xml(std::mem::take(&mut member.data), edit);
    rebuilt(&members, layout)
}

fn mutate(bytes: Vec<u8>, mutation: &Mutation) -> Vec<u8> {
    match mutation {
        Mutation::Flip(flips) => flip(bytes, flips),
        Mutation::Truncate(at) => {
            let end = at % (bytes.len() + 1);
            bytes[..end].to_vec()
        }
        Mutation::Append(extra) => {
            let mut bytes = bytes;
            bytes.extend_from_slice(extra);
            bytes
        }
        Mutation::Rebuild { layout, op } => rebuild_with(bytes, *layout, *op),
        Mutation::Header {
            part,
            which,
            field,
            value,
        } => corrupt_header(bytes, *part, *which, *field, *value),
        Mutation::Xml { layout, edit } => xml_mutation(bytes, *layout, *edit),
    }
}

fn arb_layout() -> impl Strategy<Value = Layout> {
    (any::<bool>(), any::<bool>()).prop_map(|(deflate, descriptor)| Layout {
        deflate,
        descriptor,
    })
}

fn arb_op() -> impl Strategy<Value = EntryOp> {
    prop_oneof![
        Just(EntryOp::Keep),
        any::<usize>().prop_map(EntryOp::Delete),
        any::<usize>().prop_map(EntryOp::Duplicate),
        (any::<usize>(), any::<usize>()).prop_map(|(index, name)| EntryOp::Rename(index, name)),
        (any::<usize>(), any::<usize>()).prop_map(|(first, second)| EntryOp::Swap(first, second)),
    ]
}

fn arb_part() -> impl Strategy<Value = HeaderPart> {
    prop_oneof![
        Just(HeaderPart::Local),
        Just(HeaderPart::Central),
        Just(HeaderPart::End)
    ]
}

fn arb_edit() -> impl Strategy<Value = XmlEdit> {
    prop_oneof![
        (any::<usize>(), any::<usize>()).prop_map(|(at, length)| XmlEdit::Delete { at, length }),
        (any::<usize>(), any::<usize>(), any::<usize>())
            .prop_map(|(at, length, to)| XmlEdit::Duplicate { at, length, to }),
        (any::<usize>(), any::<usize>()).prop_map(|(at, snippet)| XmlEdit::Insert { at, snippet }),
        (any::<usize>(), any::<usize>()).prop_map(|(nth, value)| XmlEdit::Number { nth, value }),
        any::<usize>().prop_map(|nth| XmlEdit::Attribute { nth }),
        any::<usize>().prop_map(|nth| XmlEdit::CloseTag { nth }),
        any::<usize>().prop_map(|at| XmlEdit::Swap { at }),
        (any::<usize>(), any::<usize>(), any::<bool>()).prop_map(|(tag, nth, delete)| {
            let action = if delete {
                SpanAction::Delete
            } else {
                SpanAction::Duplicate
            };
            XmlEdit::Element { tag, nth, action }
        }),
        any::<usize>().prop_map(|at| XmlEdit::Truncate { at }),
    ]
}

fn arb_flip() -> impl Strategy<Value = ByteFlip> {
    (any::<usize>(), 1u8..=255).prop_map(|(at, xor)| ByteFlip { at, xor })
}

fn arb_mutation() -> impl Strategy<Value = Mutation> {
    prop_oneof![
        2 => vec(arb_flip(), 1..=8).prop_map(Mutation::Flip),
        1 => any::<usize>().prop_map(Mutation::Truncate),
        1 => vec(any::<u8>(), 1..=16).prop_map(Mutation::Append),
        3 => (arb_layout(), arb_op()).prop_map(|(layout, op)| Mutation::Rebuild { layout, op }),
        2 => (arb_part(), any::<usize>(), any::<usize>(), any::<u8>())
            .prop_map(|(part, which, field, value)| Mutation::Header { part, which, field, value }),
        6 => (arb_layout(), arb_edit()).prop_map(|(layout, edit)| Mutation::Xml { layout, edit }),
    ]
}

#[derive(Default)]
struct Tally {
    accepted: Cell<usize>,
    rejected: Cell<usize>,
    written: Cell<usize>,
    refused: Cell<usize>,
}

fn bump(cell: &Cell<usize>) {
    cell.set(cell.get() + 1);
}

fn check(bytes: &[u8], tally: &Tally) -> Result<(), TestCaseError> {
    let Ok(score) = read_gp(bytes) else {
        bump(&tally.rejected);
        return Ok(());
    };
    bump(&tally.accepted);
    let json = to_json(&score).map_err(|error| TestCaseError::fail(error.to_string()))?;
    let back = from_json(&json).map_err(|error| TestCaseError::fail(error.to_string()))?;
    prop_assert_eq!(&back, &score);
    match write_gp(&score) {
        Ok(written) => {
            bump(&tally.written);
            read_gp(&written).map_err(|error| TestCaseError::fail(error.to_string()))?;
        }
        Err(GpError::Unsupported { reason, .. }) => {
            bump(&tally.refused);
            prop_assert!(documented(&reason), "{reason:?}");
        }
        Err(other) => return Err(TestCaseError::fail(format!("unexpected error {other}"))),
    }
    Ok(())
}

fn fuzz(cases: u32) -> Tally {
    let seeds = &*SEEDS;
    assert!(seeds.len() >= SEED_POOL / 2);
    let tally = Tally::default();
    let mut runner = TestRunner::new(Config {
        cases,
        failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
        ..Config::default()
    });
    let strategy = (0..seeds.len(), vec(arb_mutation(), 1..=4));
    let outcome = runner.run(&strategy, |(seed, mutations)| {
        let mut bytes = seeds[seed].clone();
        for mutation in &mutations {
            bytes = mutate(bytes, mutation);
        }
        check(&bytes, &tally)
    });
    if let Err(error) = outcome {
        panic!("{error}");
    }
    tally
}

fn report(tally: &Tally) {
    println!(
        "fuzz: accepted {} rejected {} written {} refused {}",
        tally.accepted.get(),
        tally.rejected.get(),
        tally.written.get(),
        tally.refused.get()
    );
}

#[test]
fn every_seed_is_a_valid_archive_that_reads() {
    assert!(SEEDS.len() >= SEED_POOL / 2);
    for seed in SEEDS.iter() {
        assert!(read_gp(seed).is_ok());
    }
}

#[test]
fn mutated_archives_never_panic() {
    let tally = fuzz(CASES);
    report(&tally);
    assert!(tally.accepted.get() > 0);
    assert!(tally.rejected.get() > 0);
    assert!(tally.written.get() > 0);
}

#[test]
#[ignore = "20 000 cases; run at milestone review"]
fn mutated_archives_never_panic_long() {
    let tally = fuzz(LONG_CASES);
    report(&tally);
    assert!(tally.accepted.get() > 0);
    assert!(tally.rejected.get() > 0);
}
