use std::fmt::Write;

use opentab_layout::glyphs::Metrics;
use opentab_layout::text::font;
use opentab_layout::{
    Colour, FilledRect, Group, Line, Micrometres, Page, PathSegment, PathStyle, PlacedGlyph, Point,
    Primitive, STAVES_PER_EM, Shape, TextAnchor, TextRun, Units,
};

const BLACK: &str = "#000000";
const WHITE: &str = "#ffffff";
const MICROMETRES_PER_MM: i64 = 1000;

pub fn to_svg(page: &Page) -> String {
    let width = page.width().0;
    let height = page.height().0;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}mm\" height=\"{}mm\" viewBox=\"0 0 {width} {height}\">",
        millimetres(page.size.width),
        millimetres(page.size.height)
    );
    let _ = writeln!(
        out,
        "<rect width=\"{width}\" height=\"{height}\" fill=\"{WHITE}\"/>"
    );
    for item in &page.items {
        primitive(&mut out, item);
    }
    out.push_str("</svg>\n");
    out
}

fn millimetres(length: Micrometres) -> String {
    let sign = if length.0 < 0 { "-" } else { "" };
    let magnitude = length.0.unsigned_abs();
    let whole = magnitude / MICROMETRES_PER_MM.unsigned_abs();
    let fraction = magnitude % MICROMETRES_PER_MM.unsigned_abs();
    if fraction == 0 {
        format!("{sign}{whole}")
    } else {
        let digits = format!("{fraction:03}");
        format!("{sign}{whole}.{}", digits.trim_end_matches('0'))
    }
}

fn primitive(out: &mut String, item: &Primitive) {
    match item {
        Primitive::Glyph(glyph) => placed_glyph(out, glyph),
        Primitive::Line(line) => stroke_line(out, line),
        Primitive::Shape(shape) => drawn_shape(out, shape),
        Primitive::Text(run) => text_run(out, run),
        Primitive::FilledRect(rect) => filled_rect(out, rect),
        Primitive::Group(group) => grouped(out, group),
    }
}

fn placed_glyph(out: &mut String, glyph: &PlacedGlyph) {
    let em = Units(glyph.staff_space.0.saturating_mul(STAVES_PER_EM));
    let segments = Metrics::bravura()
        .outline(glyph.codepoint)
        .placed(glyph.origin, em);
    filled_path(out, &segments);
}

fn stroke_line(out: &mut String, line: &Line) {
    let _ = write!(
        out,
        "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{BLACK}\" stroke-width=\"{}\"",
        line.from.x.0, line.from.y.0, line.to.x.0, line.to.y.0, line.thickness.0
    );
    if let Some(dash) = line.dash {
        let _ = write!(out, " stroke-dasharray=\"{} {}\"", dash.on.0, dash.off.0);
    }
    out.push_str("/>\n");
}

fn drawn_shape(out: &mut String, shape: &Shape) {
    let data = path_data(&shape.segments);
    match shape.style {
        PathStyle::Fill => {
            let _ = writeln!(out, "<path d=\"{data}\" fill=\"{BLACK}\"/>");
        }
        PathStyle::Stroke(thickness) => {
            let _ = writeln!(
                out,
                "<path d=\"{data}\" fill=\"none\" stroke=\"{BLACK}\" stroke-width=\"{}\"/>",
                thickness.0
            );
        }
    }
}

fn text_run(out: &mut String, run: &TextRun) {
    let face = font(run.font);
    let positions = face.pen_positions(&run.text, run.em);
    let width = positions.last().copied().unwrap_or(Units::ZERO);
    let shift = match run.anchor {
        TextAnchor::Start => Units::ZERO,
        TextAnchor::Middle => width.half(),
        TextAnchor::End => width,
    };
    let mut segments = Vec::new();
    for (character, pen) in run.text.chars().zip(positions) {
        let origin = Point {
            x: run.origin.x + pen - shift,
            y: run.origin.y,
        };
        segments.extend(face.outline(character).placed(origin, run.em));
    }
    filled_path(out, &segments);
}

fn filled_rect(out: &mut String, filled: &FilledRect) {
    let colour = match filled.colour {
        Colour::Black => BLACK,
        Colour::White => WHITE,
    };
    let _ = writeln!(
        out,
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{colour}\"/>",
        filled.rect.min.x.0,
        filled.rect.min.y.0,
        filled.rect.width().0,
        filled.rect.height().0
    );
}

fn grouped(out: &mut String, group: &Group) {
    let _ = writeln!(out, "<g data-id=\"{}\">", group.id.0);
    for item in &group.items {
        primitive(out, item);
    }
    out.push_str("</g>\n");
}

fn filled_path(out: &mut String, segments: &[PathSegment]) {
    if segments.is_empty() {
        return;
    }
    let _ = writeln!(out, "<path d=\"{}\"/>", path_data(segments));
}

fn path_data(segments: &[PathSegment]) -> String {
    let mut data = String::new();
    for segment in segments {
        match *segment {
            PathSegment::MoveTo(to) => command(&mut data, 'M', &[to]),
            PathSegment::LineTo(to) => command(&mut data, 'L', &[to]),
            PathSegment::QuadTo(control, to) => command(&mut data, 'Q', &[control, to]),
            PathSegment::CubicTo(first, second, to) => {
                command(&mut data, 'C', &[first, second, to]);
            }
            PathSegment::Close => data.push('Z'),
        }
    }
    data
}

fn command(data: &mut String, letter: char, points: &[Point]) {
    data.push(letter);
    for (index, point) in points.iter().enumerate() {
        if index > 0 {
            data.push(' ');
        }
        let _ = write!(data, "{} {}", point.x.0, point.y.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn millimetres_print_without_trailing_zeros() {
        assert_eq!(millimetres(Micrometres(210_000)), "210");
        assert_eq!(millimetres(Micrometres(215_900)), "215.9");
        assert_eq!(millimetres(Micrometres(279_400)), "279.4");
        assert_eq!(millimetres(Micrometres(1_005)), "1.005");
        assert_eq!(millimetres(Micrometres(50)), "0.05");
        assert_eq!(millimetres(Micrometres(-1_500)), "-1.5");
        assert_eq!(millimetres(Micrometres(0)), "0");
    }

    #[test]
    fn path_data_is_compact_integer_text() {
        let segments = [
            PathSegment::MoveTo(Point::new(1, -2)),
            PathSegment::LineTo(Point::new(3, 4)),
            PathSegment::QuadTo(Point::new(5, 6), Point::new(7, 8)),
            PathSegment::CubicTo(Point::new(9, 10), Point::new(11, 12), Point::new(-13, 14)),
            PathSegment::Close,
        ];
        assert_eq!(path_data(&segments), "M1 -2L3 4Q5 6 7 8C9 10 11 12 -13 14Z");
    }
}
