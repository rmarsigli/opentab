use opentab_model::{Bend, BendPoint, Note};

use crate::PlaybackError;
use crate::exact::Exact;
use crate::midi::{BEND_CENTRE, BEND_MAX, BEND_RANGE_SEMITONES};

const CENTS_PER_UNIT: i32 = 2;
const UNITS_PER_SEMITONE: i32 = 50;
const STEPS_PER_RANGE: i32 = 25;
const PERCENT: i128 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BendEvent {
    pub(crate) at: Exact,
    pub(crate) value: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Contour {
    Line,
    Peak,
}

#[derive(Debug, Clone, Copy)]
struct Shaped {
    bend: Bend,
    contour: Contour,
}

#[derive(Debug, Clone, Copy)]
struct Window {
    start: Exact,
    length: Exact,
}

#[derive(Debug, Clone, Copy)]
struct Knot {
    at: Exact,
    units: i32,
}

fn range_units() -> i32 {
    i32::from(BEND_RANGE_SEMITONES) * UNITS_PER_SEMITONE
}

fn step_units() -> i32 {
    range_units() / STEPS_PER_RANGE
}

pub(crate) fn units_of(cents: i16) -> i32 {
    i32::from(cents) / CENTS_PER_UNIT
}

pub(crate) fn bend_value(units: i32) -> u16 {
    let centre = i64::from(BEND_CENTRE);
    let reach = i64::from(BEND_MAX) - centre;
    let offset = (i64::from(units) * reach).div_euclid(i64::from(range_units()));
    let value = (centre + offset).clamp(0, i64::from(BEND_MAX));
    u16::try_from(value).unwrap_or(BEND_CENTRE)
}

impl Window {
    fn at(self, offset: u8) -> Result<Exact, PlaybackError> {
        self.length
            .mul(Exact::new(i128::from(offset), PERCENT)?)
            .and_then(|span| self.start.add(span))
    }

    fn knot(self, point: BendPoint) -> Result<Knot, PlaybackError> {
        Ok(Knot {
            at: self.at(point.offset())?,
            units: units_of(point.value()),
        })
    }
}

fn line(out: &mut Vec<BendEvent>, from: Knot, to: Knot) -> Result<(), PlaybackError> {
    let end = to.at.max(from.at);
    let distance = (to.units - from.units).abs();
    let direction = (to.units - from.units).signum();
    let step = step_units();
    let mut k: i32 = 1;
    while step * k <= distance {
        let moved = step * k;
        let fraction = Exact::new(i128::from(moved), i128::from(distance))?;
        let at = end.sub(from.at)?.mul(fraction)?;
        out.push(BendEvent {
            at: from.at.add(at)?,
            value: bend_value(from.units + direction * moved),
        });
        k += 1;
    }
    out.push(BendEvent {
        at: end,
        value: bend_value(to.units),
    });
    Ok(())
}

fn shape(out: &mut Vec<BendEvent>, window: Window, shaped: Shaped) -> Result<(), PlaybackError> {
    let bend = shaped.bend;
    let origin = window.knot(bend.origin())?;
    let middle_start = window.knot(bend.middle_start())?;
    let middle_end = window.knot(bend.middle_end())?;
    let destination = window.knot(bend.destination())?;
    out.push(BendEvent {
        at: window.start,
        value: bend_value(origin.units),
    });
    match shaped.contour {
        Contour::Peak => {
            line(out, origin, middle_start)?;
            let held = middle_end.at.max(middle_start.at);
            out.push(BendEvent {
                at: held,
                value: bend_value(middle_end.units),
            });
            line(
                out,
                Knot {
                    at: held,
                    units: middle_end.units,
                },
                destination,
            )?;
        }
        Contour::Line => line(out, origin, destination)?,
    }
    Ok(())
}

fn at_pitch_or_above(point: BendPoint) -> BendPoint {
    BendPoint::new(point.offset(), point.value().max(0)).unwrap_or(point)
}

fn gp_contour(bend: Bend) -> Option<Shaped> {
    let origin = at_pitch_or_above(bend.origin());
    let destination = at_pitch_or_above(bend.destination());
    let middle = bend.middle_value();
    if middle < origin.value().min(destination.value()) {
        return None;
    }
    let contour = if middle > origin.value().max(destination.value()) {
        Contour::Peak
    } else {
        Contour::Line
    };
    let bend = Bend::new(origin, bend.middle_start(), bend.middle_end(), destination).ok()?;
    Some(Shaped { bend, contour })
}

pub(crate) fn gp_drops(bend: Bend) -> bool {
    gp_contour(bend).is_none()
}

pub(crate) fn bend_events(
    note: &Note,
    start: Exact,
    length: Exact,
) -> Result<Vec<BendEvent>, PlaybackError> {
    let window = Window { start, length };
    let bent = note.effects.bend.and_then(gp_contour);
    let flat = BendPoint::default();
    let shaped = match (bent, note.effects.vibrato) {
        (Some(shaped), _) => Some(shaped),
        (None, Some(_)) => Bend::new(flat, flat, flat, BendPoint::new(100, 0).unwrap_or(flat))
            .ok()
            .map(|bend| Shaped {
                bend,
                contour: Contour::Line,
            }),
        (None, None) => None,
    };
    let Some(shaped) = shaped else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    shape(&mut out, window, shaped)?;
    out.push(BendEvent {
        at: start.add(length)?,
        value: BEND_CENTRE,
    });
    Ok(out)
}
