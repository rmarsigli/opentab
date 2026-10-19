use opentab_model::Score;
use opentab_playback::{PlayedPosition, ScoreTime, play_order};

use crate::support::{frac, master_bar, numbers, plain_score, score_from};

fn sources(score: &Score) -> Vec<usize> {
    play_order(score).sources()
}

#[test]
fn bars_without_repeats_or_directions_play_once_in_order() {
    assert_eq!(sources(&plain_score(4)), [0, 1, 2, 3]);
    assert!(play_order(&Score::new()).bars.is_empty());
}

#[test]
fn twin_jumps_keep_only_the_last_copy_of_their_name() {
    let twins = score_from(5, "1 DaCapo; 3 DaCapo");
    assert_eq!(sources(&twins), numbers("0 1 2 3 0 1 2 3 4"));
    let same_bar = score_from(4, "2 DaCapo DaCapo");
    assert_eq!(sources(&same_bar), numbers("0 1 2 0 1 2 3"));
    let twin_segnos = score_from(6, "1 Segno; 3 Segno; 4 DaSegno");
    assert_eq!(sources(&twin_segnos), numbers("0 1 2 3 4 3 4 5"));
}

#[test]
fn al_fine_ends_at_the_fine_and_a_plain_jump_does_not() {
    let al_fine = score_from(6, "2 Fine; 4 DaCapoAlFine");
    assert_eq!(sources(&al_fine), numbers("0 1 2 3 4 0 1 2"));
    let plain = score_from(6, "2 Fine; 4 DaCapo");
    assert_eq!(sources(&plain), numbers("0 1 2 3 4 0 1 2 3 4 5"));
    let segno = score_from(6, "1 Segno; 2 Fine; 4 DaSegnoAlFine");
    assert_eq!(sources(&segno), numbers("0 1 2 3 4 1 2"));
}

#[test]
fn a_to_coda_never_visited_plays_our_order_not_guitar_pros_truncated_export() {
    let skipped = score_from(8, "2 DaSegno; 3 DaCoda; 4 Segno; 5 Coda");
    assert_eq!(sources(&skipped), numbers("0 1 2 4 5 6 7"));
}

#[test]
fn passes_count_the_repeat_and_restart_after_a_return_jump() {
    let score = score_from(5, "1 [S]; 2 [E3]; 3 DaCapo");
    let order = play_order(&score);
    let passes: Vec<(usize, u8)> = order
        .bars
        .iter()
        .map(|bar| (bar.source, bar.pass))
        .collect();
    assert_eq!(
        passes,
        [
            (0, 1),
            (1, 1),
            (2, 1),
            (1, 2),
            (2, 2),
            (1, 3),
            (2, 3),
            (3, 3),
            (0, 1),
            (1, 1),
            (2, 1),
            (1, 2),
            (2, 2),
            (1, 3),
            (2, 3),
            (3, 3),
            (4, 3)
        ]
    );
}

#[test]
fn an_ending_after_the_repeat_end_plays_on_its_pass_only() {
    let score = score_from(5, "0 [S]; 1 {1} [E2]; 2 {2}; 3; 4 [S]");
    assert_eq!(sources(&score), numbers("0 1 0 2 3 4"));
    let three = score_from(5, "0 [S]; 1 {1} [E3]; 2 {2} [E3]; 3 {3}");
    assert_eq!(sources(&three), numbers("0 1 0 2 0 3 4"));
}

#[test]
fn starts_accumulate_each_played_bar_length() {
    let mut score = plain_score(3);
    score.master_bars[1] = master_bar(1, 3, 8);
    score.master_bars[1].repeat_end = Some(2);
    score.master_bars[2] = master_bar(2, 7, 16);
    let order = play_order(&score);
    let starts: Vec<_> = order
        .bars
        .iter()
        .map(|bar| bar.start.whole_notes())
        .collect();
    assert_eq!(
        starts,
        [
            frac(0, 1),
            frac(1, 1),
            frac(11, 8),
            frac(19, 8),
            frac(22, 8)
        ]
    );
    let lengths: Vec<_> = order.bars.iter().map(|bar| bar.length).collect();
    assert_eq!(
        lengths,
        [frac(1, 1), frac(3, 8), frac(1, 1), frac(3, 8), frac(7, 16)]
    );
    assert_eq!(order.end().whole_notes(), frac(51, 16));
    assert_eq!(order.played_of(1), [1, 3]);
    assert_eq!(order.played_of(0), [0, 2]);
    assert!(order.played_of(7).is_empty());
}

#[test]
fn position_at_finds_the_played_bar_and_its_offset() {
    let score = score_from(3, "0 [S]; 1 [E2]");
    let order = play_order(&score);
    let at = |numer, denom| order.position_at(ScoreTime::new(frac(numer, denom)).unwrap());
    assert_eq!(
        at(0, 1),
        Some(PlayedPosition {
            played_bar: 0,
            source_bar: 0,
            offset: frac(0, 1)
        })
    );
    assert_eq!(
        at(5, 2),
        Some(PlayedPosition {
            played_bar: 2,
            source_bar: 0,
            offset: frac(1, 2)
        })
    );
    assert_eq!(
        at(3, 1),
        Some(PlayedPosition {
            played_bar: 3,
            source_bar: 1,
            offset: frac(0, 1)
        })
    );
    assert_eq!(
        at(4999, 1000),
        Some(PlayedPosition {
            played_bar: 4,
            source_bar: 2,
            offset: frac(999, 1000)
        })
    );
    assert_eq!(at(5, 1), None);
    assert!(ScoreTime::new(frac(-1, 2)).is_err());
}
