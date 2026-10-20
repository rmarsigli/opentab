use opentab_playback::{Performance, ScoreTime, Speed, TempoMap};

use crate::{AudioError, SampleRate};

const PERCENT: i128 = 100;

pub(crate) fn unit_at(
    tempo: &TempoMap,
    at: ScoreTime,
    rate: SampleRate,
) -> Result<u64, AudioError> {
    let seconds = tempo.seconds_at(at, Speed::NORMAL)?;
    let units = seconds.floor_scaled(i128::from(rate.hz()) * PERCENT)?;
    u64::try_from(units).map_err(|_| AudioError::FrameOverflow)
}

pub(crate) fn step_of(speed: Speed) -> u64 {
    u64::from(speed.percent())
}

pub(crate) fn frames_until(unit: u64, position: u64, step: u64) -> u64 {
    if unit < position {
        0
    } else {
        (unit - position) / step.max(1)
    }
}

pub(crate) fn is_due(unit: u64, position: u64, step: u64) -> bool {
    unit < position.saturating_add(step)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Timeline {
    pub(crate) events: Vec<u64>,
    pub(crate) clicks: Vec<u64>,
    pub(crate) bars: Vec<u64>,
    pub(crate) bar_clicks: Vec<usize>,
    pub(crate) end: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CachedUnit {
    at: ScoreTime,
    unit: u64,
}

fn units_of<I>(tempo: &TempoMap, times: I, rate: SampleRate) -> Result<Vec<u64>, AudioError>
where
    I: Iterator<Item = ScoreTime>,
{
    let mut units = Vec::new();
    let mut cached: Option<CachedUnit> = None;
    for at in times {
        let unit = match cached {
            Some(hit) if hit.at == at => hit.unit,
            _ => {
                let unit = unit_at(tempo, at, rate)?;
                cached = Some(CachedUnit { at, unit });
                unit
            }
        };
        units.push(unit);
    }
    Ok(units)
}

impl Timeline {
    pub(crate) fn new(performance: &Performance, rate: SampleRate) -> Result<Timeline, AudioError> {
        let tempo = &performance.tempo;
        let events = units_of(tempo, performance.events.iter().map(|event| event.at), rate)?;
        let clicks = units_of(tempo, performance.clicks.iter().map(|click| click.at), rate)?;
        let order_end = performance.order.end();
        let mut bars = units_of(
            tempo,
            performance.order.bars.iter().map(|bar| bar.start),
            rate,
        )?;
        let end_of_bars = unit_at(tempo, order_end, rate)?;
        bars.push(end_of_bars);
        let mut bar_clicks: Vec<usize> = performance
            .order
            .bars
            .iter()
            .map(|bar| {
                performance
                    .clicks
                    .partition_point(|click| click.at < bar.start)
            })
            .collect();
        bar_clicks.push(
            performance
                .clicks
                .partition_point(|click| click.at < order_end),
        );
        let end = events.last().copied().unwrap_or(0).max(end_of_bars);
        Ok(Timeline {
            events,
            clicks,
            bars,
            bar_clicks,
            end,
        })
    }

    pub(crate) fn bar_count(&self) -> usize {
        self.bars.len().saturating_sub(1)
    }
}
