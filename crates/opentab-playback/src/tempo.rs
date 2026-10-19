use opentab_model::{Fraction, Score, Tempo, TempoChange, TempoUnit};

use crate::exact::Exact;
use crate::order::{PlayOrder, SIXTY_FOURTHS, sixty_fourths};
use crate::time::PERCENT;
use crate::{PlaybackError, ScoreTime, Seconds, Speed};

pub const DEFAULT_QUARTERS_PER_MINUTE: i64 = 120;
pub const MAX_PLAYBACK_DECIMALS: usize = 6;
const DECIMAL_BASE: i128 = 10;
const ROUNDING_DIGIT: u8 = b'5';
const SECONDS_PER_WHOLE_AT_ONE_QPM: i128 = 240;
const MICROSECONDS_PER_MINUTE: i128 = 60_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RampStep(Fraction);

impl RampStep {
    pub const SIXTEENTH: RampStep = RampStep(Fraction::new_raw(1, 16));

    pub fn new(step: Fraction) -> Result<RampStep, PlaybackError> {
        if step <= Fraction::new_raw(0, 1) {
            return Err(PlaybackError::NonPositiveStep);
        }
        Ok(RampStep(step))
    }

    pub fn whole_notes(self) -> Fraction {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RampGrid {
    Hold,
    Step(RampStep),
}

impl RampGrid {
    pub const MUSICAL: RampGrid = RampGrid::Step(RampStep::SIXTEENTH);
    pub const GP: RampGrid = RampGrid::Hold;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum JumpTempo {
    #[default]
    SourcePlace,
    HeldUntilBackward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TempoSegment {
    pub start: ScoreTime,
    pub quarters_per_minute: Fraction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TempoMap {
    segments: Vec<TempoSegment>,
    seconds: Vec<Exact>,
}

pub fn quarters_per_unit(unit: TempoUnit) -> Fraction {
    match unit {
        TempoUnit::Eighth => Fraction::new_raw(1, 2),
        TempoUnit::Quarter => Fraction::new_raw(1, 1),
        TempoUnit::DottedQuarter => Fraction::new_raw(3, 2),
        TempoUnit::Half => Fraction::new_raw(2, 1),
        TempoUnit::DottedHalf => Fraction::new_raw(3, 1),
    }
}

pub fn playback_bpm(tempo: Tempo) -> Option<Fraction> {
    tempo
        .exact()
        .or_else(|| nearest_millionth(&format!("{tempo:?}")))
}

fn nearest_millionth(text: &str) -> Option<Fraction> {
    let digits: String = text
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let (whole, decimals) = digits.split_once('.').unwrap_or((digits.as_str(), ""));
    if whole.is_empty() || decimals.contains('.') {
        return None;
    }
    let kept = decimals.get(..MAX_PLAYBACK_DECIMALS).unwrap_or(decimals);
    let mut numer: i128 = 0;
    for byte in whole.bytes().chain(kept.bytes()) {
        numer = numer
            .checked_mul(DECIMAL_BASE)?
            .checked_add(i128::from(byte - b'0'))?;
    }
    for _ in kept.len()..MAX_PLAYBACK_DECIMALS {
        numer = numer.checked_mul(DECIMAL_BASE)?;
    }
    let round_up = decimals
        .as_bytes()
        .get(MAX_PLAYBACK_DECIMALS)
        .is_some_and(|digit| *digit >= ROUNDING_DIGIT);
    if round_up {
        numer = numer.checked_add(1)?;
    }
    let places = u32::try_from(MAX_PLAYBACK_DECIMALS).ok()?;
    let denom = DECIMAL_BASE.checked_pow(places)?;
    let exact = Exact::new(numer, denom).ok()?.fraction().ok()?;
    (exact > Fraction::new_raw(0, 1)).then_some(exact)
}

pub fn quarters_per_minute(change: &TempoChange) -> Option<Fraction> {
    let bpm = playback_bpm(change.bpm)?;
    Exact::of(bpm)
        .mul(Exact::of(quarters_per_unit(change.unit)))
        .and_then(Exact::fraction)
        .ok()
}

#[derive(Debug, Clone, Copy)]
struct Point {
    at: Exact,
    qpm: Exact,
    linear: bool,
}

struct Profile {
    points: Vec<Point>,
    bar_starts: Vec<Exact>,
    grid: Option<Exact>,
}

struct Piece {
    qpm: Exact,
    until: Option<Exact>,
}

pub(crate) fn quantized(qpm: Exact) -> Result<Exact, PlaybackError> {
    let doubled = qpm
        .numer()
        .checked_mul(2)
        .ok_or(PlaybackError::TimeOverflow)?;
    let scaled = MICROSECONDS_PER_MINUTE
        .checked_mul(2)
        .and_then(|value| value.checked_mul(qpm.denom()))
        .and_then(|value| value.checked_add(qpm.numer()))
        .ok_or(PlaybackError::TimeOverflow)?;
    let micros = scaled
        .checked_div(doubled)
        .ok_or(PlaybackError::TimeOverflow)?
        .max(1);
    Exact::new(MICROSECONDS_PER_MINUTE, micros)
}

impl Profile {
    fn new(score: &Score, grid: RampGrid) -> Result<Profile, PlaybackError> {
        let mut bar_starts = Vec::with_capacity(score.master_bars.len());
        let mut sixty_fourth: i128 = 0;
        let mut points = Vec::new();
        for bar in &score.master_bars {
            let start = Exact::new(sixty_fourth, i128::from(SIXTY_FOURTHS))?;
            bar_starts.push(start);
            let length = bar.time_signature.bar_length();
            for change in &bar.tempo_changes {
                let Some(qpm) = quarters_per_minute(change) else {
                    continue;
                };
                let offset = change.position.min(length);
                points.push(Point {
                    at: start.add(Exact::of(offset))?,
                    qpm: quantized(Exact::of(qpm))?,
                    linear: change.linear,
                });
            }
            sixty_fourth = sixty_fourth
                .checked_add(i128::from(sixty_fourths(bar)))
                .ok_or(PlaybackError::TimeOverflow)?;
        }
        points.sort_by(|left, right| left.at.cmp(&right.at));
        Ok(Profile {
            points,
            bar_starts,
            grid: match grid {
                RampGrid::Hold => None,
                RampGrid::Step(step) => Some(Exact::of(step.whole_notes())),
            },
        })
    }

    fn piece(&self, x: Exact) -> Result<Piece, PlaybackError> {
        let after = self.points.partition_point(|point| point.at <= x);
        let next = self.points.get(after);
        let Some(current) = after
            .checked_sub(1)
            .and_then(|index| self.points.get(index))
        else {
            return Ok(Piece {
                qpm: quantized(Exact::integer(i128::from(DEFAULT_QUARTERS_PER_MINUTE)))?,
                until: next.map(|point| point.at),
            });
        };
        let target = next.map(|first| {
            let last = self.points[after..].partition_point(|point| point.at <= first.at);
            self.points[after + last - 1]
        });
        let (Some(next), Some(grid)) = (target.filter(|_| current.linear), self.grid) else {
            return Ok(Piece {
                qpm: current.qpm,
                until: next.map(|point| point.at),
            });
        };
        let span = next.at.sub(current.at)?;
        let step = x.sub(current.at)?.div(grid)?.floor();
        let reached = Exact::integer(step).mul(grid)?;
        let step_end = current.at.add(reached)?.add(grid)?;
        let until = step_end.min(next.at);
        let qpm = if step == 0 {
            current.qpm
        } else {
            let rise = next.qpm.sub(current.qpm)?.mul(reached)?.div(span)?;
            quantized(current.qpm.add(rise)?)?
        };
        Ok(Piece {
            qpm,
            until: Some(until),
        })
    }
}

impl TempoMap {
    pub fn build(
        score: &Score,
        order: &PlayOrder,
        ramp: RampGrid,
    ) -> Result<TempoMap, PlaybackError> {
        TempoMap::build_with(score, order, ramp, JumpTempo::SourcePlace)
    }

    pub fn build_with(
        score: &Score,
        order: &PlayOrder,
        ramp: RampGrid,
        jumps: JumpTempo,
    ) -> Result<TempoMap, PlaybackError> {
        let profile = Profile::new(score, ramp)?;
        let mut map = TempoMap {
            segments: Vec::new(),
            seconds: Vec::new(),
        };
        if order.bars.is_empty() {
            map.push(
                Exact::ZERO,
                Exact::integer(i128::from(DEFAULT_QUARTERS_PER_MINUTE)),
            )?;
        }
        let mut previous: Option<usize> = None;
        let mut held: Option<Exact> = None;
        for played in &order.bars {
            let source_start = *profile
                .bar_starts
                .get(played.source)
                .ok_or(PlaybackError::OrderMismatch)?;
            let source_end = source_start.add(Exact::of(played.length))?;
            let played_start = Exact::of(played.start.whole_notes());
            let resets = previous.is_none_or(|source| played.source < source);
            previous = Some(played.source);
            let first_point = profile
                .points
                .get(
                    profile
                        .points
                        .partition_point(|point| point.at < source_start),
                )
                .map(|point| point.at);
            let mut x = source_start;
            while x < source_end {
                let piece = profile.piece(x)?;
                let at = played_start.add(x.sub(source_start)?)?;
                let keep = match (jumps, held) {
                    (JumpTempo::HeldUntilBackward, Some(qpm))
                        if !resets && first_point.is_none_or(|point| x < point) =>
                    {
                        qpm
                    }
                    _ => piece.qpm,
                };
                map.push(at, keep)?;
                held = Some(keep);
                x = piece
                    .until
                    .map_or(source_end, |until| until.min(source_end));
            }
        }
        Ok(map)
    }

    fn push(&mut self, at: Exact, qpm: Exact) -> Result<(), PlaybackError> {
        let quarters_per_minute = qpm.fraction()?;
        let seconds = match (self.segments.last(), self.seconds.last()) {
            (Some(last), _) if last.quarters_per_minute == quarters_per_minute => return Ok(()),
            (Some(last), Some(elapsed)) => elapsed.add(span_seconds(
                at.sub(Exact::of(last.start.whole_notes()))?,
                Exact::of(last.quarters_per_minute),
            )?)?,
            _ => Exact::ZERO,
        };
        self.segments.push(TempoSegment {
            start: ScoreTime::new(at.fraction()?)?,
            quarters_per_minute,
        });
        self.seconds.push(seconds);
        Ok(())
    }

    pub fn segments(&self) -> &[TempoSegment] {
        &self.segments
    }

    fn segment_at(&self, t: ScoreTime) -> usize {
        self.segments
            .partition_point(|segment| segment.start <= t)
            .saturating_sub(1)
    }

    pub fn seconds_at(&self, t: ScoreTime, speed: Speed) -> Result<Seconds, PlaybackError> {
        let index = self.segment_at(t);
        let (Some(segment), Some(elapsed)) = (self.segments.get(index), self.seconds.get(index))
        else {
            return Err(PlaybackError::TimeOverflow);
        };
        let offset = Exact::of(t.whole_notes()).sub(Exact::of(segment.start.whole_notes()))?;
        let base = elapsed.add(span_seconds(
            offset,
            Exact::of(segment.quarters_per_minute),
        )?)?;
        Ok(Seconds::of(base.mul(Exact::new(
            i128::from(PERCENT),
            i128::from(speed.percent()),
        )?)?))
    }

    fn whole_notes_at(&self, seconds: Seconds, speed: Speed) -> Result<Exact, PlaybackError> {
        let base = seconds.exact().mul(Exact::new(
            i128::from(speed.percent()),
            i128::from(PERCENT),
        )?)?;
        if base.is_negative() {
            return Err(PlaybackError::NegativeTime);
        }
        let index = self
            .seconds
            .partition_point(|elapsed| *elapsed <= base)
            .saturating_sub(1);
        let (Some(segment), Some(elapsed)) = (self.segments.get(index), self.seconds.get(index))
        else {
            return Err(PlaybackError::TimeOverflow);
        };
        let whole_notes = base
            .sub(*elapsed)?
            .mul(Exact::of(segment.quarters_per_minute))?
            .div(Exact::integer(SECONDS_PER_WHOLE_AT_ONE_QPM))?;
        Exact::of(segment.start.whole_notes()).add(whole_notes)
    }

    pub fn time_at(&self, seconds: Seconds, speed: Speed) -> Result<ScoreTime, PlaybackError> {
        ScoreTime::new(self.whole_notes_at(seconds, speed)?.fraction()?)
    }

    pub fn time_at_floor(
        &self,
        seconds: Seconds,
        speed: Speed,
        step: Fraction,
    ) -> Result<ScoreTime, PlaybackError> {
        let step = Exact::of(RampStep::new(step)?.whole_notes());
        let steps = self.whole_notes_at(seconds, speed)?.div(step)?.floor();
        ScoreTime::new(Exact::integer(steps).mul(step)?.fraction()?)
    }
}

fn span_seconds(whole_notes: Exact, qpm: Exact) -> Result<Exact, PlaybackError> {
    whole_notes
        .mul(Exact::integer(SECONDS_PER_WHOLE_AT_ONE_QPM))?
        .div(qpm)
}
