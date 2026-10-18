use opentab_model::{
    Clef, Dynamic, Finger, Grace, HarmonicKind, NoteValue, Slide, StrokeDirection, TempoUnit,
    Vibrato,
};

pub(crate) type Table<T> = [(&'static str, T)];

pub(crate) const NOTE_VALUES: &Table<NoteValue> = &[
    ("Whole", NoteValue::Whole),
    ("Half", NoteValue::Half),
    ("Quarter", NoteValue::Quarter),
    ("Eighth", NoteValue::Eighth),
    ("16th", NoteValue::Sixteenth),
    ("32nd", NoteValue::ThirtySecond),
    ("64th", NoteValue::SixtyFourth),
    ("128th", NoteValue::OneTwentyEighth),
];

pub(crate) const DYNAMICS: &Table<Dynamic> = &[
    ("PPP", Dynamic::Ppp),
    ("PP", Dynamic::Pp),
    ("P", Dynamic::P),
    ("MP", Dynamic::Mp),
    ("MF", Dynamic::Mf),
    ("F", Dynamic::F),
    ("FF", Dynamic::Ff),
    ("FFF", Dynamic::Fff),
];

pub(crate) const CLEFS: &Table<Clef> = &[
    ("G2", Clef::Treble),
    ("F4", Clef::Bass),
    ("Neutral", Clef::Neutral),
];

pub(crate) const GRACES: &Table<Grace> =
    &[("BeforeBeat", Grace::BeforeBeat), ("OnBeat", Grace::OnBeat)];

pub(crate) const VIBRATOS: &Table<Vibrato> =
    &[("Slight", Vibrato::Slight), ("Wide", Vibrato::Wide)];

pub(crate) const FINGERS: &Table<Finger> = &[
    ("Open", Finger::Open),
    ("P", Finger::Thumb),
    ("I", Finger::Index),
    ("M", Finger::Middle),
    ("A", Finger::Ring),
    ("C", Finger::Little),
];

pub(crate) const HARMONICS: &Table<HarmonicKind> = &[
    ("Natural", HarmonicKind::Natural),
    ("Artificial", HarmonicKind::Artificial),
    ("Pinch", HarmonicKind::Pinch),
    ("Tap", HarmonicKind::Tap),
    ("Semi", HarmonicKind::Semi),
    ("Feedback", HarmonicKind::Feedback),
];

pub(crate) const TREMOLOS: &Table<NoteValue> = &[
    ("1/2", NoteValue::Eighth),
    ("1/4", NoteValue::Sixteenth),
    ("1/8", NoteValue::ThirtySecond),
];

pub(crate) const STROKE_DIRECTIONS: &Table<StrokeDirection> =
    &[("Up", StrokeDirection::Up), ("Down", StrokeDirection::Down)];

pub(crate) const SLIDES: &[(u32, Slide)] = &[
    (1, Slide::Shift),
    (2, Slide::Legato),
    (4, Slide::OutDownwards),
    (8, Slide::OutUpwards),
    (16, Slide::IntoFromBelow),
    (32, Slide::IntoFromAbove),
];

pub(crate) const TEMPO_UNITS: &[(u8, TempoUnit)] = &[
    (1, TempoUnit::Eighth),
    (2, TempoUnit::Quarter),
    (3, TempoUnit::DottedQuarter),
    (4, TempoUnit::Half),
    (5, TempoUnit::DottedHalf),
    (0, TempoUnit::Quarter),
];

pub(crate) const STACCATO_BIT: u32 = 1;
pub(crate) const HEAVY_ACCENT_BIT: u32 = 4;
pub(crate) const ACCENT_BIT: u32 = 8;

pub(crate) const DRUM_KIT: &str = "drumKit";
pub(crate) const FADE_IN: &str = "FadeIn";
pub(crate) const GHOST: &str = "Normal";
pub(crate) const MINOR: &str = "Minor";
pub(crate) const BRUSH_DURATION: &str = "687935489";
pub(crate) const BRUSH_START_TIME: &str = "687935490";
pub(crate) const TEMPO_POSITION_GRID: i64 = 3840;

pub(crate) fn lookup<T: Copy>(table: &Table<T>, text: &str) -> Option<T> {
    let text = text.trim();
    table
        .iter()
        .find(|(name, _)| *name == text)
        .map(|(_, value)| *value)
}

pub(crate) fn name_of<T: Copy + PartialEq>(table: &Table<T>, value: T) -> &'static str {
    table
        .iter()
        .find(|(_, candidate)| *candidate == value)
        .map(|(name, _)| *name)
        .unwrap_or_default()
}

pub(crate) fn slide_bits(slide: Slide) -> u32 {
    SLIDES
        .iter()
        .find(|(_, candidate)| *candidate == slide)
        .map(|(bits, _)| *bits)
        .unwrap_or_default()
}
