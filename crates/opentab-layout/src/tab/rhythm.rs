use opentab_model::{Bar, NoteValue, TremoloPicking};

use super::{TabBeat, TabInput};
use crate::display::Primitive;
use crate::glyphs::{Glyph, TREMOLO_1, TREMOLO_2, TREMOLO_3};
use crate::rhythm::{
    DOT_GAP, Extremes, RhythmCanvas, RhythmEvent, RhythmGroups, RhythmPlan, RhythmStaff,
    STEM_LENGTH, StemAnchor, StemDirection, StemPolicy, draw_rhythm, fit_stems, glyph, glyph_box,
    group, rest_glyph, stemmed,
};
use crate::units::{Point, Rect, STAFF_SPACE, Units};

pub const RHYTHM_GAP: Units = STAFF_SPACE;
pub const TAB_DOT_GAP: Units = Units(STAFF_SPACE.0 / 4);

pub fn tab_staff(height: Units) -> RhythmStaff {
    RhythmStaff {
        reference: height + RHYTHM_GAP + STEM_LENGTH.half(),
        spacing: STAFF_SPACE,
        policy: StemPolicy::Fixed(StemDirection::Down),
        size: STAFF_SPACE,
        reach_reference: false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabRhythm {
    pub staff: RhythmStaff,
    pub members: Vec<usize>,
    pub events: Vec<RhythmEvent>,
    pub groups: RhythmGroups,
    pub plan: RhythmPlan,
    pub rests: Vec<Option<Glyph>>,
    pub dots: Vec<u8>,
    pub tremolos: Vec<Option<Glyph>>,
    pub centre: Units,
}

pub fn tremolo_glyph(tremolo: TremoloPicking) -> Glyph {
    match tremolo.value() {
        NoteValue::Eighth => TREMOLO_1,
        NoteValue::Sixteenth => TREMOLO_2,
        _ => TREMOLO_3,
    }
}

fn voice_of(beats: &[TabBeat]) -> Option<usize> {
    beats
        .iter()
        .filter(|beat| beat.slot.grace.is_none())
        .map(|beat| beat.slot.voice)
        .min()
}

pub fn analyse(
    input: &TabInput<'_>,
    bar: &Bar,
    beats: &[TabBeat],
    height: Units,
) -> Option<TabRhythm> {
    let voice = voice_of(beats)?;
    let staff = tab_staff(height);
    let base = height + RHYTHM_GAP;
    let mut members = Vec::new();
    let mut events = Vec::new();
    let mut anchors = Vec::new();
    let mut rests = Vec::new();
    let mut dots = Vec::new();
    let mut tremolos = Vec::new();
    let centre = super::half_head(STAFF_SPACE);
    for (index, beat) in beats.iter().enumerate() {
        if beat.slot.voice != voice || beat.slot.grace.is_some() {
            continue;
        }
        let Some(model) = bar
            .voices
            .get(beat.slot.voice)
            .and_then(|found| found.beats.get(beat.slot.beat))
        else {
            continue;
        };
        let onset = input
            .columns
            .get(beat.slot.column)
            .map_or(0, |column| column.ticks);
        let sounding = !model.notes.is_empty();
        let value = model.duration.value();
        members.push(index);
        events.push(RhythmEvent {
            voice,
            onset,
            duration: model.duration,
            grace: false,
            sounding,
            slash: false,
            extremes: Some(Extremes {
                top: base,
                bottom: base,
            }),
            left: Units::ZERO,
            right: Units::ZERO,
        });
        let base_of = if value == NoteValue::Half {
            base + STEM_LENGTH.half()
        } else {
            base
        };
        anchors.push((sounding && stemmed(value)).then_some(StemAnchor {
            dx: beat.centre,
            base: base_of,
            tip: base,
            direction: StemDirection::Down,
        }));
        rests.push((!sounding).then(|| rest_glyph(value)));
        dots.push(model.duration.dots());
        tremolos.push(
            model
                .effects
                .tremolo_picking
                .filter(|_| sounding)
                .map(tremolo_glyph),
        );
    }
    if members.is_empty() {
        return None;
    }
    let groups = group(input.time, input.per_whole, &events);
    let mut plan = fit_stems(&staff, &events, &anchors, &groups);
    let mut rhythm = TabRhythm {
        staff,
        members,
        events,
        groups,
        plan: RhythmPlan::default(),
        rests,
        dots,
        tremolos,
        centre,
    };
    for item in &mut rhythm.events {
        item.left = rhythm.centre - STAFF_SPACE.half();
        item.right = rhythm.centre + STAFF_SPACE.half();
    }
    let inks: Vec<Option<Extremes>> = (0..rhythm.members.len())
        .map(|event| {
            rhythm.event_rect(&plan, event).map(|rect| Extremes {
                top: rect.min.y,
                bottom: rect.max.y,
            })
        })
        .collect();
    plan.place_tuplets(&rhythm.staff, &rhythm.groups, &inks);
    rhythm.plan = plan;
    Some(rhythm)
}

impl TabRhythm {
    fn rest_origin(&self, event: usize) -> Option<(Glyph, Point)> {
        let rest = self.rests.get(event).copied().flatten()?;
        let value = self.events.get(event)?.duration.value();
        let rect = glyph_box(rest, self.staff.size);
        Some((
            rest,
            Point {
                x: self.centre - rect.min.x - rect.width().half(),
                y: self.staff.rest_y(value, Units::ZERO),
            },
        ))
    }

    fn dots_origin(&self, plan: &RhythmPlan, event: usize) -> Option<(Point, u8)> {
        let count = self.dots.get(event).copied().filter(|dots| *dots > 0)?;
        let right = match self.rest_origin(event) {
            Some((rest, origin)) => origin.x + glyph_box(rest, self.staff.size).max.x,
            None => {
                let stem = plan.stem_rect(&self.staff, self.events.get(event)?, event);
                stem.map_or(self.centre, |rect| rect.max.x)
            }
        };
        Some((
            Point {
                x: right + TAB_DOT_GAP - self.staff.length(DOT_GAP),
                y: self.staff.reference - STAFF_SPACE.half(),
            },
            count,
        ))
    }

    fn tremolo_origin(&self, plan: &RhythmPlan, event: usize) -> Option<(Glyph, Point)> {
        let glyph = self.tremolos.get(event).copied().flatten()?;
        let y = match plan.stem(event) {
            Some(fitted) => fitted.anchor.base + (fitted.end - fitted.anchor.base).half(),
            None => self.staff.reference,
        };
        Some((glyph, Point { x: self.centre, y }))
    }

    fn event_rect(&self, plan: &RhythmPlan, event: usize) -> Option<Rect> {
        let mut rects: Vec<Rect> = Vec::new();
        if let Some(item) = self.events.get(event)
            && let Some(rect) = plan.stem_rect(&self.staff, item, event)
        {
            rects.push(rect);
        }
        if let Some((rest, origin)) = self.rest_origin(event) {
            let rect = glyph_box(rest, self.staff.size);
            rects.push(Rect::new(origin.offset(rect.min), origin.offset(rect.max)));
        }
        if let Some((glyph_name, origin)) = self.tremolo_origin(plan, event) {
            let rect = glyph_box(glyph_name, self.staff.size);
            rects.push(Rect::new(origin.offset(rect.min), origin.offset(rect.max)));
        }
        if let Some((origin, count)) = self.dots_origin(plan, event) {
            let width = self.staff.dots_width(count);
            rects.push(Rect::new(
                Point {
                    x: origin.x,
                    y: origin.y - STAFF_SPACE.half().half(),
                },
                Point {
                    x: origin.x + width,
                    y: origin.y + STAFF_SPACE.half().half(),
                },
            ));
        }
        let (first, rest) = rects.split_first()?;
        Some(rest.iter().fold(*first, |total, rect| {
            total.including(rect.min).including(rect.max)
        }))
    }

    pub fn ink(&self, beat: usize) -> Option<Rect> {
        let event = self.members.iter().position(|member| *member == beat)?;
        self.event_rect(&self.plan, event)
    }

    pub fn reach(&self) -> Option<Units> {
        let mut bottom = self
            .plan
            .extent(&self.staff, &self.events)
            .map(|extent| extent.bottom);
        for event in 0..self.events.len() {
            if let Some(rect) = self.event_rect(&self.plan, event) {
                bottom = Some(bottom.map_or(rect.max.y, |known| known.max(rect.max.y)));
            }
        }
        bottom
    }

    pub fn draw(
        &self,
        xs: &[Units],
        top: Units,
        per_beat: &mut [Vec<Primitive>],
        shared: &mut Vec<Primitive>,
    ) {
        let member_xs: Vec<Units> = self
            .members
            .iter()
            .map(|member| xs.get(*member).copied().unwrap_or_default())
            .collect();
        let mut items: Vec<Vec<Primitive>> = vec![Vec::new(); self.members.len()];
        draw_rhythm(
            &self.staff,
            &self.plan,
            &self.events,
            &mut RhythmCanvas {
                xs: &member_xs,
                top,
                events: &mut items,
                shared,
            },
        );
        for (event, x) in member_xs.iter().enumerate() {
            let shift = Point { x: *x, y: top };
            let Some(list) = items.get_mut(event) else {
                continue;
            };
            if let Some((rest, origin)) = self.rest_origin(event) {
                list.push(glyph(rest, origin.offset(shift), self.staff.size));
            }
            if let Some((tremolo, origin)) = self.tremolo_origin(&self.plan, event) {
                list.push(glyph(tremolo, origin.offset(shift), self.staff.size));
            }
            if let Some((origin, count)) = self.dots_origin(&self.plan, event) {
                let at = origin.offset(shift);
                self.staff.draw_dots(at.x, &[at.y], count, list);
            }
        }
        for (member, list) in self.members.iter().zip(items) {
            if let Some(target) = per_beat.get_mut(*member) {
                target.extend(list);
            }
        }
    }
}
