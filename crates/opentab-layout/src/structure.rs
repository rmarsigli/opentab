use crate::display::GroupId;
use crate::units::{Rect, Units};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StaffKind {
    Standard,
    Tab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClefSign {
    Treble,
    Treble8vb,
    Bass,
    Neutral,
    Tab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GraceSlot {
    pub order: usize,
    pub count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BeatSlot {
    pub voice: usize,
    pub beat: usize,
    pub column: usize,
    pub grace: Option<GraceSlot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Column {
    pub ticks: i128,
    pub x: Units,
    pub left: Units,
    pub right: Units,
    pub graces: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarLayout {
    pub index: usize,
    pub number: usize,
    pub group: GroupId,
    pub rect: Rect,
    pub content: Units,
    pub start_width: Units,
    pub inline_width: Units,
    pub clef: ClefSign,
    pub ticks_per_whole: i128,
    pub span_ticks: i128,
    pub columns: Vec<Column>,
    pub beats: Vec<BeatSlot>,
}

impl BarLayout {
    pub fn x_of(&self, slot: &BeatSlot) -> Option<Units> {
        let column = self.columns.get(slot.column)?;
        Some(match slot.grace {
            Some(grace) => {
                let before =
                    i64::try_from(grace.count.saturating_sub(grace.order)).unwrap_or(i64::MAX);
                column.x - Units(GRACE_ADVANCE.0.saturating_mul(before))
            }
            None => column.x,
        })
    }
}

pub const GRACE_ADVANCE: Units = Units(1024);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaffLayout {
    pub track: usize,
    pub staff: usize,
    pub kind: StaffKind,
    pub top: Units,
    pub lines: usize,
    pub line_spacing: Units,
    pub bars: Vec<BarLayout>,
}

impl StaffLayout {
    pub fn line_y(&self, line: usize) -> Units {
        let line = i64::try_from(line).unwrap_or(i64::MAX);
        self.top + Units(self.line_spacing.0.saturating_mul(line))
    }

    pub fn bottom(&self) -> Units {
        self.line_y(self.lines.saturating_sub(1))
    }

    pub fn middle(&self) -> Units {
        self.top + (self.bottom() - self.top).half()
    }

    pub fn span(&self) -> (Units, Units) {
        if self.lines > 1 {
            (self.top, self.bottom())
        } else {
            let half = self.line_spacing.half();
            (self.top - half, self.top + half)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemLayout {
    pub index: usize,
    pub rect: Rect,
    pub natural: Units,
    pub available: Units,
    pub staves: Vec<StaffLayout>,
}
