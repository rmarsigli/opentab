use num_rational::Ratio;
use opentab_layout::{Micrometres, Page};
use resvg::{tiny_skia, usvg};

use crate::svg::to_svg;

pub const PIXELS_PER_INCH: u128 = 96;
pub const MICROMETRES_PER_INCH: u128 = 25_400;
pub const MAX_PIXELS: u32 = 16_384;
pub const MAX_TOTAL_PIXELS: u64 = 60_000_000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenderError {
    #[error("the scale must be greater than zero")]
    ZeroScale,
    #[error(
        "the image would be {width}x{height} pixels; each side must be between 1 and {MAX_PIXELS}"
    )]
    ImageSize { width: u128, height: u128 },
    #[error(
        "the image would be {width}x{height} pixels, {pixels} in all; at most {MAX_TOTAL_PIXELS} pixels are allowed, so lower the scale"
    )]
    TooManyPixels {
        width: u32,
        height: u32,
        pixels: u64,
    },
    #[error("cannot rasterise the page: {0}")]
    Rasterise(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelSize {
    pub width: u32,
    pub height: u32,
}

pub fn pixel_size(page: &Page, scale: Ratio<u32>) -> Result<PixelSize, RenderError> {
    if *scale.numer() == 0 || *scale.denom() == 0 {
        return Err(RenderError::ZeroScale);
    }
    let width = pixels(page.size.width, scale);
    let height = pixels(page.size.height, scale);
    let fits = |side: u128| (1..=u128::from(MAX_PIXELS)).contains(&side);
    if !fits(width) || !fits(height) {
        return Err(RenderError::ImageSize { width, height });
    }
    let (Ok(width), Ok(height)) = (u32::try_from(width), u32::try_from(height)) else {
        return Err(RenderError::ImageSize { width, height });
    };
    let pixels = u64::from(width) * u64::from(height);
    if pixels > MAX_TOTAL_PIXELS {
        return Err(RenderError::TooManyPixels {
            width,
            height,
            pixels,
        });
    }
    Ok(PixelSize { width, height })
}

fn pixels(length: Micrometres, scale: Ratio<u32>) -> u128 {
    let length = u128::try_from(length.0).unwrap_or(0);
    let numerator = length * PIXELS_PER_INCH * u128::from(*scale.numer());
    let denominator = MICROMETRES_PER_INCH * u128::from(*scale.denom());
    (2 * numerator + denominator) / (2 * denominator)
}

pub fn to_png(page: &Page, scale: Ratio<u32>) -> Result<Vec<u8>, RenderError> {
    let size = pixel_size(page, scale)?;
    let svg = to_svg(page);
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default())
        .map_err(|error| RenderError::Rasterise(error.to_string()))?;
    let mut pixmap = tiny_skia::Pixmap::new(size.width, size.height)
        .ok_or_else(|| RenderError::Rasterise("cannot allocate the image".to_string()))?;
    let view = tree.size();
    let transform = tiny_skia::Transform::from_scale(
        size.width as f32 / view.width(),
        size.height as f32 / view.height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    pixmap
        .encode_png()
        .map_err(|error| RenderError::Rasterise(error.to_string()))
}
