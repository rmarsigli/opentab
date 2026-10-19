use opentab_model::{
    AutomationValue, Balance, BendPoint, Directions, DrumArticulation, DrumKit, Fraction,
    InstrumentSet, KeySignature, Level, MidiBank, NoteKind, Pitch, TimeSignature, TremoloBar,
    TrillSpeed,
};

use crate::codes::{
    DIRECTION_JUMP, DIRECTION_JUMPS, DIRECTION_TARGET, DIRECTION_TARGETS, DRUM_ARTICULATION,
    DRUM_ARTICULATION_FIELDS, DRUM_ELEMENT, DRUM_ELEMENT_FIELDS, DRUM_ELEMENTS, INSTRUMENT_SETS,
    PAN_AUTOMATION, PAN_PARAMETER, SOUND_AUTOMATION, SOUND_KEY_SEPARATOR, STRIP_DEFAULTS,
    TECHNIQUE_PLACEMENTS, TEMPO_POSITION_GRID, TRILL_SPEED, TRILL_SPEEDS, VOLUME_AUTOMATION,
    VOLUME_PARAMETER, WHAMMY_ATTRIBUTES, name_of,
};
use crate::dom::{self, Element, Node};
use crate::guard::PitchInputs;
use crate::number::{self, Decimal};
use crate::read::Handled;

pub(crate) const GP_REVISION: &str = "GPRevision";
pub(crate) const GP_VERSION: &str = "GPVersion";
const VERSION: &str = "8.1.5";
const REVISION: &str = "13008";
const REVISION_REQUIRED: &str = "12024";
const REVISION_RECOMMENDED: &str = "13000";
const DIAGRAM_FRETS: &str = "5";
const MAX_MIDI: i32 = 127;
const SEMITONES: i32 = 12;
const FIFTH: i32 = 7;
const SIGNIFICANT_DIGITS: u32 = 6;
const MAX_SIGNIFICANT_DIGITS: u32 = 12;
pub(crate) const QUARTERS_PER_WHOLE: i64 = 4;
const MILLIONTHS: u32 = 1_000_000;
const MILLIONTH_DIGITS: usize = 6;
const SCIENTIFIC_BELOW: u32 = 5;
const SHARP_NAMES: [(&str, &str); 12] = [
    ("C", ""),
    ("C", "#"),
    ("D", ""),
    ("D", "#"),
    ("E", ""),
    ("F", ""),
    ("F", "#"),
    ("G", ""),
    ("G", "#"),
    ("A", ""),
    ("A", "#"),
    ("B", ""),
];
const FLAT_NAMES: [(&str, &str); 12] = [
    ("C", ""),
    ("D", "b"),
    ("D", ""),
    ("E", "b"),
    ("E", ""),
    ("F", ""),
    ("G", "b"),
    ("G", ""),
    ("A", "b"),
    ("A", ""),
    ("B", "b"),
    ("B", ""),
];

pub(crate) const MIDI: &str = "Midi";
pub(crate) const CONCERT_PITCH: &str = "ConcertPitch";
pub(crate) const TRANSPOSED_PITCH: &str = "TransposedPitch";

pub(crate) fn gp_version() -> Element {
    Element::leaf(GP_VERSION, VERSION)
}

pub(crate) fn gp_revision() -> Element {
    Element::leaf(GP_REVISION, REVISION)
        .with_attribute("required", REVISION_REQUIRED)
        .with_attribute("recommended", REVISION_RECOMMENDED)
}

pub(crate) fn directions(directions: &Directions) -> Option<Element> {
    if directions.is_empty() {
        return None;
    }
    let mut element = Element::new("Directions");
    for target in &directions.targets {
        element.push(Element::leaf(
            DIRECTION_TARGET,
            name_of(DIRECTION_TARGETS, *target),
        ));
    }
    for jump in &directions.jumps {
        element.push(Element::leaf(
            DIRECTION_JUMP,
            name_of(DIRECTION_JUMPS, *jump),
        ));
    }
    Some(element)
}

pub(crate) fn halves(value: i16) -> String {
    let sign = if value < 0 { "-" } else { "" };
    let magnitude = value.unsigned_abs();
    let fraction = if magnitude % 2 == 1 { "5" } else { "0" };
    format!("{sign}{}.{fraction}00000", magnitude / 2)
}

pub(crate) fn offset(point: BendPoint) -> String {
    format!("{}.000000", point.offset())
}

pub(crate) fn whammy(bar: TremoloBar) -> Element {
    let values = [
        halves(bar.origin().value()),
        halves(bar.middle_value()),
        halves(bar.destination().value()),
        offset(bar.origin()),
        offset(bar.middle_start()),
        offset(bar.middle_end()),
        offset(bar.destination()),
    ];
    let mut element = Element::new("Whammy");
    for (name, value) in WHAMMY_ATTRIBUTES.iter().zip(values) {
        element = element.with_attribute(name, &value);
    }
    element
}

pub(crate) fn drum_string(string: usize) -> Element {
    Element::property("String", Element::leaf("String", &string.to_string()))
}

pub(crate) fn trill(pitch: u8) -> Element {
    Element::leaf("Trill", &pitch.to_string())
}

pub(crate) fn trill_ticks(speed: TrillSpeed) -> Option<u16> {
    TRILL_SPEEDS
        .iter()
        .find(|(_, value)| *value == speed.value())
        .map(|(ticks, _)| *ticks)
}

pub(crate) fn trill_speed(ticks: u16) -> Element {
    Element::new("XProperty")
        .with_attribute("id", TRILL_SPEED)
        .with_child(Element::leaf("Int", &ticks.to_string()))
}

pub(crate) fn tempo_linear() -> Element {
    Element::leaf("Linear", "true")
}

pub(crate) fn instrument_type(set: InstrumentSet) -> Element {
    Element::leaf("Type", name_of(INSTRUMENT_SETS, set))
}

fn drum_articulation(articulation: &DrumArticulation) -> Element {
    let keys: Vec<String> = articulation.input_keys.iter().map(u8::to_string).collect();
    let values = [
        articulation.name.clone(),
        articulation.staff_line.to_string(),
        articulation.noteheads.join(" "),
        name_of(TECHNIQUE_PLACEMENTS, articulation.technique_placement).to_string(),
        articulation.technique_symbol.clone(),
        keys.join(" "),
        articulation.rse_sound.clone(),
        articulation.output_key.to_string(),
    ];
    let mut element = Element::new(DRUM_ARTICULATION);
    for (field, value) in DRUM_ARTICULATION_FIELDS.iter().zip(values) {
        element.push(Element::leaf(field, &value));
    }
    element
}

pub(crate) fn drum_elements(kit: &DrumKit) -> Element {
    let [name, kind, soundbank, list] = DRUM_ELEMENT_FIELDS;
    let mut elements = Element::new(DRUM_ELEMENTS);
    let mut articulations = kit.articulations().iter();
    for group in kit.elements() {
        let mut held = Element::new(list);
        for articulation in articulations.by_ref().take(group.count) {
            held.push(drum_articulation(articulation));
        }
        elements.push(
            Element::new(DRUM_ELEMENT)
                .with_child(Element::leaf(name, &group.name))
                .with_child(Element::leaf(kind, &group.kind))
                .with_child(Element::leaf(soundbank, &group.soundbank))
                .with_child(held),
        );
    }
    elements
}

pub(crate) fn sound_midi(program: u8, bank: MidiBank, full: bool) -> Element {
    let mut midi = Element::new("MIDI");
    if full || bank != MidiBank::default() {
        midi.push(Element::leaf("LSB", &bank.lsb().to_string()));
        midi.push(Element::leaf("MSB", &bank.msb().to_string()));
    }
    midi.push(Element::leaf("Program", &program.to_string()));
    midi
}

pub(crate) fn bare_sounds(program: u8, bank: MidiBank) -> Element {
    Element::new("Sounds")
        .with_child(Element::new("Sound").with_child(sound_midi(program, bank, false)))
}

pub(crate) fn millionths(value: u32) -> String {
    let whole = value / MILLIONTHS;
    let rest = value % MILLIONTHS;
    if rest == 0 {
        return whole.to_string();
    }
    let digits = format!("{rest:0width$}", width = MILLIONTH_DIGITS);
    format!("{whole}.{}", digits.trim_end_matches('0'))
}

pub(crate) struct Mixer {
    pub volume: Level,
    pub pan: Balance,
}

pub(crate) fn mixer(parameters: &str) -> Mixer {
    let tokens: Vec<&str> = parameters.split_whitespace().collect();
    let value = |at: usize| {
        tokens
            .get(at)
            .and_then(|token| number::rounded_millionths(token))
    };
    Mixer {
        volume: value(VOLUME_PARAMETER)
            .and_then(|value| Level::new(value).ok())
            .unwrap_or(Level::HALF),
        pan: value(PAN_PARAMETER)
            .and_then(|value| Balance::new(value).ok())
            .unwrap_or(Balance::CENTRE),
    }
}

pub(crate) fn strip_parameters(volume: Level, pan: Balance) -> Element {
    let mut tokens: Vec<String> = STRIP_DEFAULTS
        .iter()
        .map(|token| token.to_string())
        .collect();
    if let Some(slot) = tokens.get_mut(VOLUME_PARAMETER) {
        *slot = millionths(volume.millionths());
    }
    if let Some(slot) = tokens.get_mut(PAN_PARAMETER) {
        *slot = millionths(pan.millionths());
    }
    Element::leaf("Parameters", &tokens.join(" "))
}

pub(crate) fn sound_key(path: &str, name: &str, role: &str) -> String {
    format!("{path}{SOUND_KEY_SEPARATOR}{name}{SOUND_KEY_SEPARATOR}{role}")
}

pub(crate) fn sound_value(key: &str) -> Element {
    let mut element = Element::new("Value");
    element.children = dom::cdata_nodes(key);
    element
}

pub(crate) struct AutomationParts<'a> {
    pub kind: &'a str,
    pub linear: bool,
    pub bar: usize,
    pub position: String,
    pub visible: bool,
    pub value: Element,
}

pub(crate) fn automation(parts: AutomationParts<'_>) -> Element {
    Element::new("Automation")
        .with_child(Element::leaf("Type", parts.kind))
        .with_child(Element::leaf("Linear", &parts.linear.to_string()))
        .with_child(Element::leaf("Bar", &parts.bar.to_string()))
        .with_child(Element::leaf("Position", &parts.position))
        .with_child(Element::leaf("Visible", &parts.visible.to_string()))
        .with_child(parts.value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PositionUnit {
    Bar,
    Quarter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AutomationKind {
    Volume,
    Pan,
    Sound,
}

pub(crate) fn automation_kind(name: &str) -> Option<AutomationKind> {
    match name {
        VOLUME_AUTOMATION => Some(AutomationKind::Volume),
        PAN_AUTOMATION => Some(AutomationKind::Pan),
        SOUND_AUTOMATION => Some(AutomationKind::Sound),
        _ => None,
    }
}

pub(crate) fn kind_of(value: &AutomationValue) -> AutomationKind {
    match value {
        AutomationValue::Volume(_) => AutomationKind::Volume,
        AutomationValue::Pan(_) => AutomationKind::Pan,
        AutomationValue::Sound(_) => AutomationKind::Sound,
    }
}

pub(crate) fn position_unit(kind: AutomationKind) -> PositionUnit {
    match kind {
        AutomationKind::Volume | AutomationKind::Pan => PositionUnit::Bar,
        AutomationKind::Sound => PositionUnit::Quarter,
    }
}

pub(crate) fn automation_position(
    position: Fraction,
    time: TimeSignature,
    unit: PositionUnit,
) -> Option<String> {
    let ticks = grid_ticks(position)?;
    let bar = grid_ticks(time.bar_length())?;
    if ticks > bar {
        return None;
    }
    let per = match unit {
        PositionUnit::Bar => bar,
        PositionUnit::Quarter => TEMPO_POSITION_GRID / QUARTERS_PER_WHOLE,
    };
    grid_text(ticks, per)
}

pub(crate) fn capo_property(capo: u8) -> Element {
    Element::property("CapoFret", Element::leaf("Fret", &capo.to_string()))
}

pub(crate) fn tuning_property(strings: &[Pitch]) -> Element {
    let pitches: Vec<String> = strings
        .iter()
        .rev()
        .map(|pitch| pitch.0.to_string())
        .collect();
    Element::property("Tuning", Element::leaf("Pitches", &pitches.join(" ")))
}

pub(crate) fn chord(id: &str) -> Element {
    let mut element = Element::new("Chord");
    element.children = dom::cdata_nodes(id);
    element
}

pub(crate) fn grid_ticks(position: Fraction) -> Option<i64> {
    let scaled = i128::from(*position.numer()).checked_mul(i128::from(TEMPO_POSITION_GRID))?;
    let denom = i128::from(*position.denom());
    if denom <= 0 || scaled < 0 || scaled % denom != 0 {
        return None;
    }
    i64::try_from(scaled / denom).ok()
}

pub(crate) fn tempo_position(position: Fraction, time: TimeSignature) -> Option<String> {
    let ticks = grid_ticks(position)?;
    let bar = grid_ticks(time.bar_length())?;
    if ticks >= bar {
        return None;
    }
    grid_text(ticks, bar)
}

fn grid_text(ticks: i64, per: i64) -> Option<String> {
    let numer = u128::try_from(ticks).ok()?;
    let denom = u128::try_from(per).ok()?;
    let wanted = Fraction::from_integer(ticks);
    (SIGNIFICANT_DIGITS..=MAX_SIGNIFICANT_DIGITS).find_map(|digits| {
        let text = significant(numer, denom, digits)?;
        let back = Decimal::parse(&text)?.round_to_grid(per, 1, 1)?;
        (back == wanted).then_some(text)
    })
}

fn decimal_shift(numer: u128, denom: u128) -> u32 {
    (0u32..)
        .find(|shift| numer * 10u128.pow(*shift) >= denom)
        .unwrap_or_default()
}

fn scaled_round(numer: u128, denom: u128, places: i32) -> Option<u128> {
    let power = 10u128.checked_pow(places.unsigned_abs())?;
    let (top, bottom) = if places >= 0 {
        (numer.checked_mul(power)?, denom)
    } else {
        (numer, denom.checked_mul(power)?)
    };
    top.checked_mul(2)?
        .checked_add(bottom)?
        .checked_div(bottom.checked_mul(2)?)
}

fn significant(numer: u128, denom: u128, digits: u32) -> Option<String> {
    if numer == 0 || denom == 0 {
        return Some("0".to_string());
    }
    let whole = numer / denom;
    let mut exponent = if whole > 0 {
        i32::try_from(whole.to_string().len()).ok()? - 1
    } else {
        -i32::try_from(decimal_shift(numer, denom)).ok()?
    };
    let places = i32::try_from(digits).ok()? - 1 - exponent;
    let mut mantissa = scaled_round(numer, denom, places)?;
    if mantissa >= 10u128.checked_pow(digits)? {
        mantissa /= 10;
        exponent += 1;
    }
    let text = mantissa.to_string();
    if exponent <= -i32::try_from(SCIENTIFIC_BELOW).ok()? {
        let (head, tail) = text.split_at(1);
        let tail = tail.trim_end_matches('0');
        let power = exponent.unsigned_abs();
        return Some(if tail.is_empty() {
            format!("{head}e-{power:02}")
        } else {
            format!("{head}.{tail}e-{power:02}")
        });
    }
    if exponent < 0 {
        let zeros = "0".repeat(usize::try_from(-exponent - 1).ok()?);
        return Some(format!("0.{zeros}{}", text.trim_end_matches('0')));
    }
    let integer = usize::try_from(exponent).ok()? + 1;
    if integer >= text.len() {
        let zeros = "0".repeat(integer - text.len());
        return Some(format!("{text}{zeros}"));
    }
    let (head, tail) = text.split_at(integer);
    let tail = tail.trim_end_matches('0');
    Some(if tail.is_empty() {
        head.to_string()
    } else {
        format!("{head}.{tail}")
    })
}

#[cfg(test)]
fn general_format(numer: u128, denom: u128) -> String {
    significant(numer, denom, SIGNIFICANT_DIGITS).unwrap_or_default()
}

pub(crate) fn sounding(inputs: &PitchInputs<'_>) -> Option<i32> {
    let NoteKind::Fretted { string, fret } = inputs.kind else {
        return None;
    };
    let open = inputs.tuning.get(usize::from(string))?;
    let midi = i32::from(open.0) + i32::from(inputs.capo) + i32::from(fret);
    (0..=MAX_MIDI).contains(&midi).then_some(midi)
}

fn written_key(key: KeySignature, chromatic: i8) -> i32 {
    let semitones = (-i32::from(chromatic)).rem_euclid(SEMITONES);
    let up = i32::from(key.accidentals()) + (semitones * FIFTH).rem_euclid(SEMITONES);
    let down = up - SEMITONES;
    if up.abs() <= down.abs() { up } else { down }
}

fn pitch(midi: i32, sharps: bool) -> Element {
    let names = if sharps { &SHARP_NAMES } else { &FLAT_NAMES };
    let class = usize::try_from(midi.rem_euclid(SEMITONES)).unwrap_or_default();
    let (step, accidental) = names.get(class).copied().unwrap_or(("C", ""));
    let octave = midi.div_euclid(SEMITONES);
    Element::new("Pitch")
        .with_child(Element::leaf("Step", step))
        .with_child(Element::leaf("Accidental", accidental))
        .with_child(Element::leaf("Octave", &octave.to_string()))
}

pub(crate) fn pitch_property(
    name: &str,
    inputs: &PitchInputs<'_>,
    key: KeySignature,
) -> Option<Element> {
    let midi = sounding(inputs)?;
    let value = match name {
        MIDI => Element::leaf("Number", &midi.to_string()),
        CONCERT_PITCH => pitch(midi, key.accidentals() >= 0),
        TRANSPOSED_PITCH => {
            let transpose = inputs.transpose;
            let offset =
                i32::from(transpose.chromatic()) + SEMITONES * i32::from(transpose.octave());
            let sharps = written_key(key, transpose.chromatic()) >= 0;
            pitch(midi - offset, sharps)
        }
        _ => return None,
    };
    Some(Element::property(name, value))
}

fn empty_diagram(strings: usize) -> Element {
    Element::new("Diagram")
        .with_attribute("stringCount", &strings.to_string())
        .with_attribute("fretCount", DIAGRAM_FRETS)
}

pub(crate) fn chord_item(id: &str, name: &str, strings: usize) -> Element {
    Element::new("Item")
        .with_attribute("id", id)
        .with_attribute("name", name)
        .with_child(empty_diagram(strings))
}

fn is_generated_item(node: &Node, strings: usize) -> bool {
    let Node::Element(item) = node else {
        return false;
    };
    let diagram = empty_diagram(strings);
    item.name == "Item"
        && item.attributes.len() == 2
        && item.attribute("id").is_some()
        && item.attribute("name").is_some()
        && item.children.len() == 1
        && matches!(item.children.first(), Some(Node::Element(child)) if *child == diagram)
}

pub(crate) fn diagram_collection(items: Vec<Element>) -> Element {
    let mut list = Element::new("Items");
    list.children = items.into_iter().map(Node::Element).collect();
    Element::property("DiagramCollection", list)
}

pub(crate) fn strip_generated_items(collection: &Element, strings: usize) -> Handled {
    let generated_item = |node: &Node| is_generated_item(node, strings);
    let Some((index, items)) = collection.child("Items") else {
        return Handled::Unknown;
    };
    if !items.children.iter().any(generated_item) {
        return Handled::Unknown;
    }
    let generated = collection.attributes.len() == 1
        && collection.children.len() == 1
        && items.attributes.is_empty()
        && items.children.iter().all(generated_item);
    if generated {
        return Handled::Mapped(&["name"]);
    }
    let mut filtered = collection.clone();
    if let Some(Node::Element(list)) = filtered.children.get_mut(index) {
        list.children.retain(|node| !generated_item(node));
    }
    Handled::Substitute(Node::Element(filtered))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(accidentals: i8) -> Vec<i32> {
        let key = KeySignature::new(accidentals, false).unwrap();
        (0..12).map(|up| written_key(key, -up)).collect()
    }

    #[test]
    fn grid_ticks_accept_only_whole_non_negative_ticks() {
        let ticks = |numer, denom| grid_ticks(Fraction::new(numer, denom));
        assert_eq!(ticks(0, 1), Some(0));
        assert_eq!(ticks(1, 2), Some(1920));
        assert_eq!(ticks(1, 3), Some(1280));
        assert_eq!(ticks(3, 4), Some(2880));
        assert_eq!(ticks(1, 7), None);
        assert_eq!(ticks(-1, 2), None);
        assert_eq!(ticks(-1, 4), None);
    }

    #[test]
    fn the_decimal_shift_is_the_digits_before_the_first_significant_one() {
        assert_eq!(decimal_shift(1, 1), 0);
        assert_eq!(decimal_shift(2, 1), 0);
        assert_eq!(decimal_shift(9, 10), 1);
        assert_eq!(decimal_shift(1, 10), 1);
        assert_eq!(decimal_shift(1, 11), 2);
        assert_eq!(decimal_shift(1, 100), 2);
        assert_eq!(decimal_shift(1, 101), 3);
        assert_eq!(decimal_shift(3, 1000), 3);
        assert_eq!(decimal_shift(1, 100_000), 5);
    }

    #[test]
    fn general_format_writes_six_significant_digits() {
        let formatted = |numer, denom| general_format(numer, denom);
        assert_eq!(formatted(0, 5), "0");
        assert_eq!(formatted(5, 0), "0");
        assert_eq!(formatted(1, 2), "0.5");
        assert_eq!(formatted(1, 3), "0.333333");
        assert_eq!(formatted(2, 3), "0.666667");
        assert_eq!(formatted(1, 10), "0.1");
        assert_eq!(formatted(1, 100), "0.01");
        assert_eq!(formatted(1, 10_000), "0.0001");
        assert_eq!(formatted(1, 100_000), "1e-05");
        assert_eq!(formatted(3, 200_000), "1.5e-05");
        assert_eq!(formatted(1, 3_000_000), "3.33333e-07");
    }

    #[test]
    fn general_format_keeps_six_significant_digits_above_one() {
        let formatted = |numer, denom| general_format(numer, denom);
        assert_eq!(formatted(1, 1), "1");
        assert_eq!(formatted(4, 1), "4");
        assert_eq!(formatted(21, 2), "10.5");
        assert_eq!(formatted(10, 1), "10");
        assert_eq!(formatted(1000, 3), "333.333");
        assert_eq!(formatted(1020, 1), "1020");
        assert_eq!(formatted(9_999_996, 10), "1000000");
        assert_eq!(formatted(123_456_789, 1), "123457000");
    }

    #[test]
    fn grid_text_adds_digits_until_the_tick_comes_back() {
        assert_eq!(grid_text(320, 960), Some("0.333333".to_string()));
        assert_eq!(grid_text(960, 960), Some("1".to_string()));
        assert_eq!(grid_text(0, 960), Some("0".to_string()));
        let large = 1000 * 960 + 1;
        let text = grid_text(large, 960).expect("text");
        assert_eq!(text, "1000.001");
        for ticks in (0..=979_200).step_by(97) {
            let text = grid_text(ticks, 979_200).expect("bar text");
            let back = Decimal::parse(&text)
                .and_then(|decimal| decimal.round_to_grid(979_200, 1, 1))
                .expect("parses");
            assert_eq!(back, Fraction::from_integer(ticks), "{text}");
        }
        for ticks in (0..=979_200).step_by(89) {
            let text = grid_text(ticks, 960).expect("quarter text");
            let back = Decimal::parse(&text)
                .and_then(|decimal| decimal.round_to_grid(960, 1, 1))
                .expect("parses");
            assert_eq!(back, Fraction::from_integer(ticks), "{text}");
        }
    }

    #[test]
    fn rounding_up_to_a_power_of_ten_drops_one_decimal_place() {
        assert_eq!(general_format(9_999_996, 10_000_000), "1");
        assert_eq!(general_format(9_999_996, 100_000_000), "0.1");
        assert_eq!(general_format(99_999_996, 100_000_000_000_000), "1e-06");
    }

    #[test]
    fn the_written_key_follows_the_circle_of_fifths() {
        assert_eq!(keys(0), [0, -5, 2, -3, 4, -1, 6, 1, -4, 3, -2, 5]);
        assert_eq!(keys(3), [3, -2, 5, 0, -5, 2, -3, 4, -1, 6, 1, -4]);
        assert_eq!(keys(-2), [-2, 5, 0, -5, 2, -3, 4, -1, 6, 1, -4, 3]);
    }

    #[test]
    fn the_written_key_ignores_whole_octaves_of_transposition() {
        let key = KeySignature::new(1, false).unwrap();
        for chromatic in -11i8..=11 {
            assert_eq!(
                written_key(key, chromatic),
                written_key(key, chromatic - 12)
            );
        }
    }
}
