use crate::display::{A4, DEFAULT_STAFF_SPACE, PageSize};
use crate::units::Micrometres;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StaffMode {
    Standard,
    Tab,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BarRange {
    pub first: usize,
    pub last: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutOptions {
    pub tracks: Option<Vec<usize>>,
    pub bars: Option<BarRange>,
    pub staff_mode: Option<StaffMode>,
    pub page: PageSize,
    pub staff_space: Micrometres,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        LayoutOptions {
            tracks: None,
            bars: None,
            staff_mode: None,
            page: A4,
            staff_space: DEFAULT_STAFF_SPACE.micrometres(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LayoutError {
    #[error("the staff space must be positive, got {micrometres} micrometres")]
    StaffSpace { micrometres: i64 },
    #[error(
        "a {width}x{height} micrometre page leaves no room inside its {margin} micrometre margins"
    )]
    PageSize {
        width: i64,
        height: i64,
        margin: i64,
    },
    #[error("the page is too small to hold one system of this score")]
    PageTooSmall,
    #[error("track index {index} does not exist; the score has {tracks} track(s)")]
    Track { index: usize, tracks: usize },
    #[error("bar indices {first}..={last} are not within the score's {bars} bar(s)")]
    Bars {
        first: usize,
        last: usize,
        bars: usize,
    },
    #[error("the score has no bars to draw")]
    NoBars,
    #[error("the selected tracks have no staves to draw")]
    NoStaves,
    #[error("the rhythm of bar index {bar} is too complex to lay out exactly")]
    Rhythm { bar: usize },
}
