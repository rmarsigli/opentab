use std::collections::BTreeMap;

use opentab_model::Score;

use crate::barline::EndSign;
use crate::barline::Strokes;
use crate::display::{
    Colour, FilledRect, Font, Group, GroupId, Line, PlacedGlyph, Primitive, TextAnchor, TextRun,
};
use crate::engine::Stack;
use crate::furniture::{draw_clef, draw_time};
use crate::geometry::{
    BRACKET_GAP, COUNT_EM, COUNT_RISE, ENDING_EM, ENDING_HOOK, ENDING_INSET, ENDING_LABEL_DROP,
    ENDING_RISE, NAME_EM, NAME_GAP, NAME_MAX, NUMBER_EM, NUMBER_RISE,
};
use crate::glyphs::{BRACKET_BOTTOM, BRACKET_TOP, Metrics};
use crate::marks::{SystemMarks, draw_marks};
use crate::notes::{TieRow, draw_bar};
use crate::options::LayoutError;
use crate::plan::{BarPlan, SystemPlan};
use crate::select::Selection;
use crate::spacing::{Piece, place};
use crate::structure::{BarLayout, ClefSign, Column, StaffKind, StaffLayout, SystemLayout};
use crate::tab::{LinkAnchor, RowEnds, RowParts, TabNotation, TabRow, finish_row};
use crate::text::text_font;
use crate::ties::{TIE_END_GAP, TieAnchor, TieSpan, draw_ties};
use crate::units::{Point, Rect, STAFF_SPACE, Units};

pub struct Context<'a> {
    pub score: &'a Score,
    pub selection: &'a Selection,
    pub plans: &'a [BarPlan],
    pub strokes: Strokes,
    pub labels: &'a [Label],
    pub bracket: bool,
}

pub struct Label {
    pub track: usize,
    pub text: String,
}

pub fn fit_label(text: &str, max: Units) -> String {
    let mut label: Vec<char> = text.chars().collect();
    while !label.is_empty() && text_font().measure(&label.iter().collect::<String>(), NAME_EM) > max
    {
        label.pop();
    }
    label.into_iter().collect()
}

pub struct DrawnSystem {
    pub structure: SystemLayout,
    pub items: Vec<Primitive>,
    pub crowded: bool,
}

pub fn labels(score: &Score, selection: &Selection) -> Vec<Label> {
    if selection.tracks.len() < 2 {
        return Vec::new();
    }
    let max = NAME_MAX;
    selection
        .tracks
        .iter()
        .filter_map(|index| {
            let track = score.tracks.get(*index)?;
            let name = if track.short_name.trim().is_empty() {
                &track.name
            } else {
                &track.short_name
            };
            Some(Label {
                track: *index,
                text: fit_label(name.trim(), max),
            })
        })
        .collect()
}

pub fn draw_system(
    context: &Context<'_>,
    system: &SystemPlan,
    index: usize,
    top: Units,
    stack: &Stack,
) -> Result<DrawnSystem, LayoutError> {
    let plans = &context.plans[system.first..system.end];
    let strokes = context.strokes;
    let mut pieces = Vec::new();
    let mut bases = Vec::with_capacity(plans.len());
    for (offset, plan) in plans.iter().enumerate() {
        bases.push(pieces.len());
        pieces.extend(plan.pieces(offset == 0, strokes));
    }
    let first = place(&pieces, system.target).ok_or(LayoutError::PageTooSmall)?;
    let placement = if first.crowded {
        let contained: Vec<Piece> = plans
            .iter()
            .enumerate()
            .flat_map(|(offset, plan)| plan.contained_pieces(offset == 0, strokes))
            .collect();
        let mut again = place(&contained, system.target).ok_or(LayoutError::PageTooSmall)?;
        again.crowded = true;
        again
    } else {
        first
    };
    let positions = &placement.positions;
    let x = |piece: usize| system.left + positions.get(piece).copied().unwrap_or_default();
    let right = system.left + system.target;
    let thickness = Metrics::bravura().engraving_defaults().staff_line_thickness;
    let mut staves = Vec::with_capacity(context.selection.specs.len());
    let mut tracks: Vec<(usize, Vec<Primitive>)> = Vec::new();
    for (row, spec) in context.selection.specs.iter().enumerate() {
        let mut staff = StaffLayout {
            track: spec.track,
            staff: spec.staff,
            kind: spec.kind,
            top: top + stack.tops.get(row).copied().unwrap_or_default(),
            lines: spec.shape.lines,
            line_spacing: spec.shape.spacing,
            bars: Vec::with_capacity(plans.len()),
        };
        let mut items = Vec::new();
        for line in 0..staff.lines {
            let y = staff.line_y(line);
            items.push(Primitive::Line(Line {
                from: Point { x: system.left, y },
                to: Point { x: right, y },
                thickness,
                dash: None,
            }));
        }
        let (span_top, span_bottom) = staff.span();
        let mut bar_groups: Vec<(GroupId, Vec<Primitive>)> = Vec::with_capacity(plans.len());
        let mut anchors: Vec<TieAnchor> = Vec::new();
        let mut sequences: BTreeMap<u8, usize> = BTreeMap::new();
        let mut links: Vec<LinkAnchor> = Vec::new();
        let mut link_sequences: BTreeMap<u8, usize> = BTreeMap::new();
        let mut tab_ties: Vec<TieAnchor> = Vec::new();
        let mut tab_tie_sequences: BTreeMap<u8, usize> = BTreeMap::new();
        let band = stack.bands.get(row).cloned().unwrap_or_default();
        for (offset, plan) in plans.iter().enumerate() {
            let at_start = offset == 0;
            let base = bases[offset];
            let slots = plan.slots(at_start);
            let left = x(base);
            let bar_right = x(base + slots.right);
            let head = plan.head(at_start);
            let head_left = x(base + slots.head);
            let group = plan.groups.get(spec.source).copied().unwrap_or(GroupId(0));
            let mut bar_items = Vec::new();
            let clef = plan.clefs.get(row).copied().unwrap_or(ClefSign::Treble);
            if let Some(Some(sign)) = head.clefs.get(row) {
                draw_clef(*sign, head_left + head.clef_x, &staff, &mut bar_items);
            }
            if let (Some(Some(key)), StaffKind::Standard) = (head.keys.get(row), spec.kind) {
                key.draw(clef, head_left + head.key_x, &staff, &mut bar_items);
            }
            if let Some(time) = head.time {
                draw_time(time, head_left + head.time_x, &staff, &mut bar_items);
            }
            if plan.repeat_start {
                let repeat_left = x(base + slots.repeat);
                if !at_start && plan.shares_thick {
                    bar_items.extend(strokes.draw_shared_start_repeat(repeat_left, &staff));
                } else {
                    bar_items.extend(strokes.draw_start_repeat(repeat_left, &staff));
                }
            }
            let replaced = plans.get(offset + 1).is_some_and(|next| {
                next.repeat_start && matches!(plan.end, EndSign::Normal | EndSign::Double)
            });
            if !replaced {
                bar_items.extend(strokes.draw_end(plan.end, bar_right, &staff));
            }
            let columns = plan
                .grid
                .columns
                .iter()
                .enumerate()
                .map(|(column_index, column)| {
                    let extent = plan.extents.get(column_index).copied().unwrap_or_default();
                    Column {
                        ticks: column.ticks,
                        x: x(base + slots.columns + column_index),
                        left: extent.left,
                        right: extent.right,
                        graces: column.graces,
                        lead: extent.lead,
                        grace_advance: extent.grace_advance,
                        grace_lead: extent.grace_lead,
                    }
                })
                .collect();
            staff.bars.push(BarLayout {
                index: plan.index,
                number: plan.number,
                group,
                rect: Rect::new(
                    Point {
                        x: left,
                        y: span_top,
                    },
                    Point {
                        x: bar_right,
                        y: span_bottom,
                    },
                ),
                content: x(base + slots.lead),
                start_width: plan.natural(true, strokes),
                inline_width: plan.natural(false, strokes),
                clef,
                ticks_per_whole: plan.grid.per_whole,
                span_ticks: plan.grid.span,
                columns,
                beats: plan
                    .grid
                    .slots
                    .get(spec.source)
                    .cloned()
                    .unwrap_or_default(),
            });
            if let (Some(Some(notation)), Some(layout)) =
                (plan.notation.get(row), staff.bars.last())
            {
                draw_bar(
                    notation,
                    layout,
                    &staff,
                    &mut bar_items,
                    &mut TieRow {
                        position: offset,
                        anchors: &mut anchors,
                        sequences: &mut sequences,
                    },
                );
            }
            if let (Some(Some(tab)), Some(layout)) = (plan.tab.get(row), staff.bars.last()) {
                crate::tab::draw_bar(
                    tab,
                    layout,
                    &staff,
                    &band,
                    &mut bar_items,
                    &mut TabRow {
                        position: offset,
                        links: &mut links,
                        sequences: &mut link_sequences,
                        ties: &mut tab_ties,
                        tie_sequences: &mut tab_tie_sequences,
                    },
                );
            }
            bar_groups.push((group, bar_items));
        }
        let mut contents: Vec<Vec<Primitive>> = bar_groups
            .iter_mut()
            .map(|(_, items)| std::mem::take(items))
            .collect();
        let span = TieSpan {
            incoming: (system.first > 0)
                .then(|| staff.bars.first().map(|bar| bar.content))
                .flatten(),
            outgoing: (system.end < context.plans.len()).then_some(right - TIE_END_GAP),
            last_bar: plans.len().saturating_sub(1),
        };
        draw_ties(&anchors, span, &sequences, &mut contents);
        if spec.kind == StaffKind::Tab {
            let tabs: Vec<Option<&TabNotation>> = plans
                .iter()
                .map(|plan| plan.tab.get(row).and_then(Option::as_ref))
                .collect();
            let next = context
                .plans
                .get(system.end)
                .and_then(|plan| plan.tab.get(row))
                .and_then(Option::as_ref)
                .and_then(|tab| tab.marks.first())
                .copied();
            finish_row(
                &RowParts {
                    tabs: &tabs,
                    bars: &staff.bars,
                    band: &band,
                    top: staff.top,
                    next,
                    links: &links,
                    sequences: &link_sequences,
                    ties: &tab_ties,
                    tie_sequences: &tab_tie_sequences,
                },
                RowEnds {
                    incoming: span.incoming,
                    outgoing: span.outgoing,
                    last_bar: span.last_bar,
                },
                &mut contents,
            );
        }
        for ((group, _), bar_items) in bar_groups.into_iter().zip(contents) {
            items.push(Primitive::Group(Group {
                id: group,
                items: bar_items,
            }));
        }
        match tracks.last_mut() {
            Some((track, collected)) if *track == spec.track => collected.extend(items),
            _ => tracks.push((spec.track, items)),
        }
        staves.push(staff);
    }
    let mut items: Vec<Primitive> = tracks
        .into_iter()
        .map(|(track, items)| {
            let id = context
                .score
                .tracks
                .get(track)
                .map_or(GroupId(0), |found| GroupId(found.id.0.0));
            Primitive::Group(Group { id, items })
        })
        .collect();
    system_marks(context, system, index, &staves, &x, &bases, &mut items);
    let number = plans
        .first()
        .filter(|plan| index > 0 || plan.index > 0)
        .map(|plan| plan.number.to_string());
    draw_marks(
        &SystemMarks {
            plans,
            staves: &staves,
            bands: &stack.bands,
            left: system.left,
            right: system.left + system.available,
            number,
        },
        &mut items,
    );
    let structure = SystemLayout {
        index,
        rect: Rect::new(
            Point {
                x: system.left,
                y: top,
            },
            Point {
                x: right,
                y: top + stack.height,
            },
        ),
        natural: system.natural,
        available: system.available,
        staves,
    };
    Ok(DrawnSystem {
        structure,
        items,
        crowded: placement.crowded,
    })
}

pub fn system_marks(
    context: &Context<'_>,
    system: &SystemPlan,
    index: usize,
    staves: &[StaffLayout],
    x: &dyn Fn(usize) -> Units,
    bases: &[usize],
    items: &mut Vec<Primitive>,
) {
    let strokes = context.strokes;
    let (Some(first), Some(last)) = (staves.first(), staves.last()) else {
        return;
    };
    let top = first.span().0;
    let bottom = last.span().1;
    if staves.len() > 1 {
        items.push(Primitive::Line(Line {
            from: Point {
                x: system.left + strokes.thin.half(),
                y: top,
            },
            to: Point {
                x: system.left + strokes.thin.half(),
                y: bottom,
            },
            thickness: strokes.thin,
            dash: None,
        }));
    }
    if context.bracket {
        bracket(system.left, top, bottom, items);
    }
    if index == 0 {
        names(context, system, staves, items);
    }
    let plans = &context.plans[system.first..system.end];
    if let Some(plan) = plans.first()
        && (index > 0 || plan.index > 0)
    {
        items.push(text(
            plan.number.to_string(),
            Point {
                x: system.left,
                y: first.top - NUMBER_RISE,
            },
            NUMBER_EM,
            TextAnchor::Start,
        ));
    }
    let ending_y = first.top - ENDING_RISE;
    let thickness = Metrics::bravura()
        .engraving_defaults()
        .repeat_ending_line_thickness;
    let mut offset = 0;
    while offset < plans.len() {
        let mask = plans[offset].ending;
        let mut end = offset + 1;
        while end < plans.len() && plans[end].ending == mask && mask != 0 {
            end += 1;
        }
        if mask != 0 {
            let first_plan = &plans[offset];
            let start_x = if offset == 0 {
                x(bases[offset] + first_plan.slots(true).repeat)
            } else {
                x(bases[offset])
            };
            let last = &plans[end - 1];
            let end_x = x(bases[end - 1] + last.slots(end - 1 == 0).right) - ENDING_INSET;
            let continued = offset == 0
                && system.first > 0
                && context
                    .plans
                    .get(system.first - 1)
                    .is_some_and(|before| before.ending == mask);
            let line = |from: Point, to: Point| {
                Primitive::Line(Line {
                    from,
                    to,
                    thickness,
                    dash: None,
                })
            };
            items.push(line(
                Point {
                    x: start_x,
                    y: ending_y,
                },
                Point {
                    x: end_x,
                    y: ending_y,
                },
            ));
            if !continued {
                items.push(line(
                    Point {
                        x: start_x,
                        y: ending_y,
                    },
                    Point {
                        x: start_x,
                        y: ending_y + ENDING_HOOK,
                    },
                ));
            }
            if last.repeat_count.is_some() {
                items.push(line(
                    Point {
                        x: end_x,
                        y: ending_y,
                    },
                    Point {
                        x: end_x,
                        y: ending_y + ENDING_HOOK,
                    },
                ));
            }
            if !continued {
                items.push(text(
                    ending_label(mask),
                    Point {
                        x: start_x + ENDING_INSET,
                        y: ending_y + ENDING_LABEL_DROP,
                    },
                    ENDING_EM,
                    TextAnchor::Start,
                ));
            }
        }
        offset = end;
    }
    for (offset, plan) in plans.iter().enumerate() {
        if let Some(count) = plan
            .repeat_count
            .filter(|count| *count > 2 && plan.ending == 0)
        {
            let bar_right = x(bases[offset] + plan.slots(offset == 0).right);
            items.push(text(
                format!("\u{d7}{count}"),
                Point {
                    x: bar_right,
                    y: first.top - COUNT_RISE,
                },
                COUNT_EM,
                TextAnchor::End,
            ));
        }
    }
}

pub fn bracket(left: Units, top: Units, bottom: Units, items: &mut Vec<Primitive>) {
    let thickness = Metrics::bravura().engraving_defaults().bracket_thickness;
    let outer = left - BRACKET_GAP - thickness;
    items.push(Primitive::FilledRect(FilledRect {
        rect: Rect::new(
            Point { x: outer, y: top },
            Point {
                x: left - BRACKET_GAP,
                y: bottom,
            },
        ),
        colour: Colour::Black,
    }));
    for (glyph, y) in [(BRACKET_TOP, top), (BRACKET_BOTTOM, bottom)] {
        items.push(Primitive::Glyph(PlacedGlyph {
            origin: Point { x: outer, y },
            codepoint: glyph.codepoint,
            staff_space: STAFF_SPACE,
        }));
    }
}

pub fn names(
    context: &Context<'_>,
    system: &SystemPlan,
    staves: &[StaffLayout],
    items: &mut Vec<Primitive>,
) {
    let edge = system.left
        - if context.bracket {
            BRACKET_GAP + Metrics::bravura().engraving_defaults().bracket_thickness
        } else {
            Units::ZERO
        }
        - NAME_GAP;
    for label in context.labels {
        let mut rows = staves.iter().filter(|staff| staff.track == label.track);
        let Some(first) = rows.next() else {
            continue;
        };
        let last = rows.next_back().unwrap_or(first);
        let top = first.span().0;
        let bottom = last.span().1;
        let middle = top + (bottom - top).half();
        items.push(text(
            label.text.clone(),
            Point {
                x: edge,
                y: middle + Units(NAME_EM.0 / 3),
            },
            NAME_EM,
            TextAnchor::End,
        ));
    }
}

pub fn text(text: String, origin: Point, em: Units, anchor: TextAnchor) -> Primitive {
    Primitive::Text(TextRun {
        origin,
        text,
        font: Font::Text,
        em,
        anchor,
    })
}

pub fn ending_label(mask: u8) -> String {
    let numbers: Vec<String> = (0..8)
        .filter(|bit| mask & (1 << bit) != 0)
        .map(|bit| (bit + 1).to_string())
        .collect();
    format!("{}.", numbers.join(", "))
}
