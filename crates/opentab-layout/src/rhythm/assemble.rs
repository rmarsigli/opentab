use std::num::NonZeroI64;

use opentab_model::{Duration, TimeSignature};

use super::{
    BeamLine, CUE_SIZE, Extremes, RhythmStaff, Stem, StemDirection, StemPolicy, Timing,
    TupletBracket, anchor, beam_groups, flags_of, stemmed, tuplet_runs,
};
use crate::display::{Line, Primitive};
use crate::glyphs::{Anchor, FLAG_8TH_UP};
use crate::units::{Point, Rect, STAFF_SPACE, Units};

pub const SLOPE_SHARE: i64 = 4;
pub const MAX_BEAM_RANGE: Units = Units(STAFF_SPACE.0 * 7);
const SLOPE_DIVISOR: NonZeroI64 = NonZeroI64::new(SLOPE_SHARE).unwrap();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RhythmEvent {
    pub voice: usize,
    pub onset: i128,
    pub duration: Duration,
    pub grace: bool,
    pub sounding: bool,
    pub slash: bool,
    pub extremes: Option<Extremes>,
    pub left: Units,
    pub right: Units,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StemAnchor {
    pub dx: Units,
    pub base: Units,
    pub tip: Units,
    pub direction: StemDirection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TupletGroup {
    pub members: Vec<usize>,
    pub number: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RhythmGroups {
    pub beams: Vec<Vec<usize>>,
    pub tuplets: Vec<TupletGroup>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FittedStem {
    pub anchor: StemAnchor,
    pub beams: u8,
    pub end: Units,
    pub beam: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FittedBeam {
    pub members: Vec<usize>,
    pub line: BeamLine,
    pub levels: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FittedTuplet {
    pub first: usize,
    pub last: usize,
    pub number: u8,
    pub direction: StemDirection,
    pub edge: Units,
    pub bracket: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RhythmPlan {
    pub stems: Vec<Option<FittedStem>>,
    pub beams: Vec<FittedBeam>,
    pub tuplets: Vec<FittedTuplet>,
}

fn widen(extremes: Option<Extremes>, top: Units, bottom: Units) -> Extremes {
    match extremes {
        Some(known) => Extremes {
            top: known.top.min(top),
            bottom: known.bottom.max(bottom),
        },
        None => Extremes { top, bottom },
    }
}

impl RhythmStaff {
    pub fn cue(&self) -> RhythmStaff {
        RhythmStaff {
            policy: StemPolicy::Fixed(StemDirection::Up),
            size: CUE_SIZE,
            reach_reference: false,
            ..*self
        }
    }

    pub fn for_event(&self, event: &RhythmEvent) -> RhythmStaff {
        if event.grace { self.cue() } else { *self }
    }
}

pub fn group(time: TimeSignature, per_whole: i128, events: &[RhythmEvent]) -> RhythmGroups {
    let mut voices: Vec<usize> = events.iter().map(|event| event.voice).collect();
    voices.sort_unstable();
    voices.dedup();
    let mut groups = RhythmGroups::default();
    for voice in voices {
        let members: Vec<usize> = events
            .iter()
            .enumerate()
            .filter(|(_, event)| event.voice == voice && !event.grace)
            .map(|(index, _)| index)
            .collect();
        let timings: Vec<Timing> = members
            .iter()
            .map(|index| {
                let event = &events[*index];
                Timing {
                    onset: event.onset,
                    beamable: event.sounding && flags_of(event.duration.value()) > 0,
                }
            })
            .collect();
        for group in beam_groups(time, per_whole, &timings) {
            let group: Vec<usize> = group.into_iter().map(|offset| members[offset]).collect();
            groups.beams.extend(split_wide(group, events));
        }
        let durations: Vec<Duration> = members
            .iter()
            .map(|index| events[*index].duration)
            .collect();
        groups
            .tuplets
            .extend(tuplet_runs(&durations).into_iter().map(|run| TupletGroup {
                members: members[run.first..=run.last].to_vec(),
                number: run.number,
            }));
    }
    groups
}

fn range_of(members: &[usize], events: &[RhythmEvent]) -> Option<Units> {
    let found: Vec<Extremes> = members
        .iter()
        .filter_map(|member| events.get(*member)?.extremes)
        .collect();
    let top = found.iter().map(|extremes| extremes.top).min()?;
    let bottom = found.iter().map(|extremes| extremes.bottom).max()?;
    Some(bottom - top)
}

fn middle_of(event: Option<&RhythmEvent>) -> Option<Units> {
    event?
        .extremes
        .map(|extremes| extremes.top + (extremes.bottom - extremes.top).half())
}

pub fn split_wide(group: Vec<usize>, events: &[RhythmEvent]) -> Vec<Vec<usize>> {
    if group.len() < 2 || range_of(&group, events).is_none_or(|range| range <= MAX_BEAM_RANGE) {
        return vec![group];
    }
    let leap = |at: usize| match (
        middle_of(events.get(group[at - 1])),
        middle_of(events.get(group[at])),
    ) {
        (Some(before), Some(after)) => (after - before).0.abs(),
        _ => 0,
    };
    let cut = (1..group.len())
        .rev()
        .max_by_key(|at| leap(*at))
        .unwrap_or(1);
    let (left, right) = group.split_at(cut);
    let mut out = split_wide(left.to_vec(), events);
    out.extend(split_wide(right.to_vec(), events));
    out.retain(|part| part.len() >= 2);
    out
}

pub fn directions(
    staff: &RhythmStaff,
    events: &[RhythmEvent],
    groups: &RhythmGroups,
) -> Vec<StemDirection> {
    events
        .iter()
        .enumerate()
        .map(|(index, event)| {
            if event.grace {
                return staff.cue().direction(event.extremes);
            }
            match groups.beams.iter().find(|group| group.contains(&index)) {
                Some(group) => {
                    staff.direction(group.iter().filter_map(|member| events[*member].extremes))
                }
                None => staff.direction(event.extremes),
            }
        })
        .collect()
}

pub fn fit_stems(
    staff: &RhythmStaff,
    events: &[RhythmEvent],
    anchors: &[Option<StemAnchor>],
    groups: &RhythmGroups,
) -> RhythmPlan {
    let stem = |anchor: &StemAnchor, beams: u8| Stem {
        x: anchor.dx,
        base: anchor.base,
        tip: anchor.tip,
        direction: anchor.direction,
        beams,
    };
    let mut stems: Vec<Option<FittedStem>> = events
        .iter()
        .zip(anchors)
        .map(|(event, anchor)| {
            let anchor = (*anchor)?;
            let value = event.duration.value();
            if !stemmed(value) {
                return None;
            }
            let beams = flags_of(value);
            Some(FittedStem {
                anchor,
                beams,
                end: staff.for_event(event).stem_end(&stem(&anchor, beams)),
                beam: None,
            })
        })
        .collect();
    let mut beams = Vec::new();
    for members in &groups.beams {
        let fitted: Vec<Stem> = members
            .iter()
            .filter_map(|index| stems.get(*index).copied().flatten())
            .map(|fitted| stem(&fitted.anchor, fitted.beams))
            .collect();
        if fitted.len() != members.len() || fitted.len() < 2 {
            continue;
        }
        let Some(line) = staff.beam_line(&fitted) else {
            continue;
        };
        let index = beams.len();
        for member in members {
            if let Some(Some(fitted)) = stems.get_mut(*member) {
                fitted.beam = Some(index);
                fitted.anchor.direction = line.direction;
                fitted.end = line.outer();
            }
        }
        beams.push(FittedBeam {
            members: members.clone(),
            line,
            levels: fitted.iter().map(|item| item.beams).max().unwrap_or(1),
        });
    }
    RhythmPlan {
        stems,
        beams,
        tuplets: Vec::new(),
    }
}

impl RhythmPlan {
    pub fn stem(&self, index: usize) -> Option<&FittedStem> {
        self.stems.get(index)?.as_ref()
    }

    pub fn stem_rect(
        &self,
        staff: &RhythmStaff,
        event: &RhythmEvent,
        index: usize,
    ) -> Option<Rect> {
        let fitted = self.stem(index)?;
        let geometry = staff.for_event(event);
        let half = geometry.stem_thickness().half();
        let mut rect = Rect::new(
            Point {
                x: fitted.anchor.dx - half,
                y: fitted.anchor.base.min(fitted.end),
            },
            Point {
                x: fitted.anchor.dx + half,
                y: fitted.anchor.base.max(fitted.end),
            },
        );
        if fitted.beam.is_none()
            && let Some(flag) = geometry.flag_extent(&self.relative(index)?, fitted.end)
        {
            rect = rect.including(flag.min).including(flag.max);
        }
        if event.slash {
            let (from, to) = slash(fitted, &geometry);
            rect = rect.including(from).including(to);
        }
        Some(rect)
    }

    fn relative(&self, index: usize) -> Option<Stem> {
        let fitted = self.stem(index)?;
        Some(Stem {
            x: fitted.anchor.dx,
            base: fitted.anchor.base,
            tip: fitted.anchor.tip,
            direction: fitted.anchor.direction,
            beams: fitted.beams,
        })
    }

    pub fn beam_extent(&self, staff: &RhythmStaff, beam: &FittedBeam) -> Extremes {
        staff.beam_extent(&beam.line, beam.levels)
    }

    pub fn place_tuplets(
        &mut self,
        staff: &RhythmStaff,
        groups: &RhythmGroups,
        ink: &[Option<Extremes>],
    ) {
        let mut tuplets = Vec::new();
        for group in &groups.tuplets {
            let (Some(first), Some(last)) = (group.members.first(), group.members.last()) else {
                continue;
            };
            let direction = group
                .members
                .iter()
                .find_map(|index| self.stem(*index).map(|fitted| fitted.anchor.direction))
                .unwrap_or(StemDirection::Up);
            let mut reach: Option<Extremes> = None;
            for index in &group.members {
                if let Some(Some(known)) = ink.get(*index) {
                    reach = Some(widen(reach, known.top, known.bottom));
                }
            }
            for beam in &self.beams {
                if beam
                    .members
                    .iter()
                    .any(|member| group.members.contains(member))
                {
                    let extent = self.beam_extent(staff, beam);
                    reach = Some(widen(reach, extent.top, extent.bottom));
                }
            }
            let reach = reach.unwrap_or(Extremes {
                top: staff.reference,
                bottom: staff.reference,
            });
            let beamed = self.beams.iter().any(|beam| {
                group
                    .members
                    .iter()
                    .all(|member| beam.members.contains(member))
            });
            tuplets.push(FittedTuplet {
                first: *first,
                last: *last,
                number: group.number,
                direction,
                edge: staff.tuplet_edge(direction, reach),
                bracket: !beamed,
            });
        }
        self.tuplets = tuplets;
    }

    pub fn extent(&self, staff: &RhythmStaff, events: &[RhythmEvent]) -> Option<Extremes> {
        let mut reach: Option<Extremes> = None;
        for (index, event) in events.iter().enumerate() {
            if let Some(rect) = self.stem_rect(staff, event, index) {
                reach = Some(widen(reach, rect.min.y, rect.max.y));
            }
        }
        for beam in &self.beams {
            let extent = self.beam_extent(staff, beam);
            reach = Some(widen(reach, extent.top, extent.bottom));
        }
        for tuplet in &self.tuplets {
            let extent = staff.tuplet_extent(&TupletBracket {
                left: Units::ZERO,
                right: Units::ZERO,
                edge: tuplet.edge,
                direction: tuplet.direction,
                number: tuplet.number,
                bracket: tuplet.bracket,
            });
            reach = Some(widen(reach, extent.top, extent.bottom));
        }
        reach
    }
}

pub fn slash(fitted: &FittedStem, staff: &RhythmStaff) -> (Point, Point) {
    let flagged = Stem {
        x: fitted.anchor.dx,
        base: fitted.anchor.base,
        tip: fitted.anchor.tip,
        direction: fitted.anchor.direction,
        beams: fitted.beams.max(1),
    };
    let origin = staff.flag_origin(&flagged, fitted.end).map_or(
        Point {
            x: fitted.anchor.dx,
            y: fitted.end,
        },
        |(_, origin)| origin,
    );
    let south_west = anchor(FLAG_8TH_UP, Anchor::GraceNoteSlashSW, staff.size);
    let north_east = anchor(FLAG_8TH_UP, Anchor::GraceNoteSlashNE, staff.size);
    (origin.offset(south_west), origin.offset(north_east))
}

pub struct RhythmCanvas<'a> {
    pub xs: &'a [Units],
    pub top: Units,
    pub events: &'a mut [Vec<Primitive>],
    pub shared: &'a mut Vec<Primitive>,
}

fn absolute(fitted: &FittedStem, x: Units, top: Units) -> Stem {
    Stem {
        x: x + fitted.anchor.dx,
        base: top + fitted.anchor.base,
        tip: top + fitted.anchor.tip,
        direction: fitted.anchor.direction,
        beams: fitted.beams,
    }
}

pub fn clamp_slope(line: BeamLine, span: Units) -> BeamLine {
    let limit = Units(span.0.abs()).scaled(1, SLOPE_DIVISOR);
    BeamLine {
        delta: line.delta.clamp(-limit, limit),
        ..line
    }
}

pub fn draw_rhythm(
    staff: &RhythmStaff,
    plan: &RhythmPlan,
    events: &[RhythmEvent],
    canvas: &mut RhythmCanvas<'_>,
) {
    let top = canvas.top;
    let x_of = |index: usize| canvas.xs.get(index).copied().unwrap_or_default();
    let mut ends: Vec<Option<Units>> = vec![None; events.len()];
    for beam in &plan.beams {
        let stems: Vec<Stem> = beam
            .members
            .iter()
            .filter_map(|index| {
                plan.stem(*index)
                    .map(|fitted| absolute(fitted, x_of(*index), top))
            })
            .collect();
        let (Some(first), Some(last)) = (stems.first(), stems.last()) else {
            continue;
        };
        let line = clamp_slope(
            BeamLine {
                start: beam.line.start + top,
                ..beam.line
            },
            last.x - first.x,
        );
        for (index, stem) in beam.members.iter().zip(&stems) {
            if let Some(slot) = ends.get_mut(*index) {
                *slot = Some(line.at(stem.x, first.x, last.x));
            }
        }
        staff.draw_beams(&stems, &line, canvas.shared);
    }
    for (index, event) in events.iter().enumerate() {
        let Some(fitted) = plan.stem(index) else {
            continue;
        };
        let geometry = staff.for_event(event);
        let stem = absolute(fitted, x_of(index), top);
        let Some(items) = canvas.events.get_mut(index) else {
            continue;
        };
        match ends[index] {
            Some(end) => geometry.draw_stem(&stem, end, items),
            None => {
                let end = top + fitted.end;
                geometry.draw_stem(&stem, end, items);
                geometry.draw_flag(&stem, end, items);
            }
        }
        if event.slash {
            let (from, to) = slash(fitted, &geometry);
            let shift = Point {
                x: x_of(index),
                y: top,
            };
            items.push(Primitive::Line(Line {
                from: from.offset(shift),
                to: to.offset(shift),
                thickness: geometry.stem_thickness(),
                dash: None,
            }));
        }
    }
    for tuplet in &plan.tuplets {
        let (Some(first), Some(last)) = (events.get(tuplet.first), events.get(tuplet.last)) else {
            continue;
        };
        staff.draw_tuplet(
            &TupletBracket {
                left: x_of(tuplet.first) + first.left,
                right: x_of(tuplet.last) + last.right,
                edge: top + tuplet.edge,
                direction: tuplet.direction,
                number: tuplet.number,
                bracket: tuplet.bracket,
            },
            canvas.shared,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_beam_slopes_at_most_a_quarter_of_its_span() {
        let line = BeamLine {
            direction: StemDirection::Up,
            start: Units(-3000),
            delta: Units(-1024),
        };
        assert_eq!(clamp_slope(line, Units(1536)).delta, Units(-384));
        assert_eq!(clamp_slope(line, Units(-1536)).delta, Units(-384));
        assert_eq!(clamp_slope(line, Units(8192)).delta, Units(-1024));
        let rising = BeamLine {
            delta: Units(700),
            ..line
        };
        assert_eq!(clamp_slope(rising, Units(2000)).delta, Units(500));
        assert_eq!(clamp_slope(rising, Units::ZERO).delta, Units::ZERO);
    }
}
