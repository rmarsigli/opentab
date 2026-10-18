use opentab_layout::{
    A4, DEFAULT_STAFF_SPACE, Line, Micrometres, Page, PageSize, Point, Primitive, Units,
};
use opentab_render::{
    MAX_PIXELS, MAX_TOTAL_PIXELS, PixelSize, Ratio, RenderError, pixel_size, to_png,
};
use resvg::tiny_skia::Pixmap;

const RGBA_BYTES: u64 = 4;

fn a4() -> Page {
    Page::new(A4, DEFAULT_STAFF_SPACE)
}

fn small() -> Page {
    let mut page = Page::new(
        PageSize {
            width: Micrometres(20_000),
            height: Micrometres(10_000),
        },
        DEFAULT_STAFF_SPACE,
    );
    page.items.push(Primitive::Line(Line {
        from: Point::new(0, 2925),
        to: Point::new(11703, 2925),
        thickness: Units(1024),
        dash: None,
    }));
    page
}

fn size(width: u32, height: u32) -> PixelSize {
    PixelSize { width, height }
}

#[test]
fn pixel_size_is_millimetres_times_96_over_25_4_times_scale_rounded_half_up() {
    assert_eq!(
        pixel_size(&a4(), Ratio::from_integer(1)),
        Ok(size(794, 1123))
    );
    assert_eq!(
        pixel_size(&a4(), Ratio::from_integer(2)),
        Ok(size(1587, 2245))
    );
    assert_eq!(pixel_size(&a4(), Ratio::new(3, 2)), Ok(size(1191, 1684)));
    let exact_half = Page::new(
        PageSize {
            width: Micrometres(3_175),
            height: Micrometres(9_525),
        },
        DEFAULT_STAFF_SPACE,
    );
    assert_eq!(pixel_size(&exact_half, Ratio::new(1, 24)), Ok(size(1, 2)));
}

#[test]
fn pixel_size_matches_the_formula_for_many_sizes_and_scales() {
    for millimetres in [1_i64, 7, 50, 148, 210, 216, 279, 297, 420] {
        for (numerator, denominator) in [(1_u32, 1_u32), (2, 1), (3, 2), (1, 2), (5, 4), (7, 3)] {
            let page = Page::new(
                PageSize {
                    width: Micrometres::from_mm(millimetres),
                    height: Micrometres::from_mm(millimetres),
                },
                DEFAULT_STAFF_SPACE,
            );
            let exact_times_two =
                2 * u128::try_from(millimetres).unwrap() * 96_000 * u128::from(numerator);
            let denominator_total = 25_400 * u128::from(denominator);
            let expected = (exact_times_two + denominator_total) / (2 * denominator_total);
            let got = pixel_size(&page, Ratio::new(numerator, denominator)).unwrap();
            assert_eq!(
                u128::from(got.width),
                expected,
                "{millimetres} {numerator}/{denominator}"
            );
        }
    }
}

#[test]
fn zero_and_absurd_scales_are_refused() {
    assert_eq!(
        pixel_size(&a4(), Ratio::new_raw(0, 1)),
        Err(RenderError::ZeroScale)
    );
    assert_eq!(
        to_png(&a4(), Ratio::new_raw(0, 3)),
        Err(RenderError::ZeroScale)
    );
    assert!(matches!(
        to_png(&a4(), Ratio::from_integer(15)),
        Err(RenderError::ImageSize { .. })
    ));
    assert!(matches!(
        pixel_size(&a4(), Ratio::new(1, 100_000)),
        Err(RenderError::ImageSize { .. })
    ));
    let strip = Page::new(
        PageSize {
            width: Micrometres(10_000),
            height: Micrometres(297_000),
        },
        DEFAULT_STAFF_SPACE,
    );
    let largest = Ratio::new(16_384 * 25_400, 297_000 * 96);
    let fitting = pixel_size(&strip, largest).unwrap();
    assert!(fitting.height <= MAX_PIXELS && fitting.height >= MAX_PIXELS - 1);
}

fn sized(width: i64, height: i64) -> Page {
    Page::new(
        PageSize {
            width: Micrometres(width),
            height: Micrometres(height),
        },
        DEFAULT_STAFF_SPACE,
    )
}

#[test]
fn the_total_pixel_count_is_capped_so_the_pixmap_stays_under_256_mebibytes() {
    const { assert!(MAX_TOTAL_PIXELS * RGBA_BYTES <= 256 * 1024 * 1024) };
    let one = Ratio::from_integer(1);
    assert_eq!(
        pixel_size(&sized(2_645_834, 1_587_500), one),
        Ok(size(10_000, 6_000))
    );
    assert_eq!(10_000 * 6_000, MAX_TOTAL_PIXELS);
    assert_eq!(
        pixel_size(&sized(2_645_834, 1_587_765), one),
        Err(RenderError::TooManyPixels {
            width: 10_000,
            height: 6_001,
            pixels: 60_010_000
        })
    );
    assert_eq!(
        pixel_size(&a4(), Ratio::from_integer(8)),
        Ok(size(6_350, 8_980))
    );
    let refused = to_png(&a4(), Ratio::from_integer(9));
    assert!(
        matches!(
            refused,
            Err(RenderError::TooManyPixels {
                width: 7_143,
                height: 10_103,
                ..
            })
        ),
        "{refused:?}"
    );
    let message = refused.unwrap_err().to_string();
    assert!(
        message.contains("7143x10103") && message.contains("60000000"),
        "{message}"
    );
}

#[test]
fn the_png_decodes_with_the_expected_size_for_the_scale() {
    for (scale, expected) in [
        (Ratio::from_integer(1), size(794, 1123)),
        (Ratio::new(3, 2), size(1191, 1684)),
    ] {
        let bytes = to_png(&a4(), scale).unwrap();
        let pixmap = Pixmap::decode_png(&bytes).unwrap();
        assert_eq!(size(pixmap.width(), pixmap.height()), expected);
    }
}

#[test]
fn the_png_shows_white_paper_and_black_ink() {
    let bytes = to_png(&small(), Ratio::from_integer(2)).unwrap();
    let pixmap = Pixmap::decode_png(&bytes).unwrap();
    assert_eq!(size(pixmap.width(), pixmap.height()), size(151, 76));
    let pixel = |x: u32, y: u32| pixmap.pixel(x, y).unwrap();
    let paper = pixel(75, 5);
    assert_eq!(
        (paper.red(), paper.green(), paper.blue(), paper.alpha()),
        (255, 255, 255, 255)
    );
    let ink = pixel(75, 38);
    assert_eq!(
        (ink.red(), ink.green(), ink.blue(), ink.alpha()),
        (0, 0, 0, 255)
    );
}

#[test]
fn the_png_is_deterministic() {
    let page = small();
    assert_eq!(
        to_png(&page, Ratio::from_integer(1)).unwrap(),
        to_png(&page, Ratio::from_integer(1)).unwrap()
    );
}
