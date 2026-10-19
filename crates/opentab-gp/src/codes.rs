use opentab_model::{
    Accent, Clef, DirectionJump, DirectionTarget, Dynamic, Finger, Grace, HarmonicKind,
    InstrumentSet, NoteValue, Slide, StrokeDirection, TechniquePlacement, TempoUnit, TripletFeel,
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
pub(crate) const ELECTRIC_GUITAR: &str = "electricGuitar";

pub(crate) const INSTRUMENT_SETS: &Table<InstrumentSet> = &[
    (ELECTRIC_GUITAR, InstrumentSet::ElectricGuitar),
    ("steelGuitar", InstrumentSet::SteelGuitar),
    ("nylonGuitar", InstrumentSet::NylonGuitar),
    ("electricBass", InstrumentSet::ElectricBass),
    ("acousticBass", InstrumentSet::AcousticBass),
    ("violin", InstrumentSet::Violin),
    ("acousticPiano", InstrumentSet::AcousticPiano),
    ("saxophone", InstrumentSet::Saxophone),
    ("trumpet", InstrumentSet::Trumpet),
    ("electricOrgan", InstrumentSet::ElectricOrgan),
    ("voice", InstrumentSet::Voice),
    (DRUM_KIT, InstrumentSet::DrumKit),
];

pub(crate) const TECHNIQUE_PLACEMENTS: &Table<TechniquePlacement> = &[
    ("outside", TechniquePlacement::Outside),
    ("inside", TechniquePlacement::Inside),
    ("above", TechniquePlacement::Above),
    ("below", TechniquePlacement::Below),
];

pub(crate) const DRUM_ELEMENTS: &str = "Elements";
pub(crate) const DRUM_ELEMENT: &str = "Element";
pub(crate) const DRUM_ELEMENT_FIELDS: [&str; 4] =
    ["Name", "Type", "SoundbankName", "Articulations"];
pub(crate) const DRUM_ARTICULATION: &str = "Articulation";
pub(crate) const DRUM_ARTICULATION_FIELDS: [&str; 8] = [
    "Name",
    "StaffLine",
    "Noteheads",
    "TechniquePlacement",
    "TechniqueSymbol",
    "InputMidiNumbers",
    "OutputRSESound",
    "OutputMidiNumber",
];

pub(crate) const DIRECTION_TARGETS: &Table<DirectionTarget> = &[
    ("Segno", DirectionTarget::Segno),
    ("SegnoSegno", DirectionTarget::SegnoSegno),
    ("Coda", DirectionTarget::Coda),
    ("DoubleCoda", DirectionTarget::DoubleCoda),
    ("Fine", DirectionTarget::Fine),
];

pub(crate) const DIRECTION_JUMPS: &Table<DirectionJump> = &[
    ("DaCapo", DirectionJump::DaCapo),
    ("DaCapoAlFine", DirectionJump::DaCapoAlFine),
    ("DaCapoAlCoda", DirectionJump::DaCapoAlCoda),
    ("DaCapoAlDoubleCoda", DirectionJump::DaCapoAlDoubleCoda),
    ("DaSegno", DirectionJump::DaSegno),
    ("DaSegnoAlFine", DirectionJump::DaSegnoAlFine),
    ("DaSegnoAlCoda", DirectionJump::DaSegnoAlCoda),
    ("DaSegnoAlDoubleCoda", DirectionJump::DaSegnoAlDoubleCoda),
    ("DaSegnoSegno", DirectionJump::DaSegnoSegno),
    ("DaSegnoSegnoAlFine", DirectionJump::DaSegnoSegnoAlFine),
    ("DaSegnoSegnoAlCoda", DirectionJump::DaSegnoSegnoAlCoda),
    (
        "DaSegnoSegnoAlDoubleCoda",
        DirectionJump::DaSegnoSegnoAlDoubleCoda,
    ),
    ("DaCoda", DirectionJump::DaCoda),
    ("DaDoubleCoda", DirectionJump::DaDoubleCoda),
];

pub(crate) const MASTER_BAR_ORDER: [&str; 11] = [
    "Key",
    "Time",
    "Fermatas",
    "Repeat",
    "Section",
    "AlternateEndings",
    "TripletFeel",
    "DoubleBar",
    "Directions",
    "Bars",
    "XProperties",
];

pub(crate) const NOTE_ORDER: [&str; 11] = [
    "Trill",
    "Vibrato",
    "AntiAccent",
    "LetRing",
    "Accent",
    "Tie",
    "LeftFingering",
    "RightFingering",
    "InstrumentArticulation",
    "Properties",
    "XProperties",
];

pub(crate) const TRIPLET_FEELS: &Table<TripletFeel> = &[
    ("None", TripletFeel::None),
    ("Triplet8th", TripletFeel::TripletEighth),
    ("Triplet16th", TripletFeel::TripletSixteenth),
    ("Dotted8th", TripletFeel::DottedEighth),
    ("Dotted16th", TripletFeel::DottedSixteenth),
    ("Scottish8th", TripletFeel::ScottishEighth),
    ("Scottish16th", TripletFeel::ScottishSixteenth),
];

pub(crate) const TRIPLET_FEEL: &str = "TripletFeel";

pub(crate) const DIRECTION_TARGET: &str = "Target";
pub(crate) const DIRECTION_JUMP: &str = "Jump";

pub(crate) const SOUND_AUTOMATION: &str = "Sound";
pub(crate) const VOLUME_AUTOMATION: &str = "DSPParam_12";
pub(crate) const PAN_AUTOMATION: &str = "DSPParam_11";
pub(crate) const VOLUME_PARAMETER: usize = 12;
pub(crate) const PAN_PARAMETER: usize = 11;
pub(crate) const STRIP_DEFAULTS: [&str; 16] = [
    "0.5", "0.5", "0.5", "0.5", "0.5", "0.5", "0.5", "0.5", "0.5", "0", "0.5", "0.5", "0.5", "0.5",
    "0.5", "0.5",
];
pub(crate) const GENERATED_ROLE: &str = "User";
pub(crate) const SOUND_KEY_SEPARATOR: char = ';';
pub(crate) const FADE_IN: &str = "FadeIn";
pub(crate) const GHOST: &str = "Normal";
pub(crate) const MINOR: &str = "Minor";
pub(crate) const BRUSH_DURATION: &str = "687935489";
pub(crate) const BRUSH_START_TIME: &str = "687935490";
pub(crate) const TEMPO_POSITION_GRID: i64 = 3840;
pub(crate) const TRILL_SPEED: &str = "688062467";
pub(crate) const DEFAULT_TRILL_TICKS: u16 = 60;
pub(crate) const TRILL_SPEEDS: &[(u16, NoteValue)] = &[
    (60, NoteValue::ThirtySecond),
    (120, NoteValue::Sixteenth),
    (240, NoteValue::Eighth),
    (480, NoteValue::Quarter),
];
pub(crate) const WHAMMY_ATTRIBUTES: [&str; 7] = [
    "originValue",
    "middleValue",
    "destinationValue",
    "originOffset",
    "middleOffset1",
    "middleOffset2",
    "destinationOffset",
];

pub(crate) fn accent_bits(accent: Option<Accent>, staccato: bool) -> u32 {
    let accent = match accent {
        Some(Accent::Normal) => ACCENT_BIT,
        Some(Accent::Heavy) => HEAVY_ACCENT_BIT,
        None => 0,
    };
    if staccato {
        accent | STACCATO_BIT
    } else {
        accent
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_slide_has_its_own_bit() {
        let expected = [
            (Slide::Shift, 1),
            (Slide::Legato, 2),
            (Slide::OutDownwards, 4),
            (Slide::OutUpwards, 8),
            (Slide::IntoFromBelow, 16),
            (Slide::IntoFromAbove, 32),
        ];
        for (slide, bits) in expected {
            assert_eq!(slide_bits(slide), bits, "{slide:?}");
        }
    }

    #[test]
    fn accent_and_staccato_bits_combine() {
        assert_eq!(accent_bits(None, false), 0);
        assert_eq!(accent_bits(None, true), 1);
        assert_eq!(accent_bits(Some(Accent::Normal), false), 8);
        assert_eq!(accent_bits(Some(Accent::Normal), true), 9);
        assert_eq!(accent_bits(Some(Accent::Heavy), false), 4);
        assert_eq!(accent_bits(Some(Accent::Heavy), true), 5);
    }
}
