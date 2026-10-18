use opentab_model::{Score, ScoreMeta};

use crate::barline::Strokes;
use crate::barline::strokes;
use crate::display::{DisplayList, Page, PageSize, StaffSpace};
use crate::draw::{Context, draw_system, labels};
use crate::geometry::{
    BRACKET_GAP, FIT_ATTEMPTS, FOOTROOM, GROUP_GAP, HEADROOM, MARGIN, MIN_STAFF_SPACE, NAME_GAP,
    NOTATION_CLEARANCE, STAFF_GAP,
};
use crate::glyphs::Metrics;
use crate::lanes::{Band, BandLayout};
use crate::marks::{
    DroppedMark, Frame as BlockFrame, MarkKind, TitleBlock, capo_notes, title_block,
};
use crate::options::{LayoutError, LayoutOptions};
use crate::plan::{BarPlan, SystemPlan, VerticalNeeds, above_share, break_systems, plan_bars};
use crate::select::{StaffSpec, select};
use crate::units::{Micrometres, STAFF_SPACE, Units};
use crate::unrendered::unrendered;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stack {
    pub tops: Vec<Units>,
    pub height: Units,
    pub bands: Vec<BandLayout>,
}

pub struct Frame {
    pub left: Units,
    pub right: Units,
    pub top: Units,
    pub bottom: Units,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Need {
    pub width: Units,
    pub height: Units,
}

pub fn layout(score: &Score, options: &LayoutOptions) -> Result<DisplayList, LayoutError> {
    let requested = StaffSpace::new(options.staff_space).ok_or(LayoutError::StaffSpace {
        micrometres: options.staff_space.0,
    })?;
    check_page(options.page)?;
    let selection = select(score, options)?;
    let strokes = strokes();
    let plans = plan_bars(score, &selection)?;
    let bracket = selection.tracks.len() > 1;
    let bracket_space = if bracket {
        BRACKET_GAP + Metrics::bravura().engraving_defaults().bracket_thickness
    } else {
        Units::ZERO
    };
    let labels = labels(score, &selection);
    let name_indent = labels
        .iter()
        .map(|label| label.name.width())
        .max()
        .filter(|width| *width > Units::ZERO)
        .map_or(Units::ZERO, |width| width + NAME_GAP);
    let widest = plans
        .iter()
        .enumerate()
        .map(|(offset, plan)| {
            let indent = if offset == 0 {
                name_indent
            } else {
                Units::ZERO
            };
            plan.minimum(true, strokes)
                .max(plan.minimum(false, strokes))
                + bracket_space
                + indent
        })
        .max()
        .unwrap_or_default();
    let need = Need {
        width: widest,
        height: Units::ZERO,
    };
    let widest_fit = effective_staff_space(options.page, requested, need)?;
    let capos: Vec<(&str, u8)> = selection
        .tracks
        .iter()
        .filter_map(|index| score.tracks.get(*index))
        .map(|track| (track.name.as_str(), track.capo))
        .collect();
    let notes = capo_notes(&capos);
    let fit = Fit {
        plans: &plans,
        specs: &selection.specs,
        strokes,
        bracket_space,
        name_indent,
        page: options.page,
        meta: &score.meta,
        notes: &notes,
    };
    let staff_space = fit.largest(requested, widest_fit)?;
    let template = Page::new(options.page, staff_space);
    let frame = fit.frame(staff_space);
    let systems = fit.systems(&frame);
    let context = Context {
        score,
        selection: &selection,
        plans: &plans,
        strokes,
        labels: &labels,
        bracket,
    };
    let block = fit.block(staff_space);
    let mut pages: Vec<Page> = Vec::new();
    let mut crowded = Vec::new();
    let mut dropped = Vec::new();
    if block.truncated {
        dropped.push(DroppedMark {
            bar: None,
            kind: MarkKind::Title,
        });
    }
    let mut cursor = frame.bottom;
    if !block.items.is_empty() {
        let mut first = template.clone();
        first.items.extend(block.items);
        pages.push(first);
        cursor = frame.top + block.height;
    }
    for (index, system) in systems.iter().enumerate() {
        let own = stack(
            &selection.specs,
            plans[system.first..system.end]
                .iter()
                .map(|plan| &plan.vertical),
        );
        if own.height > frame.bottom - frame.top {
            return Err(LayoutError::PageTooSmall);
        }
        if pages.is_empty() || cursor + own.height > frame.bottom {
            pages.push(template.clone());
            cursor = frame.top;
        }
        let drawn = draw_system(&context, system, index, cursor, &own)?;
        if drawn.crowded {
            crowded.extend(
                plans[system.first..system.end]
                    .iter()
                    .map(|plan| plan.index),
            );
        }
        dropped.extend(drawn.dropped);
        if let Some(page) = pages.last_mut() {
            page.items.extend(drawn.items);
            page.systems.push(drawn.structure);
        }
        cursor += own.height;
    }
    let title_page = pages.first().is_some_and(|page| page.systems.is_empty());
    let unrendered = unrendered(score, &selection, &pages);
    Ok(DisplayList {
        pages,
        staff_space,
        crowded,
        dropped,
        title_page,
        unrendered,
    })
}

pub struct Fit<'a> {
    pub plans: &'a [BarPlan],
    pub specs: &'a [StaffSpec],
    pub strokes: Strokes,
    pub bracket_space: Units,
    pub name_indent: Units,
    pub page: PageSize,
    pub meta: &'a ScoreMeta,
    pub notes: &'a [String],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Heights {
    pub tallest: Units,
    pub first: Units,
}

impl Fit<'_> {
    pub fn frame(&self, space: StaffSpace) -> Frame {
        let template = Page::new(self.page, space);
        Frame {
            left: template.units(MARGIN),
            right: template.width() - template.units(MARGIN),
            top: template.units(MARGIN),
            bottom: template.height() - template.units(MARGIN),
        }
    }

    pub fn systems(&self, frame: &Frame) -> Vec<SystemPlan> {
        break_systems(
            self.plans,
            self.strokes,
            frame.left + self.bracket_space,
            frame.right,
            self.name_indent,
        )
    }

    pub fn block(&self, space: StaffSpace) -> TitleBlock {
        let frame = self.frame(space);
        title_block(
            self.meta,
            self.notes,
            BlockFrame {
                left: frame.left,
                right: frame.right,
                top: frame.top,
                bottom: frame.bottom,
            },
        )
    }

    pub fn heights(&self, space: StaffSpace) -> Heights {
        let frame = self.frame(space);
        let heights: Vec<Units> = self
            .systems(&frame)
            .iter()
            .map(|system| {
                stack(
                    self.specs,
                    self.plans[system.first..system.end]
                        .iter()
                        .map(|plan| &plan.vertical),
                )
                .height
            })
            .collect();
        Heights {
            tallest: heights.iter().copied().max().unwrap_or_default(),
            first: heights.first().copied().unwrap_or_default(),
        }
    }

    pub fn systems_fit(&self, space: StaffSpace) -> bool {
        let frame = self.frame(space);
        self.heights(space).tallest <= frame.bottom - frame.top
    }

    pub fn fits(&self, space: StaffSpace) -> bool {
        let frame = self.frame(space);
        let room = frame.bottom - frame.top;
        let heights = self.heights(space);
        heights.tallest <= room && self.block(space).height + heights.first <= room
    }

    fn search(&self, low: i64, high: i64, test: &dyn Fn(StaffSpace) -> bool) -> i64 {
        let (mut low, mut high) = (low, high);
        while high - low > 1 {
            let middle = low + (high - low) / 2;
            match StaffSpace::new(Micrometres(middle)) {
                Some(space) if test(space) => low = middle,
                _ => high = middle,
            }
        }
        low
    }

    pub fn largest(
        &self,
        requested: StaffSpace,
        upper: StaffSpace,
    ) -> Result<StaffSpace, LayoutError> {
        if self.fits(upper) {
            return Ok(upper);
        }
        let floor = requested
            .micrometres()
            .min(MIN_STAFF_SPACE)
            .min(upper.micrometres());
        let lowest = StaffSpace::new(floor).ok_or(LayoutError::PageTooSmall)?;
        let high = upper.micrometres().0;
        let found = if self.fits(lowest) {
            self.search(floor.0, high, &|space| self.fits(space))
        } else if self.systems_fit(lowest) {
            self.search(floor.0, high, &|space| self.systems_fit(space))
        } else {
            return Err(LayoutError::PageTooSmall);
        };
        StaffSpace::new(Micrometres(found)).ok_or(LayoutError::PageTooSmall)
    }
}

pub fn check_page(page: PageSize) -> Result<(), LayoutError> {
    if inner(page.width) <= 0 || inner(page.height) <= 0 {
        return Err(LayoutError::PageSize {
            width: page.width.0,
            height: page.height.0,
            margin: MARGIN.0,
        });
    }
    Ok(())
}

fn inner(length: Micrometres) -> i64 {
    length.0.saturating_sub(MARGIN.0.saturating_mul(2))
}

pub fn stack<'a>(specs: &[StaffSpec], needs: impl Iterator<Item = &'a VerticalNeeds>) -> Stack {
    let mut above_system = Units::ZERO;
    let mut below_system = Units::ZERO;
    let mut above_staff = vec![Units::ZERO; specs.len()];
    let mut below_staff = vec![Units::ZERO; specs.len()];
    let mut bands = vec![Band::default(); specs.len()];
    for need in needs {
        above_system = above_system.max(need.above_system);
        below_system = below_system.max(need.below_system);
        for (slot, extra) in above_staff.iter_mut().zip(&need.above_staff) {
            *slot = (*slot).max(*extra);
        }
        for (slot, extra) in below_staff.iter_mut().zip(&need.below_staff) {
            *slot = (*slot).max(*extra);
        }
        for (slot, band) in bands.iter_mut().zip(&need.bands) {
            *slot = slot.merge(*band);
        }
    }
    let bands: Vec<BandLayout> = bands.into_iter().map(BandLayout::of).collect();
    for (row, (slot, band)) in above_staff.iter_mut().zip(&bands).enumerate() {
        if band.reach > Units::ZERO {
            let extra = band.reach + NOTATION_CLEARANCE - above_share(specs, row);
            *slot = (*slot).max(extra);
        }
    }
    let mut tops = Vec::with_capacity(specs.len());
    let mut cursor = HEADROOM + above_system;
    let mut previous: Option<&StaffSpec> = None;
    for ((spec, extra), below) in specs.iter().zip(&above_staff).zip(&below_staff) {
        if let Some(before) = previous {
            cursor += if before.source == spec.source {
                STAFF_GAP
            } else {
                GROUP_GAP
            };
        }
        cursor += *extra;
        tops.push(cursor);
        cursor += spec.shape.height() + *below;
        previous = Some(spec);
    }
    Stack {
        tops,
        height: cursor + FOOTROOM + below_system,
        bands,
    }
}

fn content(length: Micrometres, space: StaffSpace) -> Units {
    space.units(length) - space.units(MARGIN) - space.units(MARGIN)
}

fn fits(page: PageSize, space: StaffSpace, need: Need) -> (bool, bool) {
    (
        content(page.width, space) >= need.width,
        content(page.height, space) >= need.height,
    )
}

fn largest_fitting(length: Micrometres, need: Units) -> Micrometres {
    let room = i128::from(inner(length));
    let divisor = i128::from(need.0.max(1));
    let mut candidate =
        i64::try_from(room * i128::from(STAFF_SPACE.0) / divisor).unwrap_or(i64::MAX);
    for _ in 0..FIT_ATTEMPTS {
        match StaffSpace::new(Micrometres(candidate)) {
            Some(space) if content(length, space) >= need => return Micrometres(candidate),
            Some(_) => candidate -= 1,
            None => return Micrometres(0),
        }
    }
    Micrometres(0)
}

pub fn effective_staff_space(
    page: PageSize,
    requested: StaffSpace,
    need: Need,
) -> Result<StaffSpace, LayoutError> {
    let (wide_enough, tall_enough) = fits(page, requested, need);
    let mut wanted = requested.micrometres();
    if !wide_enough {
        wanted = wanted.min(largest_fitting(page.width, need.width));
    }
    if !tall_enough {
        wanted = wanted.min(largest_fitting(page.height, need.height));
    }
    let floor = requested.micrometres().min(MIN_STAFF_SPACE);
    let chosen = StaffSpace::new(wanted.max(floor)).ok_or(LayoutError::PageTooSmall)?;
    if fits(page, chosen, need).1 {
        Ok(chosen)
    } else {
        Err(LayoutError::PageTooSmall)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::furniture::StaffShape;
    use crate::structure::StaffKind;

    fn spec(source: usize, kind: StaffKind, lines: usize, spacing: Units) -> StaffSpec {
        StaffSpec {
            track: source,
            staff: 0,
            source,
            kind,
            shape: StaffShape { lines, spacing },
        }
    }

    #[test]
    fn each_system_stacks_its_own_vertical_needs() {
        let specs = [
            spec(0, StaffKind::Standard, 5, STAFF_SPACE),
            spec(0, StaffKind::Tab, 6, Units(1536)),
            spec(1, StaffKind::Standard, 5, STAFF_SPACE),
        ];
        let plain = stack(&specs, std::iter::empty());
        assert_eq!(
            plain.tops,
            vec![
                Units(4096),
                Units(4096 + 4096 + 6144),
                Units(14336 + 7680 + 7168)
            ]
        );
        assert_eq!(plain.height, Units(29184 + 4096 + 3072));
        let needs = [
            VerticalNeeds {
                above_system: Units(1000),
                above_staff: vec![Units(0), Units(500), Units(0)],
                below_staff: vec![Units(0), Units(0), Units(0)],
                below_system: Units(0),
                bands: Vec::new(),
            },
            VerticalNeeds {
                above_system: Units(300),
                above_staff: vec![Units(0), Units(200), Units(50)],
                below_staff: vec![Units(0), Units(300), Units(100)],
                below_system: Units(700),
                bands: Vec::new(),
            },
        ];
        let grown = stack(&specs, needs.iter());
        assert_eq!(
            grown.tops,
            vec![
                Units(5096),
                Units(5096 + 4096 + 6144 + 500),
                Units(15836 + 7680 + 300 + 7168 + 50)
            ]
        );
        assert_eq!(grown.height, Units(31034 + 4096 + 100 + 3072 + 700));
    }

    #[test]
    fn the_staff_space_shrinks_only_as_far_as_needed_and_never_below_the_floor() {
        let a4 = crate::display::A4;
        let requested = StaffSpace::new(Micrometres(1_750)).unwrap();
        let small = Need {
            width: Units(10_000),
            height: Units(10_000),
        };
        assert_eq!(effective_staff_space(a4, requested, small), Ok(requested));
        let tall = Need {
            width: Units(10_000),
            height: Units(204_800),
        };
        let shrunk = effective_staff_space(a4, requested, tall).unwrap();
        assert_eq!(shrunk.micrometres(), Micrometres(1_334));
        assert!(content(a4.height, shrunk) >= tall.height);
        let one_more = StaffSpace::new(Micrometres(1_335)).unwrap();
        assert!(content(a4.height, one_more) < tall.height);
        let wide = Need {
            width: Units(204_800),
            height: Units(10_000),
        };
        assert_eq!(
            effective_staff_space(a4, requested, wide)
                .unwrap()
                .micrometres(),
            Micrometres(900)
        );
        let hopeless = Need {
            width: Units(10_000),
            height: Units(400_000),
        };
        assert_eq!(
            effective_staff_space(a4, requested, hopeless),
            Err(LayoutError::PageTooSmall)
        );
    }
}
