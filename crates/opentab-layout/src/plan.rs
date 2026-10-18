use std::num::NonZeroI128;

use opentab_model::{Bar, KeySignature, MasterBar, Score, TimeSignature};

use crate::barline::{EndSign, Strokes};
use crate::display::GroupId;
use crate::furniture::{HEAD_GAP, HEAD_PAD, KeyDraw, clef_glyph, time_width};
use crate::geometry::{
    BAR_LEAD, COLUMN_GAP, EMPTY_BAR, EMPTY_BAR_MINIMUM, END_PAD, FOOTROOM, GROUP_GAP, HEADROOM,
    MIN_COLUMN_GAP, NOTATION_CLEARANCE, STAFF_GAP,
};
use crate::glyphs::{Metrics, NOTEHEAD_BLACK};
use crate::grid::{Grid, grid};
use crate::lanes::Band;
use crate::notation::{ColumnNeed, RowInput, StaffNotation, analyse};
use crate::options::LayoutError;
use crate::pitch::written_key;
use crate::select::{Selection, StaffSpec, model_bar, standard_clef};
use crate::spacing::{Piece, duration_width, minimum_width, natural_width};
use crate::structure::{ClefSign, GRACE_ADVANCE, StaffKind};
use crate::tab::{TabInput, TabNotation, analyse_tab};
use crate::units::{STAFF_SPACE, Units};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub clefs: Vec<Option<ClefSign>>,
    pub keys: Vec<Option<KeyDraw>>,
    pub time: Option<TimeSignature>,
    pub clef_x: Units,
    pub key_x: Units,
    pub time_x: Units,
    pub width: Units,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Extent {
    pub left: Units,
    pub right: Units,
    pub lead: Units,
    pub grace_advance: Units,
    pub grace_lead: Units,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VerticalNeeds {
    pub above_system: Units,
    pub above_staff: Vec<Units>,
    pub below_staff: Vec<Units>,
    pub below_system: Units,
    pub bands: Vec<Band>,
}

pub struct BarInput<'a> {
    pub score: &'a Score,
    pub selection: &'a Selection,
    pub index: usize,
    pub master: &'a MasterBar,
    pub bars: &'a [Option<&'a Bar>],
    pub clefs: &'a [ClefSign],
    pub key: KeySignature,
    pub grid: &'a Grid,
    pub notation: &'a [Option<StaffNotation>],
    pub tab: &'a [Option<TabNotation>],
}

pub struct BarPlan {
    pub index: usize,
    pub number: usize,
    pub grid: Grid,
    pub clefs: Vec<ClefSign>,
    pub start: Head,
    pub inline: Head,
    pub repeat_start: bool,
    pub shares_thick: bool,
    pub extents: Vec<Extent>,
    pub spaces: Vec<Piece>,
    pub end: EndSign,
    pub ending: u8,
    pub repeat_count: Option<u8>,
    pub groups: Vec<GroupId>,
    pub vertical: VerticalNeeds,
    pub notation: Vec<Option<StaffNotation>>,
    pub tab: Vec<Option<TabNotation>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slots {
    pub head: usize,
    pub repeat: usize,
    pub lead: usize,
    pub columns: usize,
    pub end: usize,
    pub right: usize,
}

impl BarPlan {
    pub fn head(&self, at_start: bool) -> &Head {
        if at_start { &self.start } else { &self.inline }
    }

    pub fn repeat_width(&self, at_start: bool, strokes: Strokes) -> Units {
        if !self.repeat_start {
            Units::ZERO
        } else if !at_start && self.shares_thick {
            strokes.shared_start_repeat_width()
        } else {
            strokes.start_repeat_width()
        }
    }

    pub fn lead(&self) -> Units {
        match self.extents.first() {
            Some(extent) => BAR_LEAD + extent.left,
            None => Units::ZERO,
        }
    }

    pub fn slots(&self, at_start: bool) -> Slots {
        let (head, repeat) = if at_start { (0, 1) } else { (1, 0) };
        Slots {
            head,
            repeat,
            lead: 2,
            columns: 3,
            end: 3 + self.spaces.len(),
            right: 4 + self.spaces.len(),
        }
    }

    pub fn pieces(&self, at_start: bool, strokes: Strokes) -> Vec<Piece> {
        let head = Piece::Fixed(self.head(at_start).width);
        let repeat = Piece::Fixed(self.repeat_width(at_start, strokes));
        let mut pieces = if at_start {
            vec![head, repeat]
        } else {
            vec![repeat, head]
        };
        pieces.push(Piece::Fixed(self.lead()));
        pieces.extend(self.spaces.iter().copied());
        pieces.push(Piece::Fixed(strokes.end_width(self.end)));
        pieces
    }

    pub fn contained_pieces(&self, at_start: bool, strokes: Strokes) -> Vec<Piece> {
        let mut pieces = self.pieces(at_start, strokes);
        if self.extents.is_empty() {
            return pieces;
        }
        let slots = self.slots(at_start);
        let widest_left = self
            .extents
            .iter()
            .map(|extent| extent.left)
            .max()
            .unwrap_or_default();
        let widest_right = self
            .extents
            .iter()
            .map(|extent| extent.right)
            .max()
            .unwrap_or_default();
        if let Some(lead) = pieces.get_mut(slots.lead) {
            *lead = Piece::Fixed(BAR_LEAD + widest_left);
        }
        if let Some(tail) = slots
            .end
            .checked_sub(1)
            .and_then(|index| pieces.get_mut(index))
        {
            *tail = Piece::Fixed(widest_right + MIN_COLUMN_GAP);
        }
        pieces
    }

    pub fn natural(&self, at_start: bool, strokes: Strokes) -> Units {
        natural_width(&self.pieces(at_start, strokes))
    }

    pub fn minimum(&self, at_start: bool, strokes: Strokes) -> Units {
        minimum_width(&self.pieces(at_start, strokes))
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
    keys: Vec<Option<KeyDraw>>,
    time: Option<TimeSignature>,
) -> Head {
    let key_width = keys.iter().flatten().map(|draw| draw.width()).max();
    let clef_column = specs
        .iter()
        .zip(&clefs)
        .filter_map(|(spec, clef)| clef.map(|clef| clef_glyph(clef, spec.shape).width()))
        .max();
    let shows_anything = clef_column.is_some() || key_width.is_some() || time.is_some();
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
    if let Some(width) = key_width {
        cursor += width + HEAD_GAP;
    }
    let time_x = cursor;
    if let Some(time) = time {
        cursor += time_width(time) + HEAD_GAP;
    }
    Head {
        clefs,
        keys,
        time,
        clef_x,
        key_x,
        time_x,
        width: cursor,
    }
}

pub fn notehead_width() -> Units {
    Metrics::bravura()
        .bbox(NOTEHEAD_BLACK.name)
        .map_or(STAFF_SPACE, |rect| rect.max.x)
}

pub fn extents(input: &BarInput<'_>) -> Vec<Extent> {
    let notehead = notehead_width();
    input
        .grid
        .columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let mut need = ColumnNeed {
                right: notehead,
                ..ColumnNeed::default()
            };
            let mut graced = false;
            for staff in input
                .notation
                .iter()
                .flatten()
                .map(|staff| &staff.columns)
                .chain(input.tab.iter().flatten().map(|tab| &tab.columns))
            {
                if let Some(found) = staff.get(index) {
                    need.lead = need.lead.max(found.lead);
                    need.right = need.right.max(found.right);
                    need.grace_lead = need.grace_lead.max(found.grace_lead);
                    need.grace_right = need.grace_right.max(found.grace_right);
                    graced |= found.grace_right > Units::ZERO;
                }
            }
            let grace_advance = if graced {
                need.grace_advance().max(GRACE_ADVANCE)
            } else {
                GRACE_ADVANCE
            };
            let graces = i64::try_from(column.graces).unwrap_or(i64::MAX);
            Extent {
                left: need.lead + Units(grace_advance.0.saturating_mul(graces)),
                right: need.right,
                lead: need.lead,
                grace_advance,
                grace_lead: need.grace_lead,
            }
        })
        .collect()
}

pub fn gap_between(specs: &[StaffSpec], upper: usize) -> Option<Units> {
    let first = specs.get(upper)?;
    let second = specs.get(upper + 1)?;
    Some(if first.source == second.source {
        STAFF_GAP
    } else {
        GROUP_GAP
    })
}

pub fn above_share(specs: &[StaffSpec], row: usize) -> Units {
    match row.checked_sub(1) {
        Some(upper) => gap_between(specs, upper).unwrap_or(GROUP_GAP).half(),
        None => HEADROOM,
    }
}

pub fn below_share(specs: &[StaffSpec], row: usize) -> Units {
    gap_between(specs, row).map_or(FOOTROOM, Units::half)
}

pub fn vertical_needs(input: &BarInput<'_>) -> VerticalNeeds {
    let specs = &input.selection.specs;
    let mut needs = VerticalNeeds {
        above_system: Units::ZERO,
        above_staff: vec![Units::ZERO; specs.len()],
        below_staff: vec![Units::ZERO; specs.len()],
        below_system: Units::ZERO,
        bands: vec![Band::default(); specs.len()],
    };
    for (row, spec) in specs.iter().enumerate() {
        if let Some(Some(tab)) = input.tab.get(row) {
            needs.bands[row] = tab.band;
            let below = tab.bottom - spec.shape.height() + NOTATION_CLEARANCE;
            needs.below_staff[row] = (below - below_share(specs, row)).max(Units::ZERO);
        }
        let Some(Some(notation)) = input.notation.get(row) else {
            continue;
        };
        let above_share = above_share(specs, row);
        let below_share = below_share(specs, row);
        let above = -notation.top + NOTATION_CLEARANCE;
        let below = notation.bottom - spec.shape.height() + NOTATION_CLEARANCE;
        needs.above_staff[row] = (above - above_share).max(Units::ZERO);
        needs.below_staff[row] = (below - below_share).max(Units::ZERO);
    }
    needs
}

pub fn staff_keys(
    score: &Score,
    selection: &Selection,
    key: KeySignature,
) -> Vec<Option<KeySignature>> {
    selection
        .specs
        .iter()
        .map(|spec| match spec.kind {
            StaffKind::Tab => None,
            StaffKind::Standard => {
                let chromatic = score
                    .tracks
                    .get(spec.track)
                    .map_or(0, |track| track.transpose.chromatic());
                Some(written_key(key, chromatic))
            }
        })
        .collect()
}

pub fn notation(
    score: &Score,
    selection: &Selection,
    master: &MasterBar,
    bars: &[Option<&Bar>],
    clefs: &[ClefSign],
    keys: &[Option<KeySignature>],
    grid: &Grid,
) -> Vec<Option<StaffNotation>> {
    selection
        .specs
        .iter()
        .zip(clefs)
        .zip(keys)
        .map(|((spec, clef), key)| {
            if spec.kind != StaffKind::Standard {
                return None;
            }
            let track = score.tracks.get(spec.track)?;
            analyse(&RowInput {
                track,
                bar: bars.get(spec.source).copied().flatten(),
                slots: grid.slots.get(spec.source).map_or(&[], Vec::as_slice),
                columns: &grid.columns,
                per_whole: grid.per_whole,
                clef: *clef,
                key: key.unwrap_or(master.key_signature),
                time: master.time_signature,
            })
        })
        .collect()
}

pub fn tab_notation(
    score: &Score,
    selection: &Selection,
    master: &MasterBar,
    index: usize,
    bars: &[Option<&Bar>],
    grid: &Grid,
) -> Vec<Option<TabNotation>> {
    selection
        .specs
        .iter()
        .map(|spec| {
            if spec.kind != StaffKind::Tab {
                return None;
            }
            let track = score.tracks.get(spec.track)?;
            let source = selection.sources.get(spec.source).copied()?;
            let alone = !selection
                .specs
                .iter()
                .any(|other| other.source == spec.source && other.kind == StaffKind::Standard);
            analyse_tab(&TabInput {
                track,
                bar: bars.get(spec.source).copied().flatten(),
                previous: index
                    .checked_sub(1)
                    .and_then(|before| model_bar(score, source, before)),
                next: model_bar(score, source, index + 1),
                slots: grid.slots.get(spec.source).map_or(&[], Vec::as_slice),
                columns: &grid.columns,
                per_whole: grid.per_whole,
                time: master.time_signature,
                lines: spec.shape.lines,
                spacing: spec.shape.spacing,
                alone,
            })
        })
        .collect()
}

pub fn spaces(grid: &Grid, extents: &[Extent]) -> Vec<Piece> {
    let empty = vec![Piece::Space {
        natural: EMPTY_BAR,
        minimum: EMPTY_BAR_MINIMUM,
        weight: EMPTY_BAR,
    }];
    let Some(per_whole) = NonZeroI128::new(grid.per_whole) else {
        return empty;
    };
    if grid.columns.is_empty() {
        return empty;
    }
    let mut pieces = Vec::with_capacity(grid.columns.len());
    for (index, column) in grid.columns.iter().enumerate() {
        let right = extents
            .get(index)
            .map_or(Units::ZERO, |extent| extent.right);
        let (until, comfortable, minimum) = match grid.columns.get(index + 1) {
            Some(next) => {
                let left = extents
                    .get(index + 1)
                    .map_or(Units::ZERO, |extent| extent.left);
                (
                    next.ticks,
                    right + COLUMN_GAP + left,
                    right + MIN_COLUMN_GAP + left,
                )
            }
            None => (grid.span, right + END_PAD, right + MIN_COLUMN_GAP),
        };
        let weight = duration_width(until.saturating_sub(column.ticks), per_whole);
        pieces.push(Piece::Space {
            natural: weight.max(comfortable),
            minimum,
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

fn largest_id(score: &Score) -> u64 {
    let mut largest = score.ids.peek().0;
    let mut see = |id: u64| largest = largest.max(id.saturating_add(1));
    for master in &score.master_bars {
        see(master.id.0.0);
    }
    for track in &score.tracks {
        see(track.id.0.0);
        for staff in &track.staves {
            see(staff.id.0.0);
            for bar in &staff.bars {
                see(bar.id.0.0);
                for voice in &bar.voices {
                    see(voice.id.0.0);
                    for beat in &voice.beats {
                        see(beat.id.0.0);
                        for note in &beat.notes {
                            see(note.id.0.0);
                        }
                    }
                }
            }
        }
    }
    largest
}

pub fn plan_bars(score: &Score, selection: &Selection) -> Result<Vec<BarPlan>, LayoutError> {
    let mut plans: Vec<BarPlan> = Vec::new();
    let mut previous_clefs: Vec<Option<ClefSign>> = vec![None; selection.specs.len()];
    let mut synthetic = largest_id(score);
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
        let keys = staff_keys(score, selection, master.key_signature);
        let previous_keys =
            previous_master.map(|before| staff_keys(score, selection, before.key_signature));
        let start_keys = keys
            .iter()
            .enumerate()
            .map(|(row, key)| {
                let key = (*key)?;
                let before = previous_keys.as_ref().and_then(|known| known[row]);
                match (before, KeyDraw::of(key)) {
                    (Some(before), None) => KeyDraw::change(before, key),
                    (_, draw) => draw,
                }
            })
            .collect();
        let start = build_head(
            &selection.specs,
            clefs.iter().copied().map(Some).collect(),
            start_keys,
            time,
        );
        let inline = match previous_master {
            None => start.clone(),
            Some(_) => {
                let changed = clefs
                    .iter()
                    .zip(&previous_clefs)
                    .map(|(clef, previous)| {
                        (*clef != ClefSign::Tab && Some(*clef) != *previous).then_some(*clef)
                    })
                    .collect();
                let inline_keys = keys
                    .iter()
                    .enumerate()
                    .map(|(row, key)| {
                        let before = previous_keys.as_ref().and_then(|known| known[row])?;
                        KeyDraw::change(before, (*key)?)
                    })
                    .collect();
                build_head(&selection.specs, changed, inline_keys, time)
            }
        };
        let notation = notation(score, selection, master, &bars, &clefs, &keys, &grid);
        let tab = tab_notation(score, selection, master, index, &bars, &grid);
        let input = BarInput {
            score,
            selection,
            index,
            master,
            bars: &bars,
            clefs: &clefs,
            key: master.key_signature,
            grid: &grid,
            notation: &notation,
            tab: &tab,
        };
        let extents = extents(&input);
        let vertical = vertical_needs(&input);
        let spaces = spaces(&grid, &extents);
        let mut groups = Vec::with_capacity(bars.len());
        for bar in &bars {
            groups.push(GroupId(match bar {
                Some(bar) => bar.id.0.0,
                None => {
                    synthetic = synthetic.saturating_add(1);
                    synthetic
                }
            }));
        }
        let shares_thick = plans
            .last()
            .is_some_and(|before| before.end == EndSign::Repeat);
        previous_clefs = clefs.iter().copied().map(Some).collect();
        plans.push(BarPlan {
            index,
            number: if score.anacrusis { index } else { index + 1 },
            grid,
            clefs,
            start,
            inline,
            repeat_start: master.repeat_start,
            shares_thick,
            extents,
            spaces,
            end: end_sign(score, index, master),
            ending: master.alternate_endings,
            repeat_count: master.repeat_end,
            groups,
            vertical,
            notation,
            tab,
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
