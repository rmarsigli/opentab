use opentab_layout::glyphs::NOTEHEAD_BLACK;
use opentab_layout::{
    Colour, DEFAULT_STAFF_SPACE, Dash, FilledRect, Font, Group, GroupId, Line, Micrometres, Page,
    PageSize, PathSegment, PathStyle, PlacedGlyph, Point, Primitive, Rect, STAFF_SPACE, Shape,
    TextAnchor, TextRun, Units,
};
use opentab_render::to_svg;

fn sample_page() -> Page {
    let size = PageSize {
        width: Micrometres(20_000),
        height: Micrometres(10_500),
    };
    let mut page = Page::new(size, DEFAULT_STAFF_SPACE);
    page.items = vec![
        Primitive::FilledRect(FilledRect {
            rect: Rect::new(Point::new(100, 200), Point::new(600, 900)),
            colour: Colour::Black,
        }),
        Primitive::Glyph(PlacedGlyph {
            origin: Point::new(2048, 3072),
            codepoint: NOTEHEAD_BLACK.codepoint,
            staff_space: STAFF_SPACE,
        }),
        Primitive::Line(Line {
            from: Point::new(0, 1024),
            to: Point::new(8192, 1024),
            thickness: Units(133),
            dash: Some(Dash {
                on: Units(512),
                off: Units(256),
            }),
        }),
        Primitive::Shape(Shape {
            segments: vec![
                PathSegment::MoveTo(Point::new(1000, 4000)),
                PathSegment::QuadTo(Point::new(1500, 3500), Point::new(2000, 4000)),
                PathSegment::Close,
            ],
            style: PathStyle::Fill,
        }),
        Primitive::Shape(Shape {
            segments: vec![
                PathSegment::MoveTo(Point::new(3000, 4000)),
                PathSegment::CubicTo(
                    Point::new(3200, 3600),
                    Point::new(3800, 3600),
                    Point::new(4000, 4000),
                ),
            ],
            style: PathStyle::Stroke(Units(160)),
        }),
        Primitive::Text(TextRun {
            origin: Point::new(6000, 5000),
            text: "T1".to_string(),
            font: Font::Text,
            em: Units(1024),
            anchor: TextAnchor::Middle,
        }),
        Primitive::Group(Group {
            id: GroupId(42),
            items: vec![
                Primitive::FilledRect(FilledRect {
                    rect: Rect::new(Point::new(7000, 5000), Point::new(7500, 5600)),
                    colour: Colour::White,
                }),
                Primitive::Line(Line {
                    from: Point::new(7000, 5300),
                    to: Point::new(7500, 5300),
                    thickness: Units(100),
                    dash: None,
                }),
            ],
        }),
    ];
    page
}

#[test]
fn one_of_each_primitive_renders_to_the_snapshot() {
    insta::assert_snapshot!(to_svg(&sample_page()));
}

#[test]
fn rendering_twice_gives_identical_bytes() {
    let page = sample_page();
    assert_eq!(to_svg(&page).into_bytes(), to_svg(&page).into_bytes());
    assert_eq!(to_svg(&page.clone()), to_svg(&page));
}

#[test]
fn the_root_element_carries_millimetres_and_a_unit_view_box() {
    let svg = to_svg(&sample_page());
    let first = svg.lines().next().unwrap();
    assert_eq!(
        first,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"20mm\" height=\"10.5mm\" viewBox=\"0 0 11703 6144\">"
    );
    assert!(!svg.contains("<text"));
    assert!(!svg.contains("font"));
    assert!(!svg.lines().skip(1).any(|line| line.contains('.')));
}

#[test]
fn a_glyph_is_drawn_from_its_outline_at_its_staff_space() {
    let mut page = sample_page();
    page.items = vec![Primitive::Glyph(PlacedGlyph {
        origin: Point::new(0, 0),
        codepoint: NOTEHEAD_BLACK.codepoint,
        staff_space: STAFF_SPACE,
    })];
    let normal = to_svg(&page);
    page.items = vec![Primitive::Glyph(PlacedGlyph {
        origin: Point::new(0, 0),
        codepoint: NOTEHEAD_BLACK.codepoint,
        staff_space: Units(STAFF_SPACE.0 * 2),
    })];
    let double = to_svg(&page);
    let max_x = |svg: &str| -> i64 {
        let path = svg.lines().find(|line| line.starts_with("<path")).unwrap();
        let data = path.split('"').nth(1).unwrap();
        data.split(|c: char| c.is_ascii_alphabetic())
            .flat_map(|chunk| {
                chunk
                    .split_whitespace()
                    .map(|value| value.parse::<i64>().unwrap())
                    .collect::<Vec<_>>()
                    .chunks(2)
                    .map(|pair| pair[0])
                    .collect::<Vec<_>>()
            })
            .max()
            .unwrap()
    };
    assert!((max_x(&normal) - 1208).abs() <= 1, "{}", max_x(&normal));
    assert!((max_x(&double) - 2416).abs() <= 2, "{}", max_x(&double));
}

#[test]
fn text_anchors_shift_the_run_by_its_measured_width() {
    let run = |anchor: TextAnchor| {
        let mut page = sample_page();
        page.items = vec![Primitive::Text(TextRun {
            origin: Point::new(5000, 3000),
            text: "l".to_string(),
            font: Font::Text,
            em: Units(4096),
            anchor,
        })];
        to_svg(&page)
    };
    let width = opentab_layout::text::text_font()
        .measure("l", Units(4096))
        .0;
    let first_x = |svg: String| -> i64 {
        let path = svg
            .lines()
            .find(|line| line.starts_with("<path"))
            .unwrap()
            .to_string();
        let data = path.split('"').nth(1).unwrap().to_string();
        data.trim_start_matches('M')
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };
    let start = first_x(run(TextAnchor::Start));
    assert_eq!(first_x(run(TextAnchor::End)), start - width);
    assert_eq!(first_x(run(TextAnchor::Middle)), start - (width + 1) / 2);
}

#[test]
fn an_empty_page_is_just_the_paper() {
    let mut page = sample_page();
    page.items.clear();
    assert_eq!(
        to_svg(&page),
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"20mm\" height=\"10.5mm\" viewBox=\"0 0 11703 6144\">\n<rect width=\"11703\" height=\"6144\" fill=\"#ffffff\"/>\n</svg>\n"
    );
}
