use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;
use serde_json::value::RawValue;

use crate::decimal::staff_spaces_to_units;
pub use crate::outline::{FontError, Outline, OutlineFont};
use crate::units::{Point, Rect, Units};

pub static BRAVURA_OTF: &[u8] = include_bytes!("../../../assets/fonts/bravura/Bravura.otf");
pub static BRAVURA_METADATA: &str = include_str!("../../../assets/fonts/bravura/Bravura.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Glyph {
    pub name: &'static str,
    pub codepoint: char,
}

pub const G_CLEF: Glyph = Glyph {
    name: "gClef",
    codepoint: '\u{E050}',
};
pub const G_CLEF_8VB: Glyph = Glyph {
    name: "gClef8vb",
    codepoint: '\u{E052}',
};
pub const F_CLEF: Glyph = Glyph {
    name: "fClef",
    codepoint: '\u{E062}',
};
pub const UNPITCHED_PERCUSSION_CLEF_1: Glyph = Glyph {
    name: "unpitchedPercussionClef1",
    codepoint: '\u{E069}',
};
pub const TAB_CLEF: Glyph = Glyph {
    name: "6stringTabClef",
    codepoint: '\u{E06D}',
};
pub const TAB_CLEF_4: Glyph = Glyph {
    name: "4stringTabClef",
    codepoint: '\u{E06E}',
};
pub const REPEAT_DOT: Glyph = Glyph {
    name: "repeatDot",
    codepoint: '\u{E044}',
};
pub const BRACKET_TOP: Glyph = Glyph {
    name: "bracketTop",
    codepoint: '\u{E003}',
};
pub const BRACKET_BOTTOM: Glyph = Glyph {
    name: "bracketBottom",
    codepoint: '\u{E004}',
};
pub const NOTEHEAD_DOUBLE_WHOLE: Glyph = Glyph {
    name: "noteheadDoubleWhole",
    codepoint: '\u{E0A0}',
};
pub const NOTEHEAD_WHOLE: Glyph = Glyph {
    name: "noteheadWhole",
    codepoint: '\u{E0A2}',
};
pub const NOTEHEAD_HALF: Glyph = Glyph {
    name: "noteheadHalf",
    codepoint: '\u{E0A3}',
};
pub const NOTEHEAD_BLACK: Glyph = Glyph {
    name: "noteheadBlack",
    codepoint: '\u{E0A4}',
};
pub const NOTEHEAD_X_BLACK: Glyph = Glyph {
    name: "noteheadXBlack",
    codepoint: '\u{E0A9}',
};
pub const ACCIDENTAL_FLAT: Glyph = Glyph {
    name: "accidentalFlat",
    codepoint: '\u{E260}',
};
pub const ACCIDENTAL_NATURAL: Glyph = Glyph {
    name: "accidentalNatural",
    codepoint: '\u{E261}',
};
pub const ACCIDENTAL_SHARP: Glyph = Glyph {
    name: "accidentalSharp",
    codepoint: '\u{E262}',
};
pub const REST_WHOLE: Glyph = Glyph {
    name: "restWhole",
    codepoint: '\u{E4E3}',
};
pub const REST_HALF: Glyph = Glyph {
    name: "restHalf",
    codepoint: '\u{E4E4}',
};
pub const REST_QUARTER: Glyph = Glyph {
    name: "restQuarter",
    codepoint: '\u{E4E5}',
};
pub const REST_8TH: Glyph = Glyph {
    name: "rest8th",
    codepoint: '\u{E4E6}',
};
pub const FLAG_8TH_UP: Glyph = Glyph {
    name: "flag8thUp",
    codepoint: '\u{E240}',
};
pub const FLAG_8TH_DOWN: Glyph = Glyph {
    name: "flag8thDown",
    codepoint: '\u{E241}',
};
pub const AUGMENTATION_DOT: Glyph = Glyph {
    name: "augmentationDot",
    codepoint: '\u{E1E7}',
};
pub const NOTEHEAD_X_WHOLE: Glyph = Glyph {
    name: "noteheadXWhole",
    codepoint: '\u{E0A7}',
};
pub const NOTEHEAD_X_HALF: Glyph = Glyph {
    name: "noteheadXHalf",
    codepoint: '\u{E0A8}',
};
pub const NOTEHEAD_DIAMOND_WHOLE: Glyph = Glyph {
    name: "noteheadDiamondWhole",
    codepoint: '\u{E0D8}',
};
pub const NOTEHEAD_DIAMOND_HALF: Glyph = Glyph {
    name: "noteheadDiamondHalf",
    codepoint: '\u{E0D9}',
};
pub const NOTEHEAD_DIAMOND_BLACK: Glyph = Glyph {
    name: "noteheadDiamondBlack",
    codepoint: '\u{E0DB}',
};
pub const NOTEHEAD_PARENTHESIS_LEFT: Glyph = Glyph {
    name: "noteheadParenthesisLeft",
    codepoint: '\u{E0F5}',
};
pub const NOTEHEAD_PARENTHESIS_RIGHT: Glyph = Glyph {
    name: "noteheadParenthesisRight",
    codepoint: '\u{E0F6}',
};
pub const ACCIDENTAL_DOUBLE_SHARP: Glyph = Glyph {
    name: "accidentalDoubleSharp",
    codepoint: '\u{E263}',
};
pub const ACCIDENTAL_DOUBLE_FLAT: Glyph = Glyph {
    name: "accidentalDoubleFlat",
    codepoint: '\u{E264}',
};
pub const REST_16TH: Glyph = Glyph {
    name: "rest16th",
    codepoint: '\u{E4E7}',
};
pub const REST_32ND: Glyph = Glyph {
    name: "rest32nd",
    codepoint: '\u{E4E8}',
};
pub const REST_64TH: Glyph = Glyph {
    name: "rest64th",
    codepoint: '\u{E4E9}',
};
pub const REST_128TH: Glyph = Glyph {
    name: "rest128th",
    codepoint: '\u{E4EA}',
};
pub const FLAG_16TH_UP: Glyph = Glyph {
    name: "flag16thUp",
    codepoint: '\u{E242}',
};
pub const FLAG_16TH_DOWN: Glyph = Glyph {
    name: "flag16thDown",
    codepoint: '\u{E243}',
};
pub const FLAG_32ND_UP: Glyph = Glyph {
    name: "flag32ndUp",
    codepoint: '\u{E244}',
};
pub const FLAG_32ND_DOWN: Glyph = Glyph {
    name: "flag32ndDown",
    codepoint: '\u{E245}',
};
pub const FLAG_64TH_UP: Glyph = Glyph {
    name: "flag64thUp",
    codepoint: '\u{E246}',
};
pub const FLAG_64TH_DOWN: Glyph = Glyph {
    name: "flag64thDown",
    codepoint: '\u{E247}',
};
pub const FLAG_128TH_UP: Glyph = Glyph {
    name: "flag128thUp",
    codepoint: '\u{E248}',
};
pub const FLAG_128TH_DOWN: Glyph = Glyph {
    name: "flag128thDown",
    codepoint: '\u{E249}',
};
pub const ARTIC_ACCENT_ABOVE: Glyph = Glyph {
    name: "articAccentAbove",
    codepoint: '\u{E4A0}',
};
pub const ARTIC_ACCENT_BELOW: Glyph = Glyph {
    name: "articAccentBelow",
    codepoint: '\u{E4A1}',
};
pub const ARTIC_STACCATO_ABOVE: Glyph = Glyph {
    name: "articStaccatoAbove",
    codepoint: '\u{E4A2}',
};
pub const ARTIC_STACCATO_BELOW: Glyph = Glyph {
    name: "articStaccatoBelow",
    codepoint: '\u{E4A3}',
};
pub const ARTIC_MARCATO_ABOVE: Glyph = Glyph {
    name: "articMarcatoAbove",
    codepoint: '\u{E4AC}',
};
pub const ARTIC_MARCATO_BELOW: Glyph = Glyph {
    name: "articMarcatoBelow",
    codepoint: '\u{E4AD}',
};
pub const TIME_SIG_0: Glyph = Glyph {
    name: "timeSig0",
    codepoint: '\u{E080}',
};
pub const TIME_SIG_DIGITS: [Glyph; 10] = [
    TIME_SIG_0,
    Glyph {
        name: "timeSig1",
        codepoint: '\u{E081}',
    },
    Glyph {
        name: "timeSig2",
        codepoint: '\u{E082}',
    },
    Glyph {
        name: "timeSig3",
        codepoint: '\u{E083}',
    },
    Glyph {
        name: "timeSig4",
        codepoint: '\u{E084}',
    },
    Glyph {
        name: "timeSig5",
        codepoint: '\u{E085}',
    },
    Glyph {
        name: "timeSig6",
        codepoint: '\u{E086}',
    },
    Glyph {
        name: "timeSig7",
        codepoint: '\u{E087}',
    },
    Glyph {
        name: "timeSig8",
        codepoint: '\u{E088}',
    },
    Glyph {
        name: "timeSig9",
        codepoint: '\u{E089}',
    },
];

pub const NAMED: [Glyph; 51] = [
    G_CLEF,
    G_CLEF_8VB,
    F_CLEF,
    UNPITCHED_PERCUSSION_CLEF_1,
    TAB_CLEF,
    TAB_CLEF_4,
    REPEAT_DOT,
    BRACKET_TOP,
    BRACKET_BOTTOM,
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
    NOTEHEAD_X_WHOLE,
    NOTEHEAD_X_HALF,
    NOTEHEAD_DIAMOND_WHOLE,
    NOTEHEAD_DIAMOND_HALF,
    NOTEHEAD_DIAMOND_BLACK,
    NOTEHEAD_PARENTHESIS_LEFT,
    NOTEHEAD_PARENTHESIS_RIGHT,
    ACCIDENTAL_DOUBLE_SHARP,
    ACCIDENTAL_DOUBLE_FLAT,
    REST_16TH,
    REST_32ND,
    REST_64TH,
    REST_128TH,
    FLAG_16TH_UP,
    FLAG_16TH_DOWN,
    FLAG_32ND_UP,
    FLAG_32ND_DOWN,
    FLAG_64TH_UP,
    FLAG_64TH_DOWN,
    FLAG_128TH_UP,
    FLAG_128TH_DOWN,
    ARTIC_ACCENT_ABOVE,
    ARTIC_ACCENT_BELOW,
    ARTIC_STACCATO_ABOVE,
    ARTIC_STACCATO_BELOW,
    ARTIC_MARCATO_ABOVE,
    ARTIC_MARCATO_BELOW,
];

pub const TUPLET_DIGITS: [Glyph; 10] = [
    Glyph {
        name: "tuplet0",
        codepoint: '\u{E880}',
    },
    Glyph {
        name: "tuplet1",
        codepoint: '\u{E881}',
    },
    Glyph {
        name: "tuplet2",
        codepoint: '\u{E882}',
    },
    Glyph {
        name: "tuplet3",
        codepoint: '\u{E883}',
    },
    Glyph {
        name: "tuplet4",
        codepoint: '\u{E884}',
    },
    Glyph {
        name: "tuplet5",
        codepoint: '\u{E885}',
    },
    Glyph {
        name: "tuplet6",
        codepoint: '\u{E886}',
    },
    Glyph {
        name: "tuplet7",
        codepoint: '\u{E887}',
    },
    Glyph {
        name: "tuplet8",
        codepoint: '\u{E888}',
    },
    Glyph {
        name: "tuplet9",
        codepoint: '\u{E889}',
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Anchor {
    StemUpSE,
    StemUpNW,
    StemDownNW,
    StemDownSW,
    SplitStemUpSE,
    SplitStemUpSW,
    SplitStemDownNE,
    SplitStemDownNW,
    CutOutNE,
    CutOutSE,
    CutOutSW,
    CutOutNW,
    GraceNoteSlashSW,
    GraceNoteSlashNE,
    GraceNoteSlashNW,
    GraceNoteSlashSE,
    RepeatOffset,
    NoteheadOrigin,
    OpticalCenter,
    NumeralBottom,
}

impl Anchor {
    pub fn key(self) -> &'static str {
        match self {
            Anchor::StemUpSE => "stemUpSE",
            Anchor::StemUpNW => "stemUpNW",
            Anchor::StemDownNW => "stemDownNW",
            Anchor::StemDownSW => "stemDownSW",
            Anchor::SplitStemUpSE => "splitStemUpSE",
            Anchor::SplitStemUpSW => "splitStemUpSW",
            Anchor::SplitStemDownNE => "splitStemDownNE",
            Anchor::SplitStemDownNW => "splitStemDownNW",
            Anchor::CutOutNE => "cutOutNE",
            Anchor::CutOutSE => "cutOutSE",
            Anchor::CutOutSW => "cutOutSW",
            Anchor::CutOutNW => "cutOutNW",
            Anchor::GraceNoteSlashSW => "graceNoteSlashSW",
            Anchor::GraceNoteSlashNE => "graceNoteSlashNE",
            Anchor::GraceNoteSlashNW => "graceNoteSlashNW",
            Anchor::GraceNoteSlashSE => "graceNoteSlashSE",
            Anchor::RepeatOffset => "repeatOffset",
            Anchor::NoteheadOrigin => "noteheadOrigin",
            Anchor::OpticalCenter => "opticalCenter",
            Anchor::NumeralBottom => "numeralBottom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngravingDefaults {
    pub arrow_shaft_thickness: Units,
    pub barline_separation: Units,
    pub beam_spacing: Units,
    pub beam_thickness: Units,
    pub bracket_thickness: Units,
    pub dashed_barline_dash_length: Units,
    pub dashed_barline_gap_length: Units,
    pub dashed_barline_thickness: Units,
    pub h_bar_thickness: Units,
    pub hairpin_thickness: Units,
    pub leger_line_extension: Units,
    pub leger_line_thickness: Units,
    pub lyric_line_thickness: Units,
    pub octave_line_thickness: Units,
    pub pedal_line_thickness: Units,
    pub repeat_barline_dot_separation: Units,
    pub repeat_ending_line_thickness: Units,
    pub slur_endpoint_thickness: Units,
    pub slur_midpoint_thickness: Units,
    pub staff_line_thickness: Units,
    pub stem_thickness: Units,
    pub sub_bracket_thickness: Units,
    pub text_enclosure_thickness: Units,
    pub thick_barline_thickness: Units,
    pub thin_barline_thickness: Units,
    pub thin_thick_barline_separation: Units,
    pub tie_endpoint_thickness: Units,
    pub tie_midpoint_thickness: Units,
    pub tuplet_bracket_thickness: Units,
}

pub struct Metrics {
    font: OutlineFont,
    bounding_boxes: BTreeMap<String, Rect>,
    advances: BTreeMap<String, Units>,
    anchors: BTreeMap<String, BTreeMap<String, Point>>,
    defaults: EngravingDefaults,
}

#[derive(Deserialize)]
struct RawBox {
    #[serde(rename = "bBoxNE")]
    north_east: [Box<RawValue>; 2],
    #[serde(rename = "bBoxSW")]
    south_west: [Box<RawValue>; 2],
}

#[derive(Deserialize)]
struct RawMetadata {
    #[serde(rename = "engravingDefaults")]
    engraving_defaults: BTreeMap<String, Box<RawValue>>,
    #[serde(rename = "glyphAdvanceWidths")]
    glyph_advance_widths: BTreeMap<String, Box<RawValue>>,
    #[serde(rename = "glyphBBoxes")]
    glyph_bboxes: BTreeMap<String, RawBox>,
    #[serde(rename = "glyphsWithAnchors")]
    glyphs_with_anchors: BTreeMap<String, BTreeMap<String, [Box<RawValue>; 2]>>,
}

static BRAVURA: OnceLock<Metrics> = OnceLock::new();

impl Metrics {
    pub fn bravura() -> &'static Metrics {
        BRAVURA.get_or_init(|| match Metrics::load(BRAVURA_OTF, BRAVURA_METADATA) {
            Ok(metrics) => metrics,
            Err(error) => panic!("the bundled Bravura font does not load: {error}"),
        })
    }

    pub fn load(font: &'static [u8], metadata: &str) -> Result<Metrics, FontError> {
        let font = OutlineFont::parse(font)?;
        let raw: RawMetadata =
            serde_json::from_str(metadata).map_err(|error| FontError(error.to_string()))?;
        let bounding_boxes = raw
            .glyph_bboxes
            .iter()
            .map(|(name, raw_box)| Ok((name.clone(), bounding_box(raw_box)?)))
            .collect::<Result<_, FontError>>()?;
        let advances = raw
            .glyph_advance_widths
            .iter()
            .map(|(name, width)| Ok((name.clone(), number(width)?)))
            .collect::<Result<_, FontError>>()?;
        let anchors = raw
            .glyphs_with_anchors
            .iter()
            .map(|(name, points)| {
                let points = points
                    .iter()
                    .map(|(anchor, raw_point)| Ok((anchor.clone(), point(raw_point)?)))
                    .collect::<Result<_, FontError>>()?;
                Ok((name.clone(), points))
            })
            .collect::<Result<_, FontError>>()?;
        let defaults = engraving_defaults(&raw.engraving_defaults)?;
        Ok(Metrics {
            font,
            bounding_boxes,
            advances,
            anchors,
            defaults,
        })
    }

    pub fn font(&self) -> &OutlineFont {
        &self.font
    }

    pub fn outline(&self, codepoint: char) -> Outline {
        self.font.outline(codepoint)
    }

    pub fn bbox(&self, name: &str) -> Option<Rect> {
        self.bounding_boxes.get(name).copied()
    }

    pub fn advance(&self, name: &str) -> Option<Units> {
        self.advances.get(name).copied()
    }

    pub fn anchor(&self, name: &str, anchor: Anchor) -> Option<Point> {
        self.anchor_by_key(name, anchor.key())
    }

    pub fn anchor_by_key(&self, name: &str, key: &str) -> Option<Point> {
        self.anchors.get(name)?.get(key).copied()
    }

    pub fn engraving_defaults(&self) -> &EngravingDefaults {
        &self.defaults
    }
}

fn number(raw: &RawValue) -> Result<Units, FontError> {
    staff_spaces_to_units(raw.get()).map_err(|error| FontError(error.0))
}

fn point(raw: &[Box<RawValue>; 2]) -> Result<Point, FontError> {
    let [x, y] = raw;
    Ok(Point {
        x: number(x)?,
        y: -number(y)?,
    })
}

fn bounding_box(raw: &RawBox) -> Result<Rect, FontError> {
    let north_east = point(&raw.north_east)?;
    let south_west = point(&raw.south_west)?;
    Ok(Rect::new(
        Point {
            x: south_west.x,
            y: north_east.y,
        },
        Point {
            x: north_east.x,
            y: south_west.y,
        },
    ))
}

fn engraving_defaults(
    raw: &BTreeMap<String, Box<RawValue>>,
) -> Result<EngravingDefaults, FontError> {
    let value = |key: &str| match raw.get(key) {
        Some(found) => number(found),
        None => Err(FontError(format!("engravingDefaults lacks {key}"))),
    };
    Ok(EngravingDefaults {
        arrow_shaft_thickness: value("arrowShaftThickness")?,
        barline_separation: value("barlineSeparation")?,
        beam_spacing: value("beamSpacing")?,
        beam_thickness: value("beamThickness")?,
        bracket_thickness: value("bracketThickness")?,
        dashed_barline_dash_length: value("dashedBarlineDashLength")?,
        dashed_barline_gap_length: value("dashedBarlineGapLength")?,
        dashed_barline_thickness: value("dashedBarlineThickness")?,
        h_bar_thickness: value("hBarThickness")?,
        hairpin_thickness: value("hairpinThickness")?,
        leger_line_extension: value("legerLineExtension")?,
        leger_line_thickness: value("legerLineThickness")?,
        lyric_line_thickness: value("lyricLineThickness")?,
        octave_line_thickness: value("octaveLineThickness")?,
        pedal_line_thickness: value("pedalLineThickness")?,
        repeat_barline_dot_separation: value("repeatBarlineDotSeparation")?,
        repeat_ending_line_thickness: value("repeatEndingLineThickness")?,
        slur_endpoint_thickness: value("slurEndpointThickness")?,
        slur_midpoint_thickness: value("slurMidpointThickness")?,
        staff_line_thickness: value("staffLineThickness")?,
        stem_thickness: value("stemThickness")?,
        sub_bracket_thickness: value("subBracketThickness")?,
        text_enclosure_thickness: value("textEnclosureThickness")?,
        thick_barline_thickness: value("thickBarlineThickness")?,
        thin_barline_thickness: value("thinBarlineThickness")?,
        thin_thick_barline_separation: value("thinThickBarlineSeparation")?,
        tie_endpoint_thickness: value("tieEndpointThickness")?,
        tie_midpoint_thickness: value("tieMidpointThickness")?,
        tuplet_bracket_thickness: value("tupletBracketThickness")?,
    })
}
