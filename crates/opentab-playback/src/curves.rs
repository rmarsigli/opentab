use opentab_model::{BendPoint, Fraction, Slide, Vibrato};

use crate::PlaybackError;
use crate::bends::BendEvent;
use crate::exact::Exact;
use crate::midi::{BEND_CENTRE, BEND_MAX};

pub(crate) const MUSICAL_BEND_RANGE: u8 = 12;
pub(crate) const CENTS_PER_SEMITONE: i64 = 100;
const STEP_CENTS: i64 = 20;
const PERCENT: i128 = 100;
const PERMILLE: i64 = 1000;
const VIBRATO_PERIOD: Fraction = Fraction::new_raw(1, 16);
const VIBRATO_SINE: [i64; 8] = [0, 707, 1000, 707, 0, -707, -1000, -707];
const VIBRATO_SLIGHT_CENTS: i64 = 25;
const VIBRATO_WIDE_CENTS: i64 = 50;
const SLIDE_IN_CENTS: i64 = 300;
const SLIDE_OUT_CENTS: i64 = 500;
const SLIDE_IN_SPAN: Fraction = Fraction::new_raw(1, 16);
const SLIDE_OUT_SPAN: Fraction = Fraction::new_raw(1, 8);
const GLIDE_SPAN: Fraction = Fraction::new_raw(1, 16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Window {
    pub(crate) start: Exact,
    pub(crate) end: Exact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Shape {
    pub(crate) origin: BendPoint,
    pub(crate) middle_start: BendPoint,
    pub(crate) middle_end: BendPoint,
    pub(crate) destination: BendPoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Knot {
    at: Exact,
    cents: i64,
}

#[derive(Debug, Default)]
pub(crate) struct Curve {
    pub(crate) events: Vec<BendEvent>,
}

pub(crate) fn musical_value(cents: i64) -> u16 {
    let range = i64::from(MUSICAL_BEND_RANGE) * CENTS_PER_SEMITONE;
    let centre = i64::from(BEND_CENTRE);
    let offset = (cents * centre).div_euclid(range);
    let value = (centre + offset).clamp(0, i64::from(BEND_MAX));
    u16::try_from(value).unwrap_or(BEND_CENTRE)
}

fn shorter(window: Window, limit: Fraction) -> Result<Exact, PlaybackError> {
    let half = window.end.sub(window.start)?.mul(Exact::new(1, 2)?)?;
    Ok(half.min(Exact::of(limit)))
}

impl Curve {
    pub(crate) fn point(&mut self, at: Exact, cents: i64) {
        self.events.push(BendEvent {
            at,
            value: musical_value(cents),
        });
    }

    pub(crate) fn glide(
        &mut self,
        from: Exact,
        to: Exact,
        from_cents: i64,
        to_cents: i64,
    ) -> Result<(), PlaybackError> {
        let distance = (to_cents - from_cents).abs();
        let direction = (to_cents - from_cents).signum();
        if to > from {
            let span = to.sub(from)?;
            let mut moved = STEP_CENTS;
            while moved < distance {
                let share = Exact::new(i128::from(moved), i128::from(distance))?;
                self.point(from.add(span.mul(share)?)?, from_cents + direction * moved);
                moved += STEP_CENTS;
            }
        }
        self.point(to.max(from), to_cents);
        Ok(())
    }

    pub(crate) fn shape(
        &mut self,
        window: Window,
        shape: Shape,
        base: i64,
    ) -> Result<Exact, PlaybackError> {
        let length = window.end.sub(window.start)?;
        let mut knots: Vec<Knot> = Vec::with_capacity(4);
        let mut previous = window.start;
        for point in [
            shape.origin,
            shape.middle_start,
            shape.middle_end,
            shape.destination,
        ] {
            let at = window
                .start
                .add(length.mul(Exact::new(i128::from(point.offset()), PERCENT)?)?)?
                .max(previous);
            previous = at;
            knots.push(Knot {
                at,
                cents: base + i64::from(point.value()),
            });
        }
        let mut last = Knot {
            at: window.start,
            cents: knots.first().map_or(base, |knot| knot.cents),
        };
        self.point(last.at, last.cents);
        for knot in knots {
            if knot.cents == last.cents {
                last.at = knot.at;
                continue;
            }
            self.glide(last.at, knot.at, last.cents, knot.cents)?;
            last = knot;
        }
        Ok(last.at)
    }

    pub(crate) fn vibrato(
        &mut self,
        window: Window,
        vibrato: Vibrato,
        base: i64,
    ) -> Result<(), PlaybackError> {
        let depth = match vibrato {
            Vibrato::Slight => VIBRATO_SLIGHT_CENTS,
            Vibrato::Wide => VIBRATO_WIDE_CENTS,
        };
        let samples = i128::try_from(VIBRATO_SINE.len()).unwrap_or(1);
        let step = Exact::of(VIBRATO_PERIOD).div(Exact::integer(samples))?;
        let mut index: usize = 1;
        loop {
            let offset = step.mul(Exact::integer(i128::try_from(index).unwrap_or(i128::MAX)))?;
            let at = window.start.add(offset)?;
            if at >= window.end {
                return Ok(());
            }
            let sine = VIBRATO_SINE[index % VIBRATO_SINE.len()];
            self.point(at, base + depth * sine / PERMILLE);
            index += 1;
        }
    }

    pub(crate) fn slide(
        &mut self,
        window: Window,
        slide: Slide,
        base: i64,
    ) -> Result<(), PlaybackError> {
        match slide {
            Slide::IntoFromBelow | Slide::IntoFromAbove => {
                let from = if slide == Slide::IntoFromBelow {
                    base - SLIDE_IN_CENTS
                } else {
                    base + SLIDE_IN_CENTS
                };
                let span = shorter(window, SLIDE_IN_SPAN)?;
                self.point(window.start, from);
                self.glide(window.start, window.start.add(span)?, from, base)
            }
            Slide::OutDownwards | Slide::OutUpwards => {
                let to = if slide == Slide::OutDownwards {
                    base - SLIDE_OUT_CENTS
                } else {
                    base + SLIDE_OUT_CENTS
                };
                let span = shorter(window, SLIDE_OUT_SPAN)?;
                let from = window.end.sub(span)?;
                self.point(from, base);
                self.glide(from, window.end, base, to)
            }
            Slide::Shift | Slide::Legato => Ok(()),
        }
    }

    pub(crate) fn towards(
        &mut self,
        origin: Window,
        from_cents: i64,
        to_cents: i64,
    ) -> Result<(), PlaybackError> {
        let span = shorter(origin, GLIDE_SPAN)?;
        let from = origin.end.sub(span)?;
        self.point(from, from_cents);
        self.glide(from, origin.end, from_cents, to_cents)
    }
}
