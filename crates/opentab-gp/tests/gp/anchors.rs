use std::io::{Cursor, Read};

use opentab_command::{Command, History, apply};
use opentab_gp::{GpError, Refusal, read_gp, write_gp};
use opentab_model::{OpaquePayload, Score, TimeSignature};
use zip::ZipArchive;

use crate::corpus::corpus_paths;
use crate::rich::{FRAME_OFFSETS, SOUND_MARKER, rich};
use crate::support::{SCORE_ENTRY, document_payloads, payloads};

pub fn gpif_text(bytes: &[u8]) -> String {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).expect("zip");
    let mut entry = archive.by_name(SCORE_ENTRY).expect("score entry");
    let mut text = String::new();
    entry.read_to_string(&mut text).expect("utf-8");
    text
}

pub fn field(block: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = block.find(&open)? + open.len();
    let end = block[start..].find(&close)? + start;
    Some(block[start..end].to_string())
}

pub fn blocks(text: &str, name: &str) -> Vec<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(&open) {
        let Some(end) = rest[start..].find(&close) else {
            break;
        };
        found.push(rest[start..start + end + close.len()].to_string());
        rest = &rest[start + end + close.len()..];
    }
    found
}

fn section<'t>(text: &'t str, open: &str, close: &str) -> &'t str {
    let start = text.find(open).expect("section start");
    let end = text[start..].find(close).expect("section end") + start;
    &text[start..end]
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Point {
    pub place: String,
    pub kind: String,
    pub bar: usize,
    pub index: Option<usize>,
    pub occurrence: Option<usize>,
    pub value: String,
}

fn number(block: &str, name: &str) -> Option<usize> {
    field(block, name).and_then(|text| text.trim().parse().ok())
}

fn without(text: &str, name: &str) -> String {
    match field(text, name) {
        Some(inner) => text.replace(&format!("<{name}>{inner}</{name}>"), ""),
        None => text.to_string(),
    }
}

fn points_in(place: &str, text: &str, points: &mut Vec<Point>) {
    for block in blocks(text, "Automation") {
        let kind = field(&block, "Type").unwrap_or_default();
        if kind == "Tempo" {
            continue;
        }
        points.push(Point {
            place: place.to_string(),
            bar: number(&block, "Bar").expect("automation bar"),
            index: number(&block, "BarIndex"),
            occurrence: number(&block, "BarOccurrence"),
            value: without(&field(&block, "Value").unwrap_or_default(), "BarIndex"),
            kind,
        });
    }
}

pub fn points(text: &str) -> Vec<Point> {
    let mut points = Vec::new();
    points_in(
        "master",
        section(text, "<MasterTrack>", "</MasterTrack>"),
        &mut points,
    );
    let tracks = section(text, "<Tracks>\n<Track", "</Tracks>");
    for (index, track) in tracks.split("<Track id=").skip(1).enumerate() {
        points_in(&format!("track {index}"), track, &mut points);
    }
    points
}

pub fn offsets(text: &str) -> Vec<usize> {
    let tracks = section(text, "<Tracks>\n<Track", "</Tracks>");
    blocks(tracks, "Line")
        .iter()
        .filter_map(|line| number(line, "Offset"))
        .collect()
}

fn written(score: &Score) -> String {
    gpif_text(&write_gp(score).expect("write"))
}

fn edited(command: Command) -> String {
    let mut score = rich().score;
    apply(&mut score, &command).expect("command applies");
    let text = written(&score);
    let again = read_gp(&write_gp(&score).expect("write")).expect("read back");
    assert_eq!(
        write_gp(&again).expect("rewrite"),
        write_gp(&score).expect("write")
    );
    text
}

fn point<'p>(points: &'p [Point], marker: &str) -> Option<&'p Point> {
    points.iter().find(|point| point.value.contains(marker))
}

fn insert(index: usize) -> Command {
    Command::InsertMasterBar {
        index,
        time_signature: TimeSignature::new(4, 4).expect("time"),
    }
}

#[test]
fn reading_anchors_every_bar_reference_to_its_master_bar() {
    let rich = rich();
    let ids: Vec<u64> = rich.master_ids.iter().map(|id| id.0).collect();
    let track = payloads(&rich.score, rich.score.tracks[0].id.0);
    let anchored = |needle: &str, bars: String| {
        track
            .iter()
            .any(|data| data.contains(needle) && data.contains(&format!(" bars=\"{bars}\"")))
    };
    assert!(anchored("DSPParam_11", ids[1].to_string()), "{track:?}");
    assert!(anchored(SOUND_MARKER, ids[2].to_string()), "{track:?}");
    assert!(
        anchored("<Lyrics", format!("{} {}", ids[1], ids[0])),
        "{track:?}"
    );
    let document = document_payloads(&rich.score);
    let expected = [
        format!("{}:0", ids[0]),
        format!("{}:0", ids[2]),
        format!("{}:0", ids[3]),
        ids[3].to_string(),
    ];
    for bars in expected {
        assert!(
            document
                .iter()
                .any(|data| data.contains(&format!(" bars=\"{bars}\""))),
            "{bars}: {document:?}"
        );
    }
}

#[test]
fn an_unchanged_score_writes_every_bar_reference_unchanged() {
    let text = written(&rich().score);
    let points = points(&text);
    assert_eq!(point(&points, SOUND_MARKER).map(|point| point.bar), Some(2));
    assert_eq!(offsets(&text), vec![1, 0]);
    let syncs: Vec<(usize, Option<usize>, Option<usize>)> = points
        .iter()
        .filter(|point| point.kind == "SyncPoint")
        .map(|point| (point.bar, point.index, point.occurrence))
        .collect();
    assert_eq!(
        syncs,
        vec![
            (2, Some(0), Some(1)),
            (4, Some(2), Some(0)),
            (5, Some(3), Some(0))
        ]
    );
}

fn sync_of(points: &[Point], frame: &str) -> Option<(usize, Option<usize>)> {
    point(points, &format!("<FrameOffset>{frame}</FrameOffset>"))
        .map(|point| (point.bar, point.index))
}

#[test]
fn inserting_a_master_bar_moves_every_bar_reference() {
    let text = edited(insert(0));
    let points = points(&text);
    assert_eq!(point(&points, SOUND_MARKER).map(|point| point.bar), Some(3));
    assert_eq!(point(&points, "0.25").map(|point| point.bar), Some(2));
    assert_eq!(
        points
            .iter()
            .find(|point| point.kind == "DSPParam_00")
            .map(|point| point.bar),
        Some(4)
    );
    assert_eq!(offsets(&text), vec![2, 1]);
    assert_eq!(sync_of(&points, FRAME_OFFSETS[0]), Some((3, Some(1))));
    assert_eq!(sync_of(&points, FRAME_OFFSETS[1]), Some((5, Some(3))));
    assert_eq!(sync_of(&points, FRAME_OFFSETS[2]), Some((6, Some(4))));
}

#[test]
fn inserting_inside_a_repeat_counts_every_pass() {
    let points = points(&edited(insert(1)));
    assert_eq!(sync_of(&points, FRAME_OFFSETS[0]), Some((3, Some(0))));
    assert_eq!(sync_of(&points, FRAME_OFFSETS[1]), Some((6, Some(3))));
    assert_eq!(sync_of(&points, FRAME_OFFSETS[2]), Some((7, Some(4))));
}

#[test]
fn removing_the_last_master_bar_drops_its_automations() {
    let text = edited(Command::RemoveMasterBar { index: 3 });
    let points = points(&text);
    assert!(!points.iter().any(|point| point.kind == "DSPParam_00"));
    assert_eq!(sync_of(&points, FRAME_OFFSETS[2]), None);
    assert_eq!(sync_of(&points, FRAME_OFFSETS[1]), Some((4, Some(2))));
    assert_eq!(point(&points, SOUND_MARKER).map(|point| point.bar), Some(2));
}

#[test]
fn removing_a_repeat_end_drops_the_passes_it_made_and_moves_lyrics_on() {
    let text = edited(Command::RemoveMasterBar { index: 1 });
    let points = points(&text);
    assert_eq!(sync_of(&points, FRAME_OFFSETS[0]), None);
    assert_eq!(sync_of(&points, FRAME_OFFSETS[1]), Some((1, Some(1))));
    assert_eq!(sync_of(&points, FRAME_OFFSETS[2]), Some((2, Some(2))));
    assert_eq!(point(&points, "0.25"), None);
    assert_eq!(point(&points, SOUND_MARKER).map(|point| point.bar), Some(1));
    assert_eq!(offsets(&text), vec![1, 0]);
    let count = text.matches("<MasterBar>").count();
    for point in &points {
        assert!(point.index.is_none_or(|index| index < count), "{point:?}");
        if point.kind != "SyncPoint" {
            assert!(point.bar < count, "{point:?}");
        }
    }
}

#[test]
fn undoing_the_edits_gives_the_original_bytes() {
    let mut score = rich().score;
    let original = write_gp(&score).expect("write");
    let mut history = History::new();
    for command in [
        Command::RemoveMasterBar { index: 1 },
        insert(0),
        Command::RemoveMasterBar { index: 3 },
    ] {
        history.execute(&mut score, command).expect("applies");
    }
    assert_ne!(write_gp(&score).expect("write"), original);
    while history.undo(&mut score).expect("undo") {}
    assert_eq!(write_gp(&score).expect("write"), original);
}

fn with_track_payload(data: &str) -> Score {
    let mut score = rich().score;
    let track = score.tracks[0].id.0;
    score.extensions.0.insert(
        track,
        vec![OpaquePayload {
            format: "gpif".to_string(),
            data: data.to_string(),
        }],
    );
    score
}

#[test]
fn malformed_bar_anchors_are_refused() {
    let node = "<Automations><Automation><Type>Sound</Type><Bar>0</Bar></Automation></Automations>";
    for bars in ["x", "1 2", "", "3:x", "+3", "03"] {
        let score = with_track_payload(&format!(
            "<Keep parent=\"\" index=\"9\" bars=\"{bars}\">{node}</Keep>"
        ));
        let refused = matches!(
            write_gp(&score),
            Err(GpError::Unsupported {
                reason: Refusal::MalformedPayload { .. },
                ..
            })
        );
        assert!(refused, "{bars}");
    }
    let id = rich().score.master_bars[1].id.0.0;
    let wrong_kind = with_track_payload(&format!(
        "<Keep parent=\"\" index=\"9\" bars=\"{id}:0\">{node}</Keep>"
    ));
    assert!(matches!(
        write_gp(&wrong_kind),
        Err(GpError::Unsupported {
            reason: Refusal::MalformedPayload { .. },
            ..
        })
    ));
}

#[test]
fn unanchored_payloads_are_written_as_they_are() {
    let node = "<Automations><Automation><Type>Sound</Type><Bar>7</Bar></Automation></Automations>";
    for keep in [
        format!("<Keep parent=\"\" index=\"9\">{node}</Keep>"),
        format!("<Keep parent=\"\" index=\"9\" bars=\"-\">{node}</Keep>"),
    ] {
        let text = written(&with_track_payload(&keep));
        assert!(text.contains("<Bar>7</Bar>"), "{keep}");
    }
    let id = rich().score.master_bars[1].id.0.0;
    let anchored = with_track_payload(&format!(
        "<Keep parent=\"\" index=\"9\" bars=\"{id}\">{node}</Keep>"
    ));
    assert!(written(&anchored).contains("<Bar>1</Bar>"));
    let gone = with_track_payload(&format!(
        "<Keep parent=\"\" index=\"9\" bars=\"999999\">{node}</Keep>"
    ));
    let text = written(&gone);
    assert!(!text.contains("<Bar>7</Bar>") && text.contains("<Automations/>"));
}

fn corpus_points(score: &Score) -> (Vec<Point>, Vec<usize>, usize) {
    let text = written(score);
    let count = text.matches("<MasterBar>").count();
    (points(&text), offsets(&text), count)
}

#[test]
fn corpus_bar_references_follow_their_master_bars() {
    let Some(paths) = corpus_paths() else {
        return;
    };
    let mut checked = 0usize;
    for path in &paths {
        let name = path.display().to_string();
        let original = read_gp(&std::fs::read(path).expect("corpus file")).expect("reads");
        let (before, lyrics, count) = corpus_points(&original);
        let mut moved = original.clone();
        apply(&mut moved, &insert(0)).expect("insert at 0");
        let (after, shifted, _) = corpus_points(&moved);
        let expected: Vec<Point> = before
            .iter()
            .map(|point| Point {
                bar: point.bar + 1,
                index: point.index.map(|index| index + 1),
                ..point.clone()
            })
            .collect();
        assert_eq!(after, expected, "{name}: insert at 0");
        let lifted: Vec<usize> = lyrics.iter().map(|offset| offset + 1).collect();
        assert_eq!(shifted, lifted, "{name}: lyrics");
        let mut shortened = original.clone();
        apply(
            &mut shortened,
            &Command::RemoveMasterBar {
                index: count.saturating_sub(1),
            },
        )
        .expect("remove the last bar");
        let (kept, _, remaining) = corpus_points(&shortened);
        for point in &kept {
            assert!(before.contains(point), "{name}: {point:?} is new");
            assert!(point.index.is_none_or(|index| index < remaining), "{name}");
            if point.kind != "SyncPoint" {
                assert!(point.bar < remaining, "{name}: {point:?}");
            }
        }
        for point in &before {
            if point.kind != "SyncPoint" && point.bar < remaining {
                assert!(kept.contains(point), "{name}: {point:?} was lost");
            }
        }
        checked += before.len();
    }
    assert_eq!(checked, 810, "bar references in the corpus");
}
