use opentab_layout::{
    BarRange, LayoutError, LayoutOptions, Micrometres, PageSize, StaffMode, layout,
};
use opentab_model::Score;
use opentab_testkit::arb_score;
use proptest::prelude::*;
use proptest::sample::subsequence;

use crate::support::check;

const CASES: u32 = 256;
const MIN_PAGE_WIDTH_MM: i64 = 120;
const MAX_PAGE_WIDTH_MM: i64 = 300;
const MIN_PAGE_HEIGHT_MM: i64 = 100;
const MAX_PAGE_HEIGHT_MM: i64 = 420;
const MIN_STAFF_SPACE_UM: i64 = 1_000;
const MAX_STAFF_SPACE_UM: i64 = 2_500;

fn arb_mode() -> impl Strategy<Value = Option<StaffMode>> {
    proptest::option::of(prop_oneof![
        Just(StaffMode::Standard),
        Just(StaffMode::Tab),
        Just(StaffMode::Both),
    ])
}

fn arb_case() -> impl Strategy<Value = (Score, LayoutOptions)> {
    arb_score().prop_flat_map(|score| {
        let tracks = score.tracks.len();
        let bars = score.master_bars.len().max(1);
        let selection =
            proptest::option::of(subsequence((0..tracks).collect::<Vec<_>>(), 0..=tracks))
                .prop_map(|chosen| chosen.filter(|indices| !indices.is_empty()));
        let range = proptest::option::of((0..bars, 0..bars).prop_map(|(a, b)| BarRange {
            first: a.min(b),
            last: a.max(b),
        }));
        let page = (
            MIN_PAGE_WIDTH_MM..=MAX_PAGE_WIDTH_MM,
            MIN_PAGE_HEIGHT_MM..=MAX_PAGE_HEIGHT_MM,
        )
            .prop_map(|(width, height)| PageSize {
                width: Micrometres::from_mm(width),
                height: Micrometres::from_mm(height),
            });
        (
            Just(score),
            selection,
            range,
            arb_mode(),
            page,
            MIN_STAFF_SPACE_UM..=MAX_STAFF_SPACE_UM,
        )
            .prop_map(|(score, tracks, bars, staff_mode, page, space)| {
                let bars = bars.filter(|_| !score.master_bars.is_empty());
                let options = LayoutOptions {
                    tracks,
                    bars,
                    staff_mode,
                    page,
                    staff_space: Micrometres(space),
                };
                (score, options)
            })
    })
}

fn has_staves(score: &Score, options: &LayoutOptions) -> bool {
    match &options.tracks {
        Some(chosen) => chosen
            .iter()
            .any(|index| !score.tracks[*index].staves.is_empty()),
        None => score.tracks.iter().any(|track| !track.staves.is_empty()),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn layout_of_any_score_keeps_every_invariant((score, options) in arb_case()) {
        let result = layout(&score, &options);
        if score.master_bars.is_empty() {
            prop_assert_eq!(result, Err(LayoutError::NoBars));
        } else if !has_staves(&score, &options) {
            prop_assert_eq!(result, Err(LayoutError::NoStaves));
        } else {
            let list = result.unwrap();
            check(&score, &options, &list);
            prop_assert_eq!(layout(&score, &options).unwrap(), list);
        }
    }

    #[test]
    fn layout_at_the_default_options_keeps_every_invariant(score in arb_score()) {
        let options = LayoutOptions::default();
        if let Ok(list) = layout(&score, &options) {
            check(&score, &options, &list);
        } else {
            prop_assert!(score.master_bars.is_empty() || !has_staves(&score, &options));
        }
    }
}
