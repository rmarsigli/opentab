use opentab_playback::{Click, ScoreTime, beat_grid, play_order};

use crate::support::{frac, master_bar, plain_score};

fn click(numer: i64, denom: i64, downbeat: bool) -> Click {
    Click {
        at: ScoreTime::new(frac(numer, denom)).expect("time"),
        downbeat,
    }
}

#[test]
fn every_played_bar_clicks_its_beats_with_an_accented_downbeat() {
    let mut score = plain_score(2);
    score.master_bars[0] = {
        let mut bar = master_bar(0, 3, 4);
        bar.repeat_start = true;
        bar.repeat_end = Some(2);
        bar
    };
    score.master_bars[1] = master_bar(1, 6, 8);
    let order = play_order(&score);
    let clicks = beat_grid(&score, &order);
    assert_eq!(
        clicks,
        [
            click(0, 4, true),
            click(1, 4, false),
            click(2, 4, false),
            click(3, 4, true),
            click(4, 4, false),
            click(5, 4, false),
            click(6, 4, true),
            click(13, 8, false),
            click(14, 8, false),
            click(15, 8, false),
            click(16, 8, false),
            click(17, 8, false)
        ]
    );
}
