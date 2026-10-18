use opentab_model::Score;

use crate::display::{A4, DEFAULT_STAFF_SPACE, DisplayList, Group, GroupId, Line, Page, Primitive};
use crate::glyphs::Metrics;
use crate::units::{Micrometres, Point, STAFF_SPACE, Units};

pub const MARGIN: Micrometres = Micrometres(15_000);
pub const STANDARD_LINES: i64 = 5;
pub const TAB_LINE_SPACING: Units = Units(STAFF_SPACE.0 * 3 / 2);
pub const STAFF_GAP: Units = Units(STAFF_SPACE.0 * 4);
pub const SYSTEM_GAP: Units = Units(STAFF_SPACE.0 * 8);

struct Staff {
    lines: i64,
    spacing: Units,
}

impl Staff {
    fn height(&self) -> Units {
        Units(self.spacing.0.saturating_mul(self.lines.saturating_sub(1)))
    }
}

struct Placeholder {
    id: GroupId,
    staves: Vec<Staff>,
}

impl Placeholder {
    fn height(&self) -> Units {
        let mut height = Units::ZERO;
        for (index, staff) in self.staves.iter().enumerate() {
            if index > 0 {
                height += STAFF_GAP;
            }
            height += staff.height();
        }
        height
    }
}

pub fn placeholder_layout(score: &Score) -> DisplayList {
    let template = Page::new(A4, DEFAULT_STAFF_SPACE);
    let margin = template.units(MARGIN);
    let left = margin;
    let right = template.width() - margin;
    let bottom = template.height() - margin;
    let thickness = Metrics::bravura().engraving_defaults().staff_line_thickness;
    let mut pages = vec![template.clone()];
    let mut cursor = margin;
    for track in &score.tracks {
        let placeholder = placeholder_for(track.id.0.0, track.tuning.strings.len());
        let height = placeholder.height();
        let page_is_empty = pages.last().is_none_or(|page| page.items.is_empty());
        if !page_is_empty && cursor + height > bottom {
            pages.push(template.clone());
            cursor = margin;
        }
        let group = draw(&placeholder, cursor, left, right, thickness);
        if let Some(page) = pages.last_mut() {
            page.items.push(Primitive::Group(group));
        }
        cursor += height + SYSTEM_GAP;
    }
    DisplayList { pages }
}

fn placeholder_for(id: u64, strings: usize) -> Placeholder {
    let mut staves = vec![Staff {
        lines: STANDARD_LINES,
        spacing: STAFF_SPACE,
    }];
    let strings = i64::try_from(strings).unwrap_or(i64::MAX);
    if strings > 0 {
        staves.push(Staff {
            lines: strings,
            spacing: TAB_LINE_SPACING,
        });
    }
    Placeholder {
        id: GroupId(id),
        staves,
    }
}

fn draw(
    placeholder: &Placeholder,
    top: Units,
    left: Units,
    right: Units,
    thickness: Units,
) -> Group {
    let mut items = Vec::new();
    let mut y = top;
    for (index, staff) in placeholder.staves.iter().enumerate() {
        if index > 0 {
            y += STAFF_GAP;
        }
        for line in 0..staff.lines {
            let line_y = y + Units(staff.spacing.0.saturating_mul(line));
            items.push(Primitive::Line(Line {
                from: Point { x: left, y: line_y },
                to: Point {
                    x: right,
                    y: line_y,
                },
                thickness,
                dash: None,
            }));
        }
        y += staff.height();
    }
    Group {
        id: placeholder.id,
        items,
    }
}
