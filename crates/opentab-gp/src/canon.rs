use opentab_model::{Fraction, InstrumentKind, KeySignature, NoteKind, TimeSignature};

use crate::codes::{DRUM_KIT, TEMPO_POSITION_GRID};
use crate::dom::{Element, Node};
use crate::guard::PitchInputs;

pub(crate) const GP_REVISION: &str = "GPRevision";
const REVISION: &str = "13008";
const REVISION_REQUIRED: &str = "12024";
const REVISION_RECOMMENDED: &str = "13000";
const FRETTED_TYPE: &str = "electricGuitar";
const MAX_MIDI: i32 = 127;
const SEMITONES: i32 = 12;
const FIFTH: i32 = 7;
const SIGNIFICANT_DIGITS: u32 = 6;
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

pub(crate) fn gp_revision() -> Element {
    Element::leaf(GP_REVISION, REVISION)
        .with_attribute("required", REVISION_REQUIRED)
        .with_attribute("recommended", REVISION_RECOMMENDED)
}

pub(crate) fn instrument_type(kind: InstrumentKind) -> Element {
    let name = match kind {
        InstrumentKind::Fretted => FRETTED_TYPE,
        InstrumentKind::Percussion => DRUM_KIT,
    };
    Element::leaf("Type", name)
}

pub(crate) fn chord(id: &str) -> Element {
    let mut element = Element::new("Chord");
    element.children.push(Node::CData(id.to_string()));
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
    Some(general_format(
        u128::try_from(ticks).ok()?,
        u128::try_from(bar).ok()?,
    ))
}

fn general_format(numer: u128, denom: u128) -> String {
    if numer == 0 || denom == 0 {
        return "0".to_string();
    }
    let mut shift = 0u32;
    while numer * 10u128.pow(shift) < denom {
        shift += 1;
    }
    let scale = 10u128.pow(shift + SIGNIFICANT_DIGITS - 1);
    let mut mantissa = (2 * numer * scale + denom) / (2 * denom);
    if mantissa >= 10u128.pow(SIGNIFICANT_DIGITS) {
        mantissa /= 10;
        shift = shift.saturating_sub(1);
    }
    let digits = mantissa.to_string();
    if shift >= SCIENTIFIC_BELOW {
        let (head, tail) = digits.split_at(1);
        let tail = tail.trim_end_matches('0');
        return if tail.is_empty() {
            format!("{head}e-{shift:02}")
        } else {
            format!("{head}.{tail}e-{shift:02}")
        };
    }
    if shift == 0 {
        let (head, tail) = digits.split_at(1);
        let tail = tail.trim_end_matches('0');
        return if tail.is_empty() {
            head.to_string()
        } else {
            format!("{head}.{tail}")
        };
    }
    let zeros = "0".repeat(usize::try_from(shift - 1).unwrap_or_default());
    format!("0.{zeros}{}", digits.trim_end_matches('0'))
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
