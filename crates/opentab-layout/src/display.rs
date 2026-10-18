use std::num::{NonZeroI64, NonZeroI128};

use crate::marks::DroppedMark;
use crate::structure::SystemLayout;
use crate::units::{Micrometres, Point, Rect, STAFF_SPACE, Units, narrow, round_div};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageSize {
    pub width: Micrometres,
    pub height: Micrometres,
}

pub const A4: PageSize = PageSize {
    width: Micrometres(210_000),
    height: Micrometres(297_000),
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StaffSpace(NonZeroI64);

impl StaffSpace {
    pub const fn new(length: Micrometres) -> Option<StaffSpace> {
        match NonZeroI64::new(length.0) {
            Some(value) if value.get() > 0 => Some(StaffSpace(value)),
            _ => None,
        }
    }

    pub fn micrometres(self) -> Micrometres {
        Micrometres(self.0.get())
    }

    pub fn units(self, length: Micrometres) -> Units {
        Units(narrow(round_div(
            i128::from(length.0) * i128::from(STAFF_SPACE.0),
            NonZeroI128::from(self.0),
        )))
    }
}

pub const DEFAULT_STAFF_SPACE: StaffSpace = StaffSpace::new(Micrometres(1_750)).unwrap();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub size: PageSize,
    pub staff_space: StaffSpace,
    pub items: Vec<Primitive>,
    pub systems: Vec<SystemLayout>,
}

impl Page {
    pub fn new(size: PageSize, staff_space: StaffSpace) -> Page {
        Page {
            size,
            staff_space,
            items: Vec::new(),
            systems: Vec::new(),
        }
    }

    pub fn units(&self, length: Micrometres) -> Units {
        self.staff_space.units(length)
    }

    pub fn width(&self) -> Units {
        self.units(self.size.width)
    }

    pub fn height(&self) -> Units {
        self.units(self.size.height)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayList {
    pub pages: Vec<Page>,
    pub staff_space: StaffSpace,
    pub crowded: Vec<usize>,
    pub dropped: Vec<DroppedMark>,
    pub title_page: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Primitive {
    Glyph(PlacedGlyph),
    Line(Line),
    Shape(Shape),
    Text(TextRun),
    FilledRect(FilledRect),
    Group(Group),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacedGlyph {
    pub origin: Point,
    pub codepoint: char,
    pub staff_space: Units,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dash {
    pub on: Units,
    pub off: Units,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Line {
    pub from: Point,
    pub to: Point,
    pub thickness: Units,
    pub dash: Option<Dash>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathSegment {
    MoveTo(Point),
    LineTo(Point),
    QuadTo(Point, Point),
    CubicTo(Point, Point, Point),
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathStyle {
    Fill,
    Stroke(Units),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shape {
    pub segments: Vec<PathSegment>,
    pub style: PathStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Font {
    Text,
    Music,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAnchor {
    Start,
    Middle,
    End,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextRun {
    pub origin: Point,
    pub text: String,
    pub font: Font,
    pub em: Units,
    pub anchor: TextAnchor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Colour {
    Black,
    White,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilledRect {
    pub rect: Rect,
    pub colour: Colour,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GroupId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub id: GroupId,
    pub items: Vec<Primitive>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_a4_page_at_the_default_staff_space_has_fixed_unit_extents() {
        let page = Page::new(A4, DEFAULT_STAFF_SPACE);
        assert_eq!(page.width(), Units(122_880));
        assert_eq!(page.height(), Units(173_787));
        assert_eq!(page.units(Micrometres::from_mm(15)), Units(8_777));
    }

    #[test]
    fn a_non_positive_staff_space_is_unrepresentable() {
        assert_eq!(StaffSpace::new(Micrometres(0)), None);
        assert_eq!(StaffSpace::new(Micrometres(-1)), None);
        let one = StaffSpace::new(Micrometres(1)).unwrap();
        assert_eq!(one.micrometres(), Micrometres(1));
        assert_eq!(one.units(Micrometres(3)), Units(3072));
        assert_eq!(DEFAULT_STAFF_SPACE.micrometres(), Micrometres(1_750));
    }
}
