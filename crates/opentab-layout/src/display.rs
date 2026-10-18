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

pub const DEFAULT_STAFF_SPACE: Micrometres = Micrometres(1_750);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub size: PageSize,
    pub staff_space: Micrometres,
    pub items: Vec<Primitive>,
}

impl Page {
    pub fn new(size: PageSize, staff_space: Micrometres) -> Page {
        Page {
            size,
            staff_space,
            items: Vec::new(),
        }
    }

    pub fn units(&self, length: Micrometres) -> Units {
        if self.staff_space.0 <= 0 {
            return Units::ZERO;
        }
        Units(narrow(round_div(
            i128::from(length.0) * i128::from(STAFF_SPACE.0),
            i128::from(self.staff_space.0),
        )))
    }

    pub fn width(&self) -> Units {
        self.units(self.size.width)
    }

    pub fn height(&self) -> Units {
        self.units(self.size.height)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DisplayList {
    pub pages: Vec<Page>,
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
    fn a_degenerate_staff_space_gives_zero_units() {
        let page = Page::new(A4, Micrometres(0));
        assert_eq!(page.width(), Units::ZERO);
    }
}
