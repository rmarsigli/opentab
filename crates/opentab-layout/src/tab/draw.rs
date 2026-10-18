use std::collections::BTreeMap;

use super::TabNotation;
use super::bend::draw_bends;
use super::effects::{draw_finger, draw_label, draw_stroke, line, slide_segment, text};
use super::label::{LABEL_PAD, MARK_EM};
use super::runs::LinkAnchor;
use crate::display::{Group, GroupId, Primitive, TextAnchor};
use crate::lanes::{BandLayout, Lane};
use crate::structure::{BarLayout, StaffLayout};
use crate::ties::TieAnchor;
use crate::units::{Point, Units};

pub struct TabRow<'a> {
    pub position: usize,
    pub links: &'a mut Vec<LinkAnchor>,
    pub sequences: &'a mut BTreeMap<u8, usize>,
    pub ties: &'a mut Vec<TieAnchor>,
    pub tie_sequences: &'a mut BTreeMap<u8, usize>,
}

fn record(tab: &TabNotation, index: usize, x: Units, top: Units, row: &mut TabRow<'_>) {
    let Some(beat) = tab.beats.get(index) else {
        return;
    };
    let counter = row.sequences.entry(beat.voice_slot).or_insert(0);
    let sequence = *counter;
    *counter += 1;
    let centre = x + beat.centre;
    for fret in &beat.frets {
        let label = fret.label();
        row.links.push(LinkAnchor {
            bar: row.position,
            voice: beat.voice_slot,
            sequence,
            line: fret.line,
            left: centre + label.min.x,
            right: centre + label.max.x,
            centre: centre + fret.dx,
            y: top + fret.y,
            top: top + label.min.y,
            link: fret.link,
            incoming: fret.incoming,
            size: beat.size,
        });
    }
    if beat.slot.grace.is_some() || !tab.alone {
        return;
    }
    let counter = row.tie_sequences.entry(beat.voice_slot).or_insert(0);
    let sequence = *counter;
    *counter += 1;
    for fret in &beat.frets {
        let label = fret.label();
        row.ties.push(TieAnchor {
            bar: row.position,
            voice: beat.voice_slot,
            sequence,
            key: -1 - i64::try_from(fret.line).unwrap_or(i64::MAX),
            left: centre + label.min.x,
            right: centre + label.max.x,
            y: top + fret.y + LABEL_PAD,
            up: false,
            origin: fret.tie_origin,
            destination: fret.tie_destination,
            size: beat.size,
        });
    }
}

pub fn draw_bar(
    tab: &TabNotation,
    bar: &BarLayout,
    staff: &StaffLayout,
    band: &BandLayout,
    items: &mut Vec<Primitive>,
    row: &mut TabRow<'_>,
) {
    let top = staff.top;
    let xs: Vec<Units> = tab
        .beats
        .iter()
        .map(|beat| bar.x_of(&beat.slot).unwrap_or(bar.content))
        .collect();
    let marks = band.lane(Lane::Marks);
    let mut per_beat: Vec<Vec<Primitive>> = Vec::with_capacity(tab.beats.len());
    for (index, (beat, x)) in tab.beats.iter().zip(&xs).enumerate() {
        let at = Point {
            x: *x + beat.centre,
            y: top,
        };
        let mut beat_items = Vec::new();
        for fret in &beat.frets {
            let mut note_items = Vec::new();
            if !fret.merged {
                draw_label(fret, at, &mut note_items);
            }
            beat_items.push(Primitive::Group(Group {
                id: GroupId(fret.id.0.0),
                items: note_items,
            }));
        }
        for fret in &beat.frets {
            if let Some(finger) = fret.finger {
                draw_finger(fret, finger, at, &mut beat_items);
            }
            if let Some((from, to)) = fret.slide.and_then(|slide| slide_segment(fret, slide)) {
                beat_items.push(line(from.offset(at), to.offset(at)));
            }
        }
        draw_bends(&beat.bends, at, &mut beat_items);
        draw_stroke(beat, at, &mut beat_items);
        if let (Some(mark), Some(lane)) = (&beat.mark, marks) {
            beat_items.push(text(
                mark,
                Point {
                    x: at.x,
                    y: top + lane.bottom,
                },
                MARK_EM,
                TextAnchor::Middle,
            ));
        }
        record(tab, index, *x, top, row);
        per_beat.push(beat_items);
    }
    let mut shared = Vec::new();
    if let Some(rhythm) = &tab.rhythm {
        rhythm.draw(&xs, top, &mut per_beat, &mut shared);
    }
    for (beat, beat_items) in tab.beats.iter().zip(per_beat) {
        items.push(Primitive::Group(Group {
            id: GroupId(beat.id.0.0),
            items: beat_items,
        }));
    }
    items.extend(shared);
}
