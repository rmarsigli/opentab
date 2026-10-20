use opentab_model::{Fraction, MasterBar, Score, source_order};

use crate::ScoreTime;
use crate::exact::Exact;

pub use opentab_model::MAX_PLAYED_BARS;
pub(crate) const SIXTY_FOURTHS: i64 = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayedBar {
    pub source: usize,
    pub start: ScoreTime,
    pub length: Fraction,
    pub pass: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayedPosition {
    pub played_bar: usize,
    pub source_bar: usize,
    pub offset: Fraction,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayOrder {
    pub bars: Vec<PlayedBar>,
    pub truncated: bool,
}

impl PlayOrder {
    pub fn played_of(&self, source: usize) -> Vec<usize> {
        self.bars
            .iter()
            .enumerate()
            .filter(|(_, bar)| bar.source == source)
            .map(|(index, _)| index)
            .collect()
    }

    pub fn sources(&self) -> Vec<usize> {
        self.bars.iter().map(|bar| bar.source).collect()
    }

    pub fn end(&self) -> ScoreTime {
        self.bars
            .last()
            .and_then(|bar| {
                Exact::of(bar.start.whole_notes())
                    .add(Exact::of(bar.length))
                    .and_then(Exact::fraction)
                    .ok()
            })
            .and_then(|end| ScoreTime::new(end).ok())
            .unwrap_or(ScoreTime::ZERO)
    }

    pub fn position_at(&self, t: ScoreTime) -> Option<PlayedPosition> {
        let after = self.bars.partition_point(|bar| bar.start <= t);
        let played_bar = after.checked_sub(1)?;
        let bar = self.bars.get(played_bar)?;
        let offset = Exact::of(t.whole_notes())
            .sub(Exact::of(bar.start.whole_notes()))
            .and_then(Exact::fraction)
            .ok()?;
        (offset < bar.length).then_some(PlayedPosition {
            played_bar,
            source_bar: bar.source,
            offset,
        })
    }
}

pub(crate) fn sixty_fourths(bar: &MasterBar) -> i64 {
    let signature = bar.time_signature;
    i64::from(signature.numerator()) * (SIXTY_FOURTHS / i64::from(signature.denominator()))
}

pub fn play_order(score: &Score) -> PlayOrder {
    let order = source_order(&score.master_bars);
    let mut bars = Vec::with_capacity(order.bars.len());
    let mut start: i64 = 0;
    for played in &order.bars {
        let Some(bar) = score.master_bars.get(played.source) else {
            continue;
        };
        bars.push(PlayedBar {
            source: played.source,
            start: ScoreTime::new(Fraction::new(start, SIXTY_FOURTHS)).unwrap_or(ScoreTime::ZERO),
            length: bar.time_signature.bar_length(),
            pass: played.pass,
        });
        start = start.saturating_add(sixty_fourths(bar));
    }
    PlayOrder {
        bars,
        truncated: order.truncated,
    }
}
