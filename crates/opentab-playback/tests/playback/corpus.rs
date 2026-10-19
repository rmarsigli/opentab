use std::collections::{BTreeMap, VecDeque};
use std::io::{Cursor, Read};
use std::path::PathBuf;

use opentab_gp::read_gp;
use opentab_playback::{
    MidiMessage, PlaybackOptions, Profile, RampGrid, ScoreTime, Speed, TempoMap, beat_grid,
    perform, play_order,
};
use zip::ZipArchive;

const CORPUS_VARIABLE: &str = "OPENTAB_CORPUS";
const SCORE_ENTRY: &str = "Content/score.gpif";
const EXPECTED_FILES: usize = 27;
const SYNC_POINT: &str = "<Type>SyncPoint</Type>";
const EXPECTED_SYNC_POINTS: usize = 546;
const EXPECTED_SYNC_FILES: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SyncPoint {
    bar: usize,
    index: usize,
    occurrence: usize,
}

fn corpus() -> Option<Vec<PathBuf>> {
    let dir = PathBuf::from(std::env::var_os(CORPUS_VARIABLE)?);
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("corpus directory")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "gp"))
        .collect();
    paths.sort();
    Some(paths)
}

fn gpif(bytes: &[u8]) -> String {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).expect("zip");
    let mut entry = archive.by_name(SCORE_ENTRY).expect("score entry");
    let mut text = String::new();
    entry.read_to_string(&mut text).expect("utf-8");
    text
}

fn field(block: &str, name: &str) -> Option<usize> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = block.find(&open)? + open.len();
    let end = block[start..].find(&close)? + start;
    block[start..end].trim().parse().ok()
}

fn sync_points(text: &str) -> Vec<SyncPoint> {
    let Some(start) = text.find("<MasterTrack>") else {
        return Vec::new();
    };
    let end = text[start..]
        .find("</MasterTrack>")
        .map_or(text.len(), |end| start + end);
    text[start..end]
        .split("<Automation>")
        .skip(1)
        .filter(|block| block.contains(SYNC_POINT))
        .map(|block| SyncPoint {
            bar: field(block, "Bar").expect("sync bar"),
            index: field(block, "BarIndex").expect("sync bar index"),
            occurrence: field(block, "BarOccurrence").expect("sync occurrence"),
        })
        .collect()
}

#[test]
fn every_corpus_sync_point_sits_at_its_place_in_our_play_order() {
    let Some(paths) = corpus() else {
        return;
    };
    assert_eq!(paths.len(), EXPECTED_FILES);
    let mut checked = 0;
    let mut files_with_points = 0;
    for path in &paths {
        let bytes = std::fs::read(path).expect("corpus file");
        let score = read_gp(&bytes).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let order = play_order(&score);
        let points = sync_points(&gpif(&bytes));
        if !points.is_empty() {
            files_with_points += 1;
        }
        for point in points {
            let played = order.played_of(point.index);
            assert_eq!(
                played.get(point.occurrence),
                Some(&point.bar),
                "{}: {point:?}",
                path.display()
            );
            checked += 1;
        }
    }
    assert_eq!(
        (checked, files_with_points),
        (EXPECTED_SYNC_POINTS, EXPECTED_SYNC_FILES)
    );
}

#[test]
fn every_corpus_score_builds_its_tempo_map_and_beat_grid() {
    let Some(paths) = corpus() else {
        return;
    };
    for path in &paths {
        let bytes = std::fs::read(path).expect("corpus file");
        let score = read_gp(&bytes).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let order = play_order(&score);
        assert!(!order.bars.is_empty(), "{}", path.display());
        for grid in [RampGrid::MUSICAL, RampGrid::GP] {
            let map = TempoMap::build(&score, &order, grid)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            for speed in [25, 100, 200] {
                let speed = Speed::new(speed).expect("speed");
                let seconds = map
                    .seconds_at(order.end(), speed)
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
                assert_eq!(map.time_at(seconds, speed), Ok(order.end()));
            }
        }
        let clicks = beat_grid(&score, &order);
        assert_eq!(
            clicks.iter().filter(|click| click.downbeat).count(),
            order.bars.len(),
            "{}",
            path.display()
        );
    }
}

#[test]
fn every_corpus_score_performs_with_every_note_paired() {
    let Some(paths) = corpus() else {
        return;
    };
    let mut notes = 0;
    for path in &paths {
        let bytes = std::fs::read(path).expect("corpus file");
        let score = read_gp(&bytes).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        for profile in [Profile::Gp, Profile::Musical] {
            let performance = perform(&score, &PlaybackOptions { profile })
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let mut open: BTreeMap<(usize, u8, u8), VecDeque<ScoreTime>> = BTreeMap::new();
            for event in &performance.events {
                let slot = |key: u8| (event.track, event.channel.get(), key);
                match event.message {
                    MidiMessage::NoteOn { key, .. } => {
                        open.entry(slot(key)).or_default().push_back(event.at);
                        notes += 1;
                    }
                    MidiMessage::NoteOff { key, .. } => {
                        let start = open
                            .get_mut(&slot(key))
                            .and_then(VecDeque::pop_front)
                            .unwrap_or_else(|| panic!("{}: unpaired off", path.display()));
                        assert!(start < event.at, "{}", path.display());
                    }
                    _ => {}
                }
            }
            assert!(open.values().all(VecDeque::is_empty), "{}", path.display());
        }
    }
    assert!(notes > 0);
}
