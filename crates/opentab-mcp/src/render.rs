use opentab_layout::{LayoutError, StaffMode, layout};
use opentab_model::Score;
use opentab_render::{
    Ratio, RenderError, ScoreSelection, Summary, layout_options, page_number, parse_bars,
    parse_scale, pixel_size, to_png,
};
use serde::Serialize;

use crate::args::{RenderArgs, StaffArg};
use crate::error::ToolError;

pub const DEFAULT_SCALE_NUMERATOR: u32 = 4;
pub const DEFAULT_SCALE_DENOMINATOR: u32 = 3;
pub const DEFAULT_PAGE: u32 = 1;

#[derive(Serialize, Debug)]
pub struct RenderedPage {
    pub page: usize,
    pub width: u32,
    pub height: u32,
    pub scale: String,
    #[serde(flatten)]
    pub summary: Summary,
}

pub struct Picture {
    pub png: Vec<u8>,
    pub report: RenderedPage,
}

pub fn default_scale() -> Ratio<u32> {
    Ratio::new(DEFAULT_SCALE_NUMERATOR, DEFAULT_SCALE_DENOMINATOR)
}

pub fn render(score: &Score, args: RenderArgs) -> Result<Picture, ToolError> {
    let scale = match &args.scale {
        Some(scale) => parse_scale(&scale.text()).map_err(ToolError::usage)?,
        None => default_scale(),
    };
    let bars = match &args.bars {
        Some(bars) => Some(parse_bars(&bars.text()).map_err(ToolError::usage)?),
        None => None,
    };
    let selection = ScoreSelection {
        tracks: args.track.map(|track| track.numbers()).unwrap_or_default(),
        bars,
        staff: args.staff.map(staff_mode),
    };
    let options = layout_options(score, &selection)
        .map_err(|error| ToolError::invalid_input(error.to_string()))?;
    let list = layout(score, &options).map_err(layout_failure)?;
    let summary = Summary::of(&list);
    let number = page_number(args.page.unwrap_or(DEFAULT_PAGE), summary.pages)
        .map_err(|error| ToolError::invalid_input(error.to_string()))?;
    let page = list
        .pages
        .get(number - 1)
        .ok_or_else(|| ToolError::internal("the chosen page is missing from the layout"))?;
    let size = pixel_size(page, scale).map_err(render_failure)?;
    let png = to_png(page, scale).map_err(render_failure)?;
    Ok(Picture {
        png,
        report: RenderedPage {
            page: number,
            width: size.width,
            height: size.height,
            scale: scale.to_string(),
            summary,
        },
    })
}

fn staff_mode(staff: StaffArg) -> StaffMode {
    match staff {
        StaffArg::Standard => StaffMode::Standard,
        StaffArg::Tab => StaffMode::Tab,
        StaffArg::Both => StaffMode::Both,
    }
}

fn layout_failure(error: LayoutError) -> ToolError {
    match error {
        LayoutError::PageTooSmall | LayoutError::Rhythm { .. } => {
            ToolError::new("layout_limit", error.to_string())
        }
        other => ToolError::invalid_input(other.to_string()),
    }
}

fn render_failure(error: RenderError) -> ToolError {
    ToolError::invalid_input(error.to_string())
}
