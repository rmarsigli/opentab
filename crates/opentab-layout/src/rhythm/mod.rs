mod assemble;

use std::num::{NonZeroI64, NonZeroI128};

use opentab_model::{Duration, NoteValue, TimeSignature, Tuplet};

use crate::display::{Line, PathSegment, PathStyle, PlacedGlyph, Primitive, Shape};
use crate::glyphs::{
    AUGMENTATION_DOT, Anchor, FLAG_8TH_DOWN, FLAG_8TH_UP, FLAG_16TH_DOWN, FLAG_16TH_UP,
    FLAG_32ND_DOWN, FLAG_32ND_UP, FLAG_64TH_DOWN, FLAG_64TH_UP, FLAG_128TH_DOWN, FLAG_128TH_UP,
    Glyph, Metrics, REST_8TH, REST_16TH, REST_32ND, REST_64TH, REST_128TH, REST_HALF, REST_QUARTER,
    REST_WHOLE, TUPLET_DIGITS,
};
use crate::units::{Point, Rect, STAFF_SPACE, Units, narrow, round_div};

pub const CUE_SIZE: Units = Units(STAFF_SPACE.0 * 7 / 10);
pub const STEM_LENGTH: Units = Units(STAFF_SPACE.0 * 7 / 2);
pub const EXTRA_BEAM_LENGTH: Units = Units(STAFF_SPACE.0 * 3 / 4);
pub const MAX_BEAM_SLOPE: Units = STAFF_SPACE;
pub const BEAMLET: Units = Units(STAFF_SPACE.0 * 9 / 10);
pub const TUPLET_GAP: Units = Units(STAFF_SPACE.0 * 3 / 4);
pub const TUPLET_HOOK: Units = Units(STAFF_SPACE.0 / 2);
pub const TUPLET_NUMBER_GAP: Units = Units(STAFF_SPACE.0 / 4);
pub const DOT_GAP: Units = Units(STAFF_SPACE.0 * 2 / 5);
pub const DOT_STEP: Units = Units(STAFF_SPACE.0 * 3 / 5);
pub const PLAIN_BEAMS: u8 = 2;
pub const WRITTEN_WHOLE: i64 = 1024;

pub use assemble::{
    FittedBeam, FittedStem, FittedTuplet, MAX_BEAM_RANGE, RhythmCanvas, RhythmEvent, RhythmGroups,
    RhythmPlan, SLOPE_SHARE, StemAnchor, TupletGroup, clamp_slope, directions, draw_rhythm,
    fit_stems, group, slash, split_wide,
};

const SPACE_DIVISOR: NonZeroI64 = NonZeroI64::new(STAFF_SPACE.0).unwrap();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StemDirection {
    Up,
    Down,
}

impl StemDirection {
    pub fn sign(self) -> i64 {
        match self {
            StemDirection::Up => -1,
            StemDirection::Down => 1,
        }
    }

    pub fn flipped(self) -> StemDirection {
        match self {
            StemDirection::Up => StemDirection::Down,
            StemDirection::Down => StemDirection::Up,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StemPolicy {
    Fixed(StemDirection),
    AwayFromReference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BeamSlope {
    Follow,
    Flat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RhythmStaff {
    pub reference: Units,
    pub spacing: Units,
    pub policy: StemPolicy,
    pub size: Units,
    pub reach_reference: bool,
    pub slope: BeamSlope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extremes {
    pub top: Units,
    pub bottom: Units,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stem {
    pub x: Units,
    pub base: Units,
    pub tip: Units,
    pub direction: StemDirection,
    pub beams: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeamLine {
    pub direction: StemDirection,
    pub start: Units,
    pub delta: Units,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TupletBracket {
    pub left: Units,
    pub right: Units,
    pub edge: Units,
    pub direction: StemDirection,
    pub number: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    pub onset: i128,
    pub beamable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TupletRun {
    pub first: usize,
    pub last: usize,
    pub number: u8,
}

pub fn flags_of(value: NoteValue) -> u8 {
    match value {
        NoteValue::Whole | NoteValue::Half | NoteValue::Quarter => 0,
        NoteValue::Eighth => 1,
        NoteValue::Sixteenth => 2,
        NoteValue::ThirtySecond => 3,
        NoteValue::SixtyFourth => 4,
        NoteValue::OneTwentyEighth => 5,
    }
}

pub fn stemmed(value: NoteValue) -> bool {
    value != NoteValue::Whole
}

pub fn flag_glyph(beams: u8, direction: StemDirection) -> Option<Glyph> {
    let pair = match beams {
        0 => return None,
        1 => (FLAG_8TH_UP, FLAG_8TH_DOWN),
        2 => (FLAG_16TH_UP, FLAG_16TH_DOWN),
        3 => (FLAG_32ND_UP, FLAG_32ND_DOWN),
        4 => (FLAG_64TH_UP, FLAG_64TH_DOWN),
        _ => (FLAG_128TH_UP, FLAG_128TH_DOWN),
    };
    Some(match direction {
        StemDirection::Up => pair.0,
        StemDirection::Down => pair.1,
    })
}

pub fn rest_glyph(value: NoteValue) -> Glyph {
    match value {
        NoteValue::Whole => REST_WHOLE,
        NoteValue::Half => REST_HALF,
        NoteValue::Quarter => REST_QUARTER,
        NoteValue::Eighth => REST_8TH,
        NoteValue::Sixteenth => REST_16TH,
        NoteValue::ThirtySecond => REST_32ND,
        NoteValue::SixtyFourth => REST_64TH,
        NoteValue::OneTwentyEighth => REST_128TH,
    }
}

pub fn scale(length: Units, size: Units) -> Units {
    length.scaled(size.0, SPACE_DIVISOR)
}

pub fn glyph_box(glyph: Glyph, size: Units) -> Rect {
    let rect = Metrics::bravura().bbox(glyph.name).unwrap_or_default();
    Rect::new(
        Point {
            x: scale(rect.min.x, size),
            y: scale(rect.min.y, size),
        },
        Point {
            x: scale(rect.max.x, size),
            y: scale(rect.max.y, size),
        },
    )
}

pub fn anchor(glyph: Glyph, kind: Anchor, size: Units) -> Point {
    let point = Metrics::bravura()
        .anchor(glyph.name, kind)
        .unwrap_or(Point::ORIGIN);
    Point {
        x: scale(point.x, size),
        y: scale(point.y, size),
    }
}

pub fn glyph(glyph: Glyph, origin: Point, size: Units) -> Primitive {
    Primitive::Glyph(PlacedGlyph {
        origin,
        codepoint: glyph.codepoint,
        staff_space: size,
    })
}

pub fn pattern(time: TimeSignature) -> Vec<i128> {
    let numerator = i128::from(time.numerator());
    let compound = time.denominator() >= 8;
    if numerator == 1 {
        vec![1]
    } else if compound && numerator % 3 == 0 {
        vec![3; usize::from(time.numerator() / 3)]
    } else if compound && numerator % 2 == 1 {
        let mut groups = vec![2; usize::from((time.numerator() - 3) / 2)];
        groups.push(3);
        groups
    } else if compound {
        vec![2; usize::from(time.numerator() / 2)]
    } else {
        vec![1; usize::from(time.numerator())]
    }
}

pub fn group_of(pattern: &[i128], denominator: u8, per_whole: i128, onset: i128) -> Option<i128> {
    let total: i128 = pattern.iter().sum();
    let scaled = onset.checked_mul(i128::from(denominator))?;
    let cycle_length = total.checked_mul(per_whole)?;
    let cycle = scaled.checked_div(cycle_length)?;
    let mut rest = scaled.checked_sub(cycle.checked_mul(cycle_length)?)?;
    let count = i128::try_from(pattern.len()).ok()?;
    let mut index = cycle.checked_mul(count)?;
    for length in pattern {
        let width = length.checked_mul(per_whole)?;
        if rest < width {
            return Some(index);
        }
        rest -= width;
        index += 1;
    }
    Some(index)
}

pub fn beam_groups(time: TimeSignature, per_whole: i128, timings: &[Timing]) -> Vec<Vec<usize>> {
    let pattern = pattern(time);
    let mut groups = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut current_group: Option<i128> = None;
    for (index, timing) in timings.iter().enumerate() {
        let group = if timing.beamable {
            group_of(&pattern, time.denominator(), per_whole, timing.onset)
        } else {
            None
        };
        if group.is_none() || group != current_group {
            if current.len() >= 2 {
                groups.push(std::mem::take(&mut current));
            }
            current.clear();
        }
        current_group = group;
        if group.is_some() {
            current.push(index);
        }
    }
    if current.len() >= 2 {
        groups.push(current);
    }
    groups
}

pub fn written_length(duration: &Duration) -> i64 {
    let base = WRITTEN_WHOLE / duration.value().denominator();
    let dots = u32::from(duration.dots());
    let numerator = (1_i64 << (dots + 1)) - 1;
    base * numerator / (1_i64 << dots)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenRun {
    first: usize,
    tuplet: Tuplet,
    written: i64,
    bases: Vec<i64>,
}

pub fn tuplet_runs(durations: &[Duration]) -> Vec<TupletRun> {
    let mut runs = Vec::new();
    let mut open: Option<OpenRun> = None;
    for (index, duration) in durations.iter().enumerate() {
        let tuplet = duration.tuplet();
        if let Some(run) = &open
            && tuplet != Some(run.tuplet)
        {
            runs.push(TupletRun {
                first: run.first,
                last: index - 1,
                number: run.tuplet.num(),
            });
            open = None;
        }
        let Some(tuplet) = tuplet else {
            continue;
        };
        let mut run = open.take().unwrap_or(OpenRun {
            first: index,
            tuplet,
            written: 0,
            bases: Vec::new(),
        });
        run.written += written_length(duration);
        let base = WRITTEN_WHOLE / duration.value().denominator();
        if !run.bases.contains(&base) {
            run.bases.push(base);
        }
        let number = i64::from(tuplet.num());
        let largest = run.bases.iter().copied().max().unwrap_or(base);
        let complete = run.bases.iter().any(|unit| run.written == number * unit)
            || run.written > number * largest;
        if complete {
            runs.push(TupletRun {
                first: run.first,
                last: index,
                number: tuplet.num(),
            });
        } else {
            open = Some(run);
        }
    }
    if let Some(run) = open {
        runs.push(TupletRun {
            first: run.first,
            last: durations.len() - 1,
            number: run.tuplet.num(),
        });
    }
    runs
}

fn interpolate(from: Units, delta: Units, offset: Units, span: Units) -> Units {
    match NonZeroI128::new(i128::from(span.0)) {
        Some(span) => {
            from + Units(narrow(round_div(
                i128::from(delta.0) * i128::from(offset.0),
                span,
            )))
        }
        None => from,
    }
}

impl BeamLine {
    pub fn at(&self, x: Units, first: Units, last: Units) -> Units {
        interpolate(self.start, self.delta, x - first, last - first)
    }

    pub fn outer(&self) -> Units {
        match self.direction {
            StemDirection::Up => self.start.min(self.start + self.delta),
            StemDirection::Down => self.start.max(self.start + self.delta),
        }
    }
}

impl RhythmStaff {
    pub fn length(&self, length: Units) -> Units {
        scale(length, self.size)
    }

    pub fn stem_thickness(&self) -> Units {
        self.length(Metrics::bravura().engraving_defaults().stem_thickness)
    }

    pub fn beam_thickness(&self) -> Units {
        self.length(Metrics::bravura().engraving_defaults().beam_thickness)
    }

    pub fn beam_step(&self) -> Units {
        self.beam_thickness() + self.length(Metrics::bravura().engraving_defaults().beam_spacing)
    }

    pub fn direction<I: IntoIterator<Item = Extremes>>(&self, extremes: I) -> StemDirection {
        match self.policy {
            StemPolicy::Fixed(direction) => direction,
            StemPolicy::AwayFromReference => {
                let mut above = Units::ZERO;
                let mut below = Units::ZERO;
                for extreme in extremes {
                    above = above.max(self.reference - extreme.top);
                    below = below.max(extreme.bottom - self.reference);
                }
                if below > above {
                    StemDirection::Up
                } else {
                    StemDirection::Down
                }
            }
        }
    }

    pub fn stem_length(&self, beams: u8) -> Units {
        let extra = i64::from(beams.saturating_sub(PLAIN_BEAMS));
        self.length(STEM_LENGTH) + Units(self.length(EXTRA_BEAM_LENGTH).0.saturating_mul(extra))
    }

    pub fn stem_end(&self, stem: &Stem) -> Units {
        let length = self.stem_length(stem.beams);
        match stem.direction {
            StemDirection::Up => {
                let end = stem.tip - length;
                if self.reach_reference {
                    end.min(self.reference)
                } else {
                    end
                }
            }
            StemDirection::Down => {
                let end = stem.tip + length;
                if self.reach_reference {
                    end.max(self.reference)
                } else {
                    end
                }
            }
        }
    }

    pub fn beam_line(&self, stems: &[Stem]) -> Option<BeamLine> {
        let first = stems.first()?;
        let direction = first.direction;
        let ends: Vec<Units> = stems
            .iter()
            .map(|stem| self.stem_end(&Stem { direction, ..*stem }))
            .collect();
        let start = *ends.first()?;
        let end = *ends.last()?;
        let inner = &ends[1..ends.len().saturating_sub(1).max(1)];
        let beyond = |value: &Units| match direction {
            StemDirection::Up => *value < start.min(end),
            StemDirection::Down => *value > start.max(end),
        };
        let limit = self.length(MAX_BEAM_SLOPE);
        let delta = if self.slope == BeamSlope::Flat || inner.iter().any(beyond) {
            Units::ZERO
        } else {
            (end - start).clamp(-limit, limit)
        };
        let start = match direction {
            StemDirection::Up => {
                let need = ends.iter().copied().min().unwrap_or(start);
                need - delta.max(Units::ZERO)
            }
            StemDirection::Down => {
                let need = ends.iter().copied().max().unwrap_or(start);
                need - delta.min(Units::ZERO)
            }
        };
        Some(BeamLine {
            direction,
            start,
            delta,
        })
    }

    pub fn beam_extent(&self, line: &BeamLine, levels: u8) -> Extremes {
        let depth = Units(
            self.beam_step()
                .0
                .saturating_mul(i64::from(levels.saturating_sub(1))),
        ) + self.beam_thickness();
        let low = line.start.min(line.start + line.delta);
        let high = line.start.max(line.start + line.delta);
        match line.direction {
            StemDirection::Up => Extremes {
                top: low,
                bottom: high + depth,
            },
            StemDirection::Down => Extremes {
                top: low - depth,
                bottom: high,
            },
        }
    }

    pub fn draw_stem(&self, stem: &Stem, end: Units, items: &mut Vec<Primitive>) {
        items.push(Primitive::Line(Line {
            from: Point {
                x: stem.x,
                y: stem.base,
            },
            to: Point { x: stem.x, y: end },
            thickness: self.stem_thickness(),
            dash: None,
        }));
    }

    pub fn flag_origin(&self, stem: &Stem, end: Units) -> Option<(Glyph, Point)> {
        let glyph = flag_glyph(stem.beams, stem.direction)?;
        let kind = match stem.direction {
            StemDirection::Up => Anchor::StemUpNW,
            StemDirection::Down => Anchor::StemDownSW,
        };
        let attach = anchor(glyph, kind, self.size);
        let origin = Point {
            x: stem.x - self.stem_thickness().half() - attach.x,
            y: end - attach.y,
        };
        Some((glyph, origin))
    }

    pub fn flag_extent(&self, stem: &Stem, end: Units) -> Option<Rect> {
        let (glyph, origin) = self.flag_origin(stem, end)?;
        let rect = glyph_box(glyph, self.size);
        Some(Rect::new(origin.offset(rect.min), origin.offset(rect.max)))
    }

    pub fn draw_flag(&self, stem: &Stem, end: Units, items: &mut Vec<Primitive>) -> Option<Point> {
        let (flag, origin) = self.flag_origin(stem, end)?;
        items.push(glyph(flag, origin, self.size));
        Some(origin)
    }

    pub fn draw_beams(&self, stems: &[Stem], line: &BeamLine, items: &mut Vec<Primitive>) {
        let (Some(first), Some(last)) = (stems.first(), stems.last()) else {
            return;
        };
        let half = self.stem_thickness().half();
        let inward = -line.direction.sign();
        let thickness = Units(self.beam_thickness().0.saturating_mul(inward));
        let step = self.beam_step();
        let levels = stems.iter().map(|stem| stem.beams).max().unwrap_or(0);
        let y_at = |x: Units, level: u8| {
            line.at(x, first.x, last.x) + Units(step.0.saturating_mul(inward * i64::from(level)))
        };
        let mut slab = |from: Units, to: Units, level: u8| {
            let top_from = y_at(from, level);
            let top_to = y_at(to, level);
            items.push(Primitive::Shape(Shape {
                segments: vec![
                    PathSegment::MoveTo(Point {
                        x: from,
                        y: top_from,
                    }),
                    PathSegment::LineTo(Point { x: to, y: top_to }),
                    PathSegment::LineTo(Point {
                        x: to,
                        y: top_to + thickness,
                    }),
                    PathSegment::LineTo(Point {
                        x: from,
                        y: top_from + thickness,
                    }),
                    PathSegment::Close,
                ],
                style: PathStyle::Fill,
            }));
        };
        for level in 0..levels {
            let mut index = 0;
            while index < stems.len() {
                if stems[index].beams <= level {
                    index += 1;
                    continue;
                }
                let start = index;
                while index + 1 < stems.len() && stems[index + 1].beams > level {
                    index += 1;
                }
                let end = index;
                index += 1;
                if end > start {
                    slab(stems[start].x - half, stems[end].x + half, level);
                    continue;
                }
                let stem = &stems[start];
                let rightward = start == 0;
                let neighbour = if rightward {
                    stems.get(start + 1)
                } else {
                    start.checked_sub(1).and_then(|before| stems.get(before))
                };
                let room = neighbour.map_or(self.length(BEAMLET), |other| {
                    Units((other.x - stem.x).0.abs() / 2)
                });
                let length = self.length(BEAMLET).min(room);
                if rightward {
                    slab(stem.x - half, stem.x + length, level);
                } else {
                    slab(stem.x - length, stem.x + half, level);
                }
            }
        }
    }

    pub fn number_half_height(&self) -> Units {
        glyph_box(TUPLET_DIGITS[0], self.size).height().half()
    }

    pub fn tuplet_edge(&self, direction: StemDirection, ink: Extremes) -> Units {
        let offset = self.length(TUPLET_GAP) + self.number_half_height();
        match direction {
            StemDirection::Up => ink.top - offset,
            StemDirection::Down => ink.bottom + offset,
        }
    }

    pub fn tuplet_extent(&self, bracket: &TupletBracket) -> Extremes {
        let half = self.number_half_height();
        let hook = self.length(TUPLET_HOOK);
        Extremes {
            top: bracket.edge - half.max(hook),
            bottom: bracket.edge + half.max(hook),
        }
    }

    pub fn draw_tuplet(&self, bracket: &TupletBracket, items: &mut Vec<Primitive>) {
        let digits: Vec<Glyph> = bracket
            .number
            .to_string()
            .bytes()
            .filter_map(|byte| TUPLET_DIGITS.get(usize::from(byte.wrapping_sub(b'0'))))
            .copied()
            .collect();
        let advance = |digit: &Glyph| glyph_box(*digit, self.size).max.x;
        let width = digits
            .iter()
            .fold(Units::ZERO, |total, digit| total + advance(digit));
        let middle = bracket.left + (bracket.right - bracket.left).half();
        let half_height = self.number_half_height();
        let mut pen = middle - width.half();
        for digit in &digits {
            items.push(glyph(
                *digit,
                Point {
                    x: pen,
                    y: bracket.edge + half_height,
                },
                self.size,
            ));
            pen += advance(digit);
        }
        let gap = self.length(TUPLET_NUMBER_GAP);
        let thickness = self.length(
            Metrics::bravura()
                .engraving_defaults()
                .tuplet_bracket_thickness,
        );
        let hook = Units(
            self.length(TUPLET_HOOK)
                .0
                .saturating_mul(-bracket.direction.sign()),
        );
        let line = |from: Point, to: Point| {
            Primitive::Line(Line {
                from,
                to,
                thickness,
                dash: None,
            })
        };
        let left_end = middle - width.half() - gap;
        let right_start = middle + width.half() + gap;
        let edge = bracket.edge;
        if left_end > bracket.left {
            items.push(line(
                Point {
                    x: bracket.left,
                    y: edge + hook,
                },
                Point {
                    x: bracket.left,
                    y: edge,
                },
            ));
            items.push(line(
                Point {
                    x: bracket.left,
                    y: edge,
                },
                Point {
                    x: left_end,
                    y: edge,
                },
            ));
        }
        if bracket.right > right_start {
            items.push(line(
                Point {
                    x: right_start,
                    y: edge,
                },
                Point {
                    x: bracket.right,
                    y: edge,
                },
            ));
            items.push(line(
                Point {
                    x: bracket.right,
                    y: edge,
                },
                Point {
                    x: bracket.right,
                    y: edge + hook,
                },
            ));
        }
    }

    pub fn rest_y(&self, value: NoteValue, shift: Units) -> Units {
        match value {
            NoteValue::Whole => self.reference - self.spacing + shift,
            _ => self.reference + shift,
        }
    }

    pub fn on_line(&self, y: Units) -> bool {
        self.spacing.0 != 0 && (y - self.reference).0.rem_euclid(self.spacing.0) == 0
    }

    pub fn dot_y(&self, y: Units, downward: bool) -> Units {
        if !self.on_line(y) {
            return y;
        }
        let half = self.spacing.half();
        if downward { y + half } else { y - half }
    }

    pub fn dots_width(&self, dots: u8) -> Units {
        if dots == 0 {
            return Units::ZERO;
        }
        let steps = i64::from(dots - 1);
        self.length(DOT_GAP)
            + Units(self.length(DOT_STEP).0.saturating_mul(steps))
            + glyph_box(AUGMENTATION_DOT, self.size).max.x
    }

    pub fn draw_dots(&self, x: Units, ys: &[Units], dots: u8, items: &mut Vec<Primitive>) {
        for y in ys {
            for index in 0..dots {
                let pen = x
                    + self.length(DOT_GAP)
                    + Units(self.length(DOT_STEP).0.saturating_mul(i64::from(index)));
                items.push(glyph(AUGMENTATION_DOT, Point { x: pen, y: *y }, self.size));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn time(numerator: u8, denominator: u8) -> TimeSignature {
        TimeSignature::new(numerator, denominator).unwrap()
    }

    fn duration(value: NoteValue, dots: u8, tuplet: Option<(u8, u8)>) -> Duration {
        let tuplet = tuplet.map(|(num, den)| Tuplet::new(num, den).unwrap());
        Duration::new(value, dots, tuplet).unwrap()
    }

    fn timings(per_whole: i128, lengths: &[(i128, bool)]) -> Vec<Timing> {
        let mut onset = 0;
        lengths
            .iter()
            .map(|(length, beamable)| {
                let timing = Timing {
                    onset,
                    beamable: *beamable,
                };
                onset += length * per_whole / 16;
                timing
            })
            .collect()
    }

    #[test]
    fn simple_meters_group_by_beat_and_compound_meters_by_dotted_beat() {
        assert_eq!(pattern(time(4, 4)), vec![1, 1, 1, 1]);
        assert_eq!(pattern(time(2, 2)), vec![1, 1]);
        assert_eq!(pattern(time(6, 8)), vec![3, 3]);
        assert_eq!(pattern(time(9, 8)), vec![3, 3, 3]);
        assert_eq!(pattern(time(12, 8)), vec![3, 3, 3, 3]);
        assert_eq!(pattern(time(3, 8)), vec![3]);
        assert_eq!(pattern(time(7, 8)), vec![2, 2, 3]);
        assert_eq!(pattern(time(5, 8)), vec![2, 3]);
        assert_eq!(pattern(time(4, 8)), vec![2, 2]);
        assert_eq!(pattern(time(1, 4)), vec![1]);
    }

    #[test]
    fn beams_never_cross_a_beat_group_or_a_rest() {
        let eighths = timings(16, &[(2, true); 8]);
        assert_eq!(
            beam_groups(time(4, 4), 16, &eighths),
            vec![vec![0, 1], vec![2, 3], vec![4, 5], vec![6, 7]]
        );
        let compound = timings(16, &[(2, true); 6]);
        assert_eq!(
            beam_groups(time(6, 8), 16, &compound),
            vec![vec![0, 1, 2], vec![3, 4, 5]]
        );
        let seven = timings(16, &[(2, true); 7]);
        assert_eq!(
            beam_groups(time(7, 8), 16, &seven),
            vec![vec![0, 1], vec![2, 3], vec![4, 5, 6]]
        );
        let broken = timings(
            16,
            &[(2, true), (2, false), (2, true), (1, true), (1, true)],
        );
        assert_eq!(beam_groups(time(2, 4), 16, &broken), vec![vec![2, 3, 4]]);
        let mixed = timings(
            16,
            &[(3, true), (1, true), (4, false), (2, true), (2, true)],
        );
        assert_eq!(
            beam_groups(time(3, 4), 16, &mixed),
            vec![vec![0, 1], vec![3, 4]]
        );
    }

    #[test]
    fn an_overfull_bar_keeps_grouping_by_its_pattern() {
        assert_eq!(group_of(&[2, 2, 3], 8, 8, 7), Some(3));
        assert_eq!(group_of(&[2, 2, 3], 8, 8, 6), Some(2));
        assert_eq!(group_of(&[2, 2, 3], 8, 8, 4), Some(2));
        assert_eq!(group_of(&[2, 2, 3], 8, 8, 3), Some(1));
        assert_eq!(group_of(&[1], 4, i128::MAX, i128::MAX), None);
    }

    #[test]
    fn tuplets_close_when_their_written_notes_fill_the_ratio() {
        let triplet = duration(NoteValue::Eighth, 0, Some((3, 2)));
        let runs = tuplet_runs(&[triplet; 6]);
        assert_eq!(
            runs,
            vec![
                TupletRun {
                    first: 0,
                    last: 2,
                    number: 3
                },
                TupletRun {
                    first: 3,
                    last: 5,
                    number: 3
                }
            ]
        );
        let quintuplet = duration(NoteValue::Sixteenth, 0, Some((5, 4)));
        let plain = duration(NoteValue::Quarter, 0, None);
        let runs = tuplet_runs(&[
            plain, quintuplet, quintuplet, quintuplet, quintuplet, quintuplet, plain,
        ]);
        assert_eq!(
            runs,
            vec![TupletRun {
                first: 1,
                last: 5,
                number: 5
            }]
        );
        let mixed = [
            duration(NoteValue::Quarter, 0, Some((3, 2))),
            duration(NoteValue::Eighth, 0, Some((3, 2))),
        ];
        assert_eq!(
            tuplet_runs(&mixed),
            vec![TupletRun {
                first: 0,
                last: 1,
                number: 3
            }]
        );
        let dotted = [
            duration(NoteValue::Eighth, 1, Some((3, 2))),
            duration(NoteValue::Sixteenth, 0, Some((3, 2))),
            duration(NoteValue::Eighth, 0, Some((3, 2))),
        ];
        assert_eq!(tuplet_runs(&dotted).len(), 1);
        let cut = [triplet, triplet, plain];
        assert_eq!(
            tuplet_runs(&cut),
            vec![TupletRun {
                first: 0,
                last: 1,
                number: 3
            }]
        );
        let changed = [triplet, quintuplet];
        assert_eq!(tuplet_runs(&changed).len(), 2);
    }

    fn standard() -> RhythmStaff {
        RhythmStaff {
            reference: Units(2048),
            spacing: STAFF_SPACE,
            policy: StemPolicy::AwayFromReference,
            size: STAFF_SPACE,
            reach_reference: true,
            slope: BeamSlope::Follow,
        }
    }

    #[test]
    fn the_farthest_note_from_the_reference_decides_and_a_tie_points_down() {
        let staff = standard();
        let at = |top: i64, bottom: i64| Extremes {
            top: Units(top),
            bottom: Units(bottom),
        };
        assert_eq!(staff.direction([at(3072, 3072)]), StemDirection::Up);
        assert_eq!(staff.direction([at(1024, 1024)]), StemDirection::Down);
        assert_eq!(staff.direction([at(2048, 2048)]), StemDirection::Down);
        assert_eq!(staff.direction([at(1024, 3072)]), StemDirection::Down);
        assert_eq!(staff.direction([at(512, 4096)]), StemDirection::Up);
        let fixed = RhythmStaff {
            policy: StemPolicy::Fixed(StemDirection::Up),
            ..staff
        };
        assert_eq!(fixed.direction([at(0, 0)]), StemDirection::Up);
    }

    #[test]
    fn stems_are_three_and_a_half_spaces_and_reach_the_middle_line() {
        let staff = standard();
        let stem = |tip: i64, direction, beams| Stem {
            x: Units::ZERO,
            base: Units(tip),
            tip: Units(tip),
            direction,
            beams,
        };
        assert_eq!(
            staff.stem_end(&stem(3072, StemDirection::Up, 0)),
            Units(3072 - 3584)
        );
        assert_eq!(
            staff.stem_end(&stem(1024, StemDirection::Down, 1)),
            Units(1024 + 3584)
        );
        assert_eq!(
            staff.stem_end(&stem(9216, StemDirection::Up, 0)),
            Units(2048)
        );
        assert_eq!(
            staff.stem_end(&stem(-6144, StemDirection::Down, 0)),
            Units(2048)
        );
        assert_eq!(
            staff.stem_end(&stem(3072, StemDirection::Up, 4)),
            Units(3072 - 3584 - 2 * 768)
        );
    }

    #[test]
    fn beam_slope_is_limited_to_one_space_and_stems_never_get_shorter() {
        let staff = standard();
        let stem = |x: i64, tip: i64| Stem {
            x: Units(x),
            base: Units(tip),
            tip: Units(tip),
            direction: StemDirection::Up,
            beams: 1,
        };
        let steep = [stem(0, 4096), stem(4000, 0)];
        let line = staff.beam_line(&steep).unwrap();
        assert_eq!(line.delta, Units(-1024));
        assert!(line.start <= Units(4096 - 3584) && line.start + line.delta <= Units(-3584));
        let concave = [stem(0, 2048), stem(2000, 0), stem(4000, 2048)];
        let flat = staff.beam_line(&concave).unwrap();
        assert_eq!(flat.delta, Units::ZERO);
        assert_eq!(flat.start, Units(-3584));
        for (index, item) in concave.iter().enumerate() {
            let at = flat.at(item.x, Units(0), Units(4000));
            assert!(at <= staff.stem_end(&concave[index]), "{index}");
        }
    }
}
