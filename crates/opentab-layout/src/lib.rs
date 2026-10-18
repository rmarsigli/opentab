mod barline;
mod decimal;
mod display;
mod draw;
mod engine;
mod furniture;
mod geometry;
pub mod glyphs;
mod grid;
mod options;
mod outline;
mod plan;
mod select;
mod spacing;
mod structure;
pub mod text;
mod units;

pub use barline::EndSign;
pub use decimal::{DecimalError, staff_spaces_to_units};
pub use display::{
    A4, Colour, DEFAULT_STAFF_SPACE, Dash, DisplayList, FilledRect, Font, Group, GroupId, Line,
    Page, PageSize, PathSegment, PathStyle, PlacedGlyph, Primitive, Shape, StaffSpace, TextAnchor,
    TextRun,
};
pub use draw::ending_label;
pub use engine::layout;
pub use furniture::StaffShape;
pub use geometry::{
    BAR_LEAD, COLUMN_GAP, EMPTY_BAR, EMPTY_BAR_MINIMUM, END_PAD, ENDING_RISE, FOOTROOM, GROUP_GAP,
    HEADROOM, MARGIN, MIN_COLUMN_GAP, MIN_STAFF_SPACE, NAME_MAX, STAFF_GAP, STANDARD_LINES,
    TAB_LINE_SPACING,
};
pub use grid::{Grid, GridColumn};
pub use options::{BarRange, LayoutError, LayoutOptions, StaffMode};
pub use plan::{BarInput, Extent, VerticalNeeds};
pub use select::{Selection, StaffSpec};
pub use spacing::{DURATION_WIDTHS, WIDTH_PER_DOUBLING_BEYOND_WHOLE, duration_width};
pub use structure::{
    BarLayout, BeatSlot, ClefSign, Column, GRACE_ADVANCE, GraceSlot, StaffKind, StaffLayout,
    SystemLayout,
};
pub use units::{Micrometres, Point, REFERENCE_EM, Rect, STAFF_SPACE, STAVES_PER_EM, Units};
