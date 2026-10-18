use opentab_model::Score;

use crate::barline::strokes;
use crate::display::{DisplayList, Page, PageSize, StaffSpace};
use crate::draw::{Context, draw_system, labels};
use crate::geometry::{
    BRACKET_GAP, FIT_ATTEMPTS, FOOTROOM, GROUP_GAP, HEADROOM, MARGIN, MIN_STAFF_SPACE, NAME_EM,
    NAME_GAP, STAFF_GAP,
};
use crate::glyphs::Metrics;
use crate::options::{LayoutError, LayoutOptions};
use crate::plan::{break_systems, plan_bars};
use crate::select::{StaffSpec, select};
use crate::text::text_font;
use crate::units::{Micrometres, STAFF_SPACE, Units};

pub struct Stack {
    pub tops: Vec<Units>,
    pub height: Units,
}

pub struct Frame {
    pub left: Units,
    pub right: Units,
    pub top: Units,
    pub bottom: Units,
}

pub fn layout(score: &Score, options: &LayoutOptions) -> Result<DisplayList, LayoutError> {
    let requested = StaffSpace::new(options.staff_space).ok_or(LayoutError::StaffSpace {
        micrometres: options.staff_space.0,
    })?;
    check_page(options.page)?;
    let selection = select(score, options)?;
    let strokes = strokes();
    let plans = plan_bars(score, &selection)?;
    let stack = stack(&selection.specs);
    let staff_space = fit_staff_space(options.page, requested, stack.height)?;
    let template = Page::new(options.page, staff_space);
    let frame = Frame {
        left: template.units(MARGIN),
        right: template.width() - template.units(MARGIN),
        top: template.units(MARGIN),
        bottom: template.height() - template.units(MARGIN),
    };
    let bracket = selection.tracks.len() > 1;
    let bracket_space = if bracket {
        BRACKET_GAP + Metrics::bravura().engraving_defaults().bracket_thickness
    } else {
        Units::ZERO
    };
    let content = frame.right - frame.left;
    let labels = labels(score, &selection, content);
    let name_indent = labels
        .iter()
        .map(|label| text_font().measure(&label.text, NAME_EM))
        .max()
        .filter(|width| *width > Units::ZERO)
        .map_or(Units::ZERO, |width| width + NAME_GAP);
    let systems = break_systems(
        &plans,
        strokes,
        frame.left + bracket_space,
        frame.right,
        name_indent,
    );
    let context = Context {
        score,
        selection: &selection,
        plans: &plans,
        stack: &stack,
        strokes,
        labels: &labels,
        bracket,
    };
    let mut pages: Vec<Page> = Vec::new();
    let mut cursor = frame.bottom;
    for (index, system) in systems.iter().enumerate() {
        if pages.is_empty() || cursor + stack.height > frame.bottom {
            pages.push(template.clone());
            cursor = frame.top;
        }
        let (structure, items) = draw_system(&context, system, index, cursor)?;
        if let Some(page) = pages.last_mut() {
            page.items.extend(items);
            page.systems.push(structure);
        }
        cursor += stack.height;
    }
    Ok(DisplayList { pages })
}

pub fn check_page(page: PageSize) -> Result<(), LayoutError> {
    let inner = |length: Micrometres| length.0.saturating_sub(MARGIN.0.saturating_mul(2));
    if inner(page.width) <= 0 || inner(page.height) <= 0 {
        return Err(LayoutError::PageSize {
            width: page.width.0,
            height: page.height.0,
            margin: MARGIN.0,
        });
    }
    Ok(())
}

pub fn stack(specs: &[StaffSpec]) -> Stack {
    let mut tops = Vec::with_capacity(specs.len());
    let mut cursor = HEADROOM;
    let mut previous: Option<&StaffSpec> = None;
    for spec in specs {
        if let Some(before) = previous {
            cursor += if before.source == spec.source {
                STAFF_GAP
            } else {
                GROUP_GAP
            };
        }
        tops.push(cursor);
        cursor += spec.shape.height();
        previous = Some(spec);
    }
    Stack {
        tops,
        height: cursor + FOOTROOM,
    }
}

pub fn content_height(page: PageSize, space: StaffSpace) -> Units {
    space.units(page.height) - space.units(MARGIN) - space.units(MARGIN)
}

pub fn fit_staff_space(
    page: PageSize,
    requested: StaffSpace,
    height: Units,
) -> Result<StaffSpace, LayoutError> {
    if content_height(page, requested) >= height {
        return Ok(requested);
    }
    let inner = i128::from(page.height.0) - 2 * i128::from(MARGIN.0);
    let divisor = i128::from(height.0.max(1));
    let mut candidate =
        i64::try_from(inner * i128::from(STAFF_SPACE.0) / divisor).unwrap_or(i64::MAX);
    for _ in 0..FIT_ATTEMPTS {
        if candidate < MIN_STAFF_SPACE.0 {
            break;
        }
        if let Some(space) = StaffSpace::new(Micrometres(candidate))
            && content_height(page, space) >= height
        {
            return Ok(space);
        }
        candidate -= 1;
    }
    Err(LayoutError::PageTooSmall)
}
