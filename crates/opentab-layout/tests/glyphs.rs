use opentab_layout::glyphs::{
    ACCIDENTAL_FLAT, ACCIDENTAL_NATURAL, ACCIDENTAL_SHARP, AUGMENTATION_DOT, Anchor, F_CLEF,
    FLAG_8TH_DOWN, FLAG_8TH_UP, G_CLEF, G_CLEF_8VB, Glyph, Metrics, NOTEHEAD_BLACK,
    NOTEHEAD_DOUBLE_WHOLE, NOTEHEAD_HALF, NOTEHEAD_WHOLE, NOTEHEAD_X_BLACK, REST_8TH, REST_HALF,
    REST_QUARTER, REST_WHOLE, TAB_CLEF, TIME_SIG_0, UNPITCHED_PERCUSSION_CLEF_1,
};
use opentab_layout::glyphs::{
    BRACKET_BOTTOM, BRACKET_TOP, REPEAT_DOT, TAB_CLEF_4, TIME_SIG_DIGITS,
};
use opentab_layout::text::text_font;
use opentab_layout::{Point, REFERENCE_EM, Rect, STAFF_SPACE, Units};

const ALL: [Glyph; 25] = [
    TAB_CLEF_4,
    REPEAT_DOT,
    BRACKET_TOP,
    BRACKET_BOTTOM,
    G_CLEF,
    G_CLEF_8VB,
    F_CLEF,
    UNPITCHED_PERCUSSION_CLEF_1,
    TAB_CLEF,
    NOTEHEAD_DOUBLE_WHOLE,
    NOTEHEAD_WHOLE,
    NOTEHEAD_HALF,
    NOTEHEAD_BLACK,
    NOTEHEAD_X_BLACK,
    ACCIDENTAL_FLAT,
    ACCIDENTAL_NATURAL,
    ACCIDENTAL_SHARP,
    REST_WHOLE,
    REST_HALF,
    REST_QUARTER,
    REST_8TH,
    FLAG_8TH_UP,
    FLAG_8TH_DOWN,
    AUGMENTATION_DOT,
    TIME_SIG_0,
];

fn rect(min_x: i64, min_y: i64, max_x: i64, max_y: i64) -> Rect {
    Rect::new(Point::new(min_x, min_y), Point::new(max_x, max_y))
}

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
    for glyph in ALL {
        let outline = metrics.outline(glyph.codepoint);
        let bounds = outline.bounds().unwrap_or_else(|| panic!("{}", glyph.name));
        let expected = metrics
            .bbox(glyph.name)
            .unwrap_or_else(|| panic!("{}", glyph.name));
        assert!(
            within_one(bounds, expected),
            "{}: outline {bounds:?} metadata {expected:?}",
            glyph.name
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
