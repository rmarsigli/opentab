use std::path::PathBuf;

use opentab_gp::read_gp;
use opentab_layout::{BarRange, LayoutOptions, StaffMode, layout};
use opentab_model::Score;
use opentab_render::{BarSpan, ScoreSelection, SelectionError, Summary, layout_options};

fn multi_track() -> Score {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/gp/multi-track.gp");
    read_gp(&std::fs::read(path).expect("fixture exists")).expect("fixture reads")
}

fn select(tracks: &[u32], bars: Option<(u32, u32)>) -> ScoreSelection {
    ScoreSelection {
        tracks: tracks.to_vec(),
        bars: bars.map(|(first, last)| BarSpan { first, last }),
        staff: None,
    }
}

#[test]
fn an_empty_selection_gives_the_default_options() {
    let score = multi_track();
    assert_eq!(
        layout_options(&score, &ScoreSelection::default()),
        Ok(LayoutOptions::default())
    );
}

#[test]
fn tracks_and_bars_count_from_one_and_the_staff_mode_passes_through() {
    let score = multi_track();
    let selection = ScoreSelection {
        staff: Some(StaffMode::Tab),
        ..select(&[4, 1], Some((2, 2)))
    };
    let options = layout_options(&score, &selection).unwrap();
    assert_eq!(options.tracks, Some(vec![3, 0]));
    assert_eq!(options.bars, Some(BarRange { first: 1, last: 1 }));
    assert_eq!(options.staff_mode, Some(StaffMode::Tab));
    let whole = layout_options(&score, &select(&[], Some((1, 2)))).unwrap();
    assert_eq!(whole.bars, Some(BarRange { first: 0, last: 1 }));
    assert_eq!(whole.tracks, None);
}

#[test]
fn tracks_outside_the_score_are_refused_with_the_track_count() {
    let score = multi_track();
    for number in [0, 5, u32::MAX] {
        assert_eq!(
            layout_options(&score, &select(&[1, number], None)),
            Err(SelectionError::Track { number, count: 4 }),
            "{number}"
        );
    }
    assert_eq!(
        SelectionError::Track {
            number: 5,
            count: 4
        }
        .to_string(),
        "track 5: the score has 4 track(s), numbered from 1"
    );
}

#[test]
fn bars_outside_the_score_or_reversed_are_refused_with_the_bar_count() {
    let score = multi_track();
    for (first, last) in [(0, 1), (1, 3), (3, 3), (2, 1), (1, u32::MAX)] {
        assert_eq!(
            layout_options(&score, &select(&[], Some((first, last)))),
            Err(SelectionError::Bars {
                first,
                last,
                count: 2
            }),
            "{first}-{last}"
        );
    }
    assert_eq!(
        SelectionError::Bars {
            first: 1,
            last: 3,
            count: 2
        }
        .to_string(),
        "bars 1-3: the score has 2 bar(s), numbered from 1"
    );
}

#[test]
fn the_summary_counts_pages_systems_and_drawn_bars() {
    let score = multi_track();
    let list = layout(&score, &LayoutOptions::default()).unwrap();
    let summary = Summary::of(&list);
    assert_eq!(summary.pages, 1);
    assert_eq!(summary.systems, 1);
    assert_eq!(summary.bars, 2);
    assert_eq!(summary.staff_space, 1750);
    assert!(summary.crowded.is_empty());
    assert!(summary.dropped.is_empty());
    assert!(!summary.title_page);
    let one_bar = layout_options(&score, &select(&[2], Some((2, 2)))).unwrap();
    let list = layout(&score, &one_bar).unwrap();
    assert_eq!(Summary::of(&list).bars, 1);
}
