use opentab_model::{Fraction, Score};

use crate::ScoreTime;
use crate::exact::Exact;
use crate::order::PlayOrder;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Click {
    pub at: ScoreTime,
    pub downbeat: bool,
}

pub fn beat_grid(score: &Score, order: &PlayOrder) -> Vec<Click> {
    let mut clicks = Vec::new();
    for played in &order.bars {
        let Some(bar) = score.master_bars.get(played.source) else {
            continue;
        };
        let signature = bar.time_signature;
        let beat = Exact::of(Fraction::new_raw(1, i64::from(signature.denominator())));
        let start = Exact::of(played.start.whole_notes());
        for index in 0..signature.numerator() {
            let at = Exact::integer(i128::from(index))
                .mul(beat)
                .and_then(|offset| start.add(offset))
                .and_then(Exact::fraction)
                .and_then(ScoreTime::new);
            if let Ok(at) = at {
                clicks.push(Click {
                    at,
                    downbeat: index == 0,
                });
            }
        }
    }
    clicks
}
