use crate::display::{Line, PlacedGlyph, Primitive};
use crate::glyphs::{Metrics, REPEAT_DOT};
use crate::structure::StaffLayout;
use crate::units::{Point, Units};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EndSign {
    Normal,
    Double,
    Final,
    Repeat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Strokes {
    pub thin: Units,
    pub thick: Units,
    pub thin_thick_gap: Units,
    pub double_gap: Units,
    pub dot_gap: Units,
    pub dot: Units,
}

pub fn strokes() -> Strokes {
    let metrics = Metrics::bravura();
    let defaults = metrics.engraving_defaults();
    Strokes {
        thin: defaults.thin_barline_thickness,
        thick: defaults.thick_barline_thickness,
        thin_thick_gap: defaults.thin_thick_barline_separation,
        double_gap: defaults.barline_separation,
        dot_gap: defaults.repeat_barline_dot_separation,
        dot: metrics
            .bbox(REPEAT_DOT.name)
            .map_or(Units::ZERO, |rect| rect.width()),
    }
}

impl Strokes {
    pub fn end_width(self, sign: EndSign) -> Units {
        match sign {
            EndSign::Normal => self.thin,
            EndSign::Double => self.thin + self.double_gap + self.thin,
            EndSign::Final => self.thin + self.thin_thick_gap + self.thick,
            EndSign::Repeat => {
                self.dot + self.dot_gap + self.thin + self.thin_thick_gap + self.thick
            }
        }
    }

    pub fn start_repeat_width(self) -> Units {
        self.thick + self.thin_thick_gap + self.thin + self.dot_gap + self.dot
    }

    pub fn shared_start_repeat_width(self) -> Units {
        self.thin_thick_gap + self.thin + self.dot_gap + self.dot
    }

    pub fn draw_end(self, sign: EndSign, right: Units, staff: &StaffLayout) -> Vec<Primitive> {
        let mut items = Vec::new();
        match sign {
            EndSign::Normal => items.push(vertical(right - self.thin.half(), staff, self.thin)),
            EndSign::Double => {
                items.push(vertical(right - self.thin.half(), staff, self.thin));
                let inner = right - self.thin - self.double_gap - self.thin.half();
                items.push(vertical(inner, staff, self.thin));
            }
            EndSign::Final | EndSign::Repeat => {
                items.push(vertical(right - self.thick.half(), staff, self.thick));
                let thin_right = right - self.thick - self.thin_thick_gap;
                items.push(vertical(thin_right - self.thin.half(), staff, self.thin));
                if sign == EndSign::Repeat {
                    dots(
                        thin_right - self.thin - self.dot_gap - self.dot,
                        staff,
                        &mut items,
                    );
                }
            }
        }
        items
    }

    pub fn draw_start_repeat(self, left: Units, staff: &StaffLayout) -> Vec<Primitive> {
        let mut items = vec![vertical(left + self.thick.half(), staff, self.thick)];
        items.extend(self.draw_shared_start_repeat(left + self.thick, staff));
        items
    }

    pub fn draw_shared_start_repeat(self, left: Units, staff: &StaffLayout) -> Vec<Primitive> {
        let thin_left = left + self.thin_thick_gap;
        let mut items = vec![vertical(thin_left + self.thin.half(), staff, self.thin)];
        dots(thin_left + self.thin + self.dot_gap, staff, &mut items);
        items
    }
}

pub fn vertical(x: Units, staff: &StaffLayout, thickness: Units) -> Primitive {
    let (top, bottom) = staff.span();
    Primitive::Line(Line {
        from: Point { x, y: top },
        to: Point { x, y: bottom },
        thickness,
        dash: None,
    })
}

fn dots(x: Units, staff: &StaffLayout, items: &mut Vec<Primitive>) {
    let middle = staff.middle();
    let offset = if staff.lines.is_multiple_of(2) {
        staff.line_spacing
    } else {
        staff.line_spacing.half()
    };
    for y in [middle - offset, middle + offset] {
        items.push(Primitive::Glyph(PlacedGlyph {
            origin: Point { x, y },
            codepoint: REPEAT_DOT.codepoint,
            staff_space: crate::units::STAFF_SPACE,
        }));
    }
}
