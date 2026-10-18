use opentab_layout::glyphs::{
    Anchor, Metrics, NAMED, NOTEHEAD_BLACK, TIME_SIG_DIGITS, TUPLET_DIGITS,
};
use opentab_layout::text::text_font;
use opentab_layout::{Point, REFERENCE_EM, Rect, STAFF_SPACE, Units};

fn rect(min_x: i64, min_y: i64, max_x: i64, max_y: i64) -> Rect {
    Rect::new(Point::new(min_x, min_y), Point::new(max_x, max_y))
}

fn encloses_closely(outline: Rect, metadata: Rect) -> bool {
    let outward = |outer: Units, inner: Units| (-1..=CONTROL_SLACK).contains(&(inner.0 - outer.0));
    outward(outline.min.x, metadata.min.x)
        && outward(outline.min.y, metadata.min.y)
        && outward(metadata.max.x, outline.max.x)
        && outward(metadata.max.y, outline.max.y)
}

const CONTROL_SLACK: i64 = 16;

fn within_one(left: Rect, right: Rect) -> bool {
    let close = |a: Units, b: Units| (a.0 - b.0).abs() <= 1;
    close(left.min.x, right.min.x)
        && close(left.min.y, right.min.y)
        && close(left.max.x, right.max.x)
        && close(left.max.y, right.max.y)
}

#[test]
fn metadata_bboxes_are_staff_spaces_times_1024_with_y_down() {
    let metrics = Metrics::bravura();
    assert_eq!(
        metrics.bbox("noteheadBlack"),
        Some(rect(0, -512, 1208, 512))
    );
    assert_eq!(metrics.bbox("gClef"), Some(rect(0, -4497, 2748, 2695)));
    assert_eq!(
        metrics.bbox("accidentalSharp"),
        Some(rect(0, -1434, 1020, 1425))
    );
    assert_eq!(
        metrics.bbox("restQuarter"),
        Some(rect(4, -1528, 1106, 1536))
    );
    assert_eq!(metrics.bbox("flag8thUp"), Some(rect(0, -37, 1081, 3318)));
    assert_eq!(metrics.bbox("noSuchGlyph"), None);
}

#[test]
fn outline_bounds_match_metadata_bboxes_for_every_named_glyph() {
    let metrics = Metrics::bravura();
    for glyph in NAMED {
        let outline = metrics.outline(glyph.codepoint);
        let bounds = outline.bounds().unwrap_or_else(|| panic!("{}", glyph.name));
        let expected = metrics
            .bbox(glyph.name)
            .unwrap_or_else(|| panic!("{}", glyph.name));
        assert!(
            within_one(bounds, expected) || encloses_closely(bounds, expected),
            "{}: outline {bounds:?} metadata {expected:?}",
            glyph.name
        );
    }
    for (digit, glyph) in TUPLET_DIGITS.iter().enumerate() {
        assert_eq!(glyph.name, format!("tuplet{digit}"));
        let bounds = metrics.outline(glyph.codepoint).bounds().unwrap();
        assert!(
            within_one(bounds, metrics.bbox(glyph.name).unwrap()),
            "{digit}"
        );
    }
    for (digit, glyph) in TIME_SIG_DIGITS.iter().enumerate() {
        assert_eq!(glyph.name, format!("timeSig{digit}"));
        let bounds = metrics.outline(glyph.codepoint).bounds().unwrap();
        assert!(
            within_one(bounds, metrics.bbox(glyph.name).unwrap()),
            "{digit}"
        );
    }
}

#[test]
fn notehead_outline_is_in_fixed_units() {
    let bounds = Metrics::bravura()
        .outline(NOTEHEAD_BLACK.codepoint)
        .bounds()
        .unwrap();
    assert!(within_one(bounds, rect(0, -512, 1208, 512)), "{bounds:?}");
}

#[test]
fn advances_come_from_the_metadata() {
    let metrics = Metrics::bravura();
    assert_eq!(metrics.advance("noteheadBlack"), Some(Units(1208)));
    assert_eq!(metrics.advance("gClef"), Some(Units(2748)));
    assert_eq!(
        metrics
            .font()
            .advance(NOTEHEAD_BLACK.codepoint, REFERENCE_EM),
        Units(1208)
    );
}

#[test]
fn anchors_are_points_in_units_with_y_down() {
    let metrics = Metrics::bravura();
    assert_eq!(
        metrics.anchor("noteheadBlack", Anchor::StemUpSE),
        Some(Point::new(1208, -172))
    );
    assert_eq!(
        metrics.anchor("noteheadBlack", Anchor::StemDownNW),
        Some(Point::new(0, 172))
    );
    assert_eq!(
        metrics.anchor_by_key("noteheadBlack", "cutOutNW"),
        Some(Point::new(213, -307))
    );
    assert_eq!(metrics.anchor("noteheadBlack", Anchor::RepeatOffset), None);
    assert_eq!(metrics.anchor("noSuchGlyph", Anchor::StemUpSE), None);
}

#[test]
fn every_anchor_kind_names_a_key_used_by_bravura() {
    let anchors = [
        Anchor::StemUpSE,
        Anchor::StemUpNW,
        Anchor::StemDownNW,
        Anchor::StemDownSW,
        Anchor::SplitStemUpSE,
        Anchor::SplitStemUpSW,
        Anchor::SplitStemDownNE,
        Anchor::SplitStemDownNW,
        Anchor::CutOutNE,
        Anchor::CutOutSE,
        Anchor::CutOutSW,
        Anchor::CutOutNW,
        Anchor::GraceNoteSlashSW,
        Anchor::GraceNoteSlashNE,
        Anchor::GraceNoteSlashNW,
        Anchor::GraceNoteSlashSE,
        Anchor::RepeatOffset,
        Anchor::NoteheadOrigin,
        Anchor::OpticalCenter,
        Anchor::NumeralBottom,
    ];
    let metadata: serde_json::Value =
        serde_json::from_str(opentab_layout::glyphs::BRAVURA_METADATA).unwrap();
    let anchored = metadata["glyphsWithAnchors"].as_object().unwrap();
    let metrics = Metrics::bravura();
    for anchor in anchors {
        let glyph = anchored
            .iter()
            .find(|(_, keys)| keys.get(anchor.key()).is_some())
            .map(|(name, _)| name.clone());
        let glyph = glyph.unwrap_or_else(|| panic!("{}", anchor.key()));
        assert!(metrics.anchor(&glyph, anchor).is_some(), "{}", anchor.key());
    }
    assert!(
        anchored
            .values()
            .all(|keys| keys.get("noSuchAnchor").is_none())
    );
}

#[test]
fn engraving_defaults_are_converted_exactly() {
    let defaults = Metrics::bravura().engraving_defaults();
    assert_eq!(defaults.staff_line_thickness, Units(133));
    assert_eq!(defaults.stem_thickness, Units(123));
    assert_eq!(defaults.beam_thickness, Units(512));
    assert_eq!(defaults.beam_spacing, Units(256));
    assert_eq!(defaults.leger_line_extension, Units(410));
    assert_eq!(defaults.h_bar_thickness, STAFF_SPACE);
}

#[test]
fn the_text_font_measures_runs_from_its_advances() {
    let font = text_font();
    assert_eq!(font.units_per_em(), 1000);
    assert!(font.has('A') && font.has('é') && font.has('ß'));
    let em = Units(2048);
    let positions = font.pen_positions("AV", em);
    assert_eq!(positions.len(), 3);
    assert_eq!(positions[0], Units::ZERO);
    assert_eq!(positions[1], font.advance('A', em));
    let total = font.advance_font_units('A') + font.advance_font_units('V');
    assert_eq!(positions[2], Units((total * 2048 * 2 + 1000) / 2000));
    assert_eq!(font.measure("AV", em), positions[2]);
    assert_eq!(font.measure("", em), Units::ZERO);
    assert!(font.outline('A').bounds().is_some());
    assert!(font.outline(' ').bounds().is_none());
    assert!(font.outline('\u{E050}').bounds().is_some());
}

const BRAVURA_BLANK: [u32; 44] = [
    0x0000, 0x0020, 0xE07F, 0xE09E, 0xE09F, 0xE0A5, 0xE31A, 0xE31B, 0xE3DE, 0xE3DF, 0xE548, 0xE8E0,
    0xE8E1, 0xE8E2, 0xE8E3, 0xE8E4, 0xE8E5, 0xE8E6, 0xE8E7, 0xEB90, 0xEB91, 0xEB92, 0xEB93, 0xEB94,
    0xEB95, 0xEB96, 0xEB97, 0xEB98, 0xEB99, 0xEB9A, 0xEB9B, 0xEB9C, 0xEB9D, 0xEB9E, 0xEB9F,
    0x1D159, 0x1D173, 0x1D174, 0x1D175, 0x1D176, 0x1D177, 0x1D178, 0x1D179, 0x1D17A,
];

fn blank_characters(font: &opentab_layout::text::OutlineFont) -> Vec<u32> {
    font.codepoints()
        .into_iter()
        .filter(|character| font.outline(*character).segments.is_empty())
        .map(u32::from)
        .collect()
}

#[test]
fn only_the_smufl_control_characters_of_bravura_have_no_outline() {
    let font = Metrics::bravura().font();
    assert!(font.codepoints().len() > 2_000);
    assert_eq!(blank_characters(font), BRAVURA_BLANK);
    for glyph in NAMED
        .iter()
        .chain(TIME_SIG_DIGITS.iter())
        .chain(TUPLET_DIGITS.iter())
    {
        assert!(font.has(glyph.codepoint), "{}", glyph.name);
        assert!(
            !BRAVURA_BLANK.contains(&u32::from(glyph.codepoint)),
            "{}",
            glyph.name
        );
    }
}

#[test]
fn only_blank_characters_of_the_text_font_have_no_outline() {
    let font = text_font();
    assert!(font.codepoints().len() > 200);
    let blank = blank_characters(font);
    assert_eq!(
        blank,
        [
            0x20, 0xA0, 0x34F, 0x2007, 0x2009, 0x200A, 0x200B, 0x202F, 0xFEFF
        ]
    );
    for code in blank {
        let character = char::from_u32(code).unwrap();
        assert!(
            character.is_whitespace() || matches!(character, '\u{34F}' | '\u{200B}' | '\u{FEFF}'),
            "U+{code:04X}"
        );
    }
}
