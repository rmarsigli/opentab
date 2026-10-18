use opentab_layout::rhythm::split_wide;
use opentab_layout::{GroupId, StaffMode};
use opentab_model::{NoteValue, Score};

use crate::music::{
    beat_id, build, glyphs, group_in, measure, named, options, play, render, stem_of,
};
use crate::support::systems;

fn sixteenths(notes: &[(u8, u8)]) -> Score {
    build(&[measure(
        (1, 4),
        0,
        vec![(
            0,
            notes
                .iter()
                .map(|note| play(NoteValue::Sixteenth, &[*note]))
                .collect(),
        )],
    )])
}

fn flagged(list: &opentab_layout::DisplayList, score: &Score, beat: usize) -> bool {
    named(&glyphs(
        &group_in(list, GroupId(beat_id(score, 0, 0, beat).0.0)).items,
    ))
    .iter()
    .any(|name| name.starts_with("flag"))
}

#[test]
fn a_beam_group_spanning_too_wide_a_range_breaks_at_its_largest_leap() {
    let wide = sixteenths(&[(5, 0), (0, 20), (0, 17), (0, 20)]);
    let list = render(&wide, &options(StaffMode::Standard));
    assert!(flagged(&list, &wide, 0), "the low note leaves the beam");
    for beat in 1..4 {
        assert!(!flagged(&list, &wide, beat), "beat {beat} stays beamed");
    }
    let staff = &systems(&list)[0].staves[0];
    for beat in 0..4 {
        let stem = stem_of(&list, beat_id(&wide, 0, 0, beat));
        let top = stem.from.y.min(stem.to.y);
        let bottom = stem.from.y.max(stem.to.y);
        assert!(
            top > staff.top || bottom < staff.bottom(),
            "the stem of beat {beat} crosses the whole staff"
        );
    }
    let joined = split_wide(vec![0, 1], &[]);
    assert_eq!(joined, vec![vec![0, 1]]);
    let near = sixteenths(&[(2, 5), (0, 8), (2, 5), (0, 8)]);
    let list = render(&near, &options(StaffMode::Standard));
    for beat in 0..4 {
        assert!(
            !flagged(&list, &near, beat),
            "a narrow group keeps one beam"
        );
    }
}
