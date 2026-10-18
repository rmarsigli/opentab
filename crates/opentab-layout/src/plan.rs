use std::num::NonZeroI128;

use opentab_model::{Bar, MasterBar, Score, TimeSignature};

use crate::barline::{EndSign, Strokes};
use crate::display::GroupId;
use crate::furniture::{HEAD_GAP, HEAD_PAD, KeyDraw, clef_glyph, time_width};
use crate::geometry::{BAR_LEAD, COLUMN_GAP, EMPTY_BAR, END_PAD};
use crate::glyphs::{Metrics, NOTEHEAD_BLACK};
use crate::grid::{Grid, grid};
use crate::options::LayoutError;
use crate::select::{Selection, StaffSpec, model_bar, standard_clef};
use crate::spacing::{Piece, duration_width, natural_width};
use crate::structure::{ClefSign, GRACE_ADVANCE, StaffKind};
use crate::units::{STAFF_SPACE, Units};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub clefs: Vec<Option<ClefSign>>,
    pub key: Option<KeyDraw>,
    pub time: Option<TimeSignature>,
    pub clef_x: Units,
    pub key_x: Units,
    pub time_x: Units,
    pub width: Units,
}

pub struct BarPlan {
    pub index: usize,
    pub number: usize,
    pub grid: Grid,
    pub clefs: Vec<ClefSign>,
    pub start: Head,
    pub inline: Head,
    pub repeat_start: bool,
    pub extents: Vec<(Units, Units)>,
    pub spaces: Vec<Piece>,
    pub end: EndSign,
    pub ending: u8,
    pub repeat_count: Option<u8>,
    pub groups: Vec<GroupId>,
}

impl BarPlan {
    pub fn head(&self, at_start: bool) -> &Head {
        if at_start { &self.start } else { &self.inline }
    }

    pub fn repeat_width(&self, at_start: bool, strokes: Strokes) -> Units {
        if !self.repeat_start {
            return Units::ZERO;
        }
        let gap = if self.head(at_start).width == Units::ZERO {
            strokes.thin_thick_gap
        } else {
            Units::ZERO
        };
        gap + strokes.start_repeat_width()
    }

    pub fn lead(&self) -> Units {
        match self.extents.first() {
            Some((left, _)) => BAR_LEAD + *left,
            None => Units::ZERO,
        }
    }

    pub fn pieces(&self, at_start: bool, strokes: Strokes) -> Vec<Piece> {
        let mut pieces = vec![
            Piece::Fixed(self.head(at_start).width),
            Piece::Fixed(self.repeat_width(at_start, strokes)),
            Piece::Fixed(self.lead()),
        ];
        pieces.extend(self.spaces.iter().copied());
        pieces.push(Piece::Fixed(strokes.end_width(self.end)));
        pieces
    }

    pub fn natural(&self, at_start: bool, strokes: Strokes) -> Units {
        natural_width(&self.pieces(at_start, strokes))
    }
}

pub struct SystemPlan {
    pub first: usize,
    pub end: usize,
    pub natural: Units,
    pub available: Units,
    pub target: Units,
    pub left: Units,
}

pub fn build_head(
    specs: &[StaffSpec],
    clefs: Vec<Option<ClefSign>>,
    key: Option<KeyDraw>,
    time: Option<TimeSignature>,
) -> Head {
    let clef_column = specs
        .iter()
        .zip(&clefs)
        .filter_map(|(spec, clef)| clef.map(|clef| clef_glyph(clef, spec.shape).width()))
        .max();
    let shows_anything = clef_column.is_some() || key.is_some() || time.is_some();
    let mut cursor = if shows_anything {
        HEAD_PAD
    } else {
        Units::ZERO
    };
    let clef_x = cursor;
    if let Some(width) = clef_column {
        cursor += width + HEAD_GAP;
    }
    let key_x = cursor;
    if let Some(draw) = key {
        cursor += draw.width() + HEAD_GAP;
    }
    let time_x = cursor;
    if let Some(time) = time {
        cursor += time_width(time) + HEAD_GAP;
    }
    Head {
        clefs,
        key,
        time,
        clef_x,
        key_x,
        time_x,
        width: cursor,
    }
}

pub fn extents(grid: &Grid) -> Vec<(Units, Units)> {
    let notehead = Metrics::bravura()
        .bbox(NOTEHEAD_BLACK.name)
        .map_or(STAFF_SPACE, |rect| rect.max.x);
    grid.columns
        .iter()
        .map(|column| {
            let graces = i64::try_from(column.graces).unwrap_or(i64::MAX);
            (Units(GRACE_ADVANCE.0.saturating_mul(graces)), notehead)
        })
        .collect()
}

pub fn spaces(grid: &Grid, extents: &[(Units, Units)]) -> Vec<Piece> {
    let Some(per_whole) = NonZeroI128::new(grid.per_whole) else {
        return vec![Piece::Space {
            natural: EMPTY_BAR,
            weight: EMPTY_BAR,
        }];
    };
    if grid.columns.is_empty() {
        return vec![Piece::Space {
            natural: EMPTY_BAR,
            weight: EMPTY_BAR,
        }];
    }
    let mut pieces = Vec::with_capacity(grid.columns.len());
    for (index, column) in grid.columns.iter().enumerate() {
        let right = extents.get(index).map_or(Units::ZERO, |extent| extent.1);
        let (until, collision) = match grid.columns.get(index + 1) {
            Some(next) => {
                let left = extents
                    .get(index + 1)
                    .map_or(Units::ZERO, |extent| extent.0);
                (next.ticks, right + COLUMN_GAP + left)
            }
            None => (grid.span, right + END_PAD),
        };
        let weight = duration_width(until.saturating_sub(column.ticks), per_whole);
        pieces.push(Piece::Space {
            natural: weight.max(collision),
            weight,
        });
    }
    pieces
}

pub fn end_sign(score: &Score, index: usize, master: &MasterBar) -> EndSign {
    if master.repeat_end.is_some() {
        EndSign::Repeat
    } else if index + 1 == score.master_bars.len() {
        EndSign::Final
    } else if master.double_bar {
        EndSign::Double
    } else {
        EndSign::Normal
    }
}

pub fn plan_bars(score: &Score, selection: &Selection) -> Result<Vec<BarPlan>, LayoutError> {
    let mut plans: Vec<BarPlan> = Vec::new();
    let mut previous_clefs: Vec<Option<ClefSign>> = vec![None; selection.specs.len()];
    for index in selection.first..=selection.last {
        let Some(master) = score.master_bars.get(index) else {
            continue;
        };
        let bars: Vec<Option<&Bar>> = selection
            .sources
            .iter()
            .map(|source| model_bar(score, *source, index))
            .collect();
        let pickup = index == 0 && score.anacrusis;
        let grid = grid(&bars, master.time_signature.bar_length(), pickup)
            .ok_or(LayoutError::Rhythm { bar: index })?;
        let clefs: Vec<ClefSign> = selection
            .specs
            .iter()
            .zip(&previous_clefs)
            .map(|(spec, previous)| match spec.kind {
                StaffKind::Tab => ClefSign::Tab,
                StaffKind::Standard => {
                    let track = &score.tracks[spec.track];
                    let bar = bars.get(spec.source).copied().flatten();
                    standard_clef(track, bar, *previous)
                }
            })
            .collect();
        let previous_master = index
            .checked_sub(1)
            .filter(|_| index > selection.first)
            .and_then(|before| score.master_bars.get(before));
        let time_changed =
            previous_master.is_none_or(|before| before.time_signature != master.time_signature);
        let time = time_changed.then_some(master.time_signature);
        let start = build_head(
            &selection.specs,
            clefs.iter().copied().map(Some).collect(),
            KeyDraw::of(master.key_signature),
            time,
        );
        let inline = match previous_master {
            None => start.clone(),
            Some(before) => {
                let changed = clefs
                    .iter()
                    .zip(&previous_clefs)
                    .map(|(clef, previous)| {
                        (*clef != ClefSign::Tab && Some(*clef) != *previous).then_some(*clef)
                    })
                    .collect();
                build_head(
                    &selection.specs,
                    changed,
                    KeyDraw::change(before.key_signature, master.key_signature),
                    time,
                )
            }
        };
        let extents = extents(&grid);
        let spaces = spaces(&grid, &extents);
        let groups = selection
            .sources
            .iter()
            .zip(&bars)
            .map(|(_, bar)| GroupId(bar.map_or(master.id.0.0, |bar| bar.id.0.0)))
            .collect();
        previous_clefs = clefs.iter().copied().map(Some).collect();
        plans.push(BarPlan {
            index,
            number: if score.anacrusis { index } else { index + 1 },
            grid,
            clefs,
            start,
            inline,
            repeat_start: master.repeat_start,
            extents,
            spaces,
            end: end_sign(score, index, master),
            ending: master.alternate_endings,
            repeat_count: master.repeat_end,
            groups,
        });
    }
    Ok(plans)
}

pub fn break_systems(
    plans: &[BarPlan],
    strokes: Strokes,
    left: Units,
    right: Units,
    first_indent: Units,
) -> Vec<SystemPlan> {
    let mut systems = Vec::new();
    let mut first = 0;
    while first < plans.len() {
        let indent = if systems.is_empty() {
            first_indent
        } else {
            Units::ZERO
        };
        let available = right - left - indent;
        let mut natural = plans[first].natural(true, strokes);
        let mut end = first + 1;
        while let Some(next) = plans.get(end) {
            let grown = natural + next.natural(false, strokes);
            if grown > available {
                break;
            }
            natural = grown;
            end += 1;
        }
        systems.push(SystemPlan {
            first,
            end,
            natural,
            available,
            target: available,
            left: left + indent,
        });
        first = end;
    }
    if let Some(last) = systems.last_mut() {
        let relaxed = Units(last.natural.0.saturating_mul(3) / 2);
        last.target = relaxed.min(last.available);
    }
    systems
}
