use opentab_model::{DrumArticulation, DrumElement, DrumKit, TechniquePlacement};

use crate::canon;
use crate::codes::{
    DRUM_ARTICULATION, DRUM_ARTICULATION_FIELDS, DRUM_ELEMENT, DRUM_ELEMENT_FIELDS,
    TECHNIQUE_PLACEMENTS, lookup,
};
use crate::dom::Element;
use crate::number;

const DEFAULT_KIND: &str = "unknown";
const DEFAULT_NOTEHEADS: [&str; 3] = ["noteheadBlack", "noteheadHalf", "noteheadWhole"];
const DEFAULT_KEY: u8 = 0;
const DEFAULT_LINE: i8 = 0;

pub(crate) enum KitRead {
    Canonical(DrumKit),
    Lenient(DrumKit),
    Untyped,
}

fn field(element: &Element, name: &str) -> Option<String> {
    element.child(name).map(|(_, child)| child.text())
}

fn number_or<T: std::str::FromStr>(text: Option<String>, default: T) -> Option<T> {
    match text {
        Some(text) => number::integer(&text),
        None => Some(default),
    }
}

fn articulation(element: &Element) -> Option<DrumArticulation> {
    let [
        name,
        staff_line,
        noteheads,
        placement,
        symbol,
        inputs,
        rse_sound,
        output,
    ] = DRUM_ARTICULATION_FIELDS.map(|name| field(element, name));
    let heads: Vec<String> = match noteheads {
        Some(text) => text.split_whitespace().map(str::to_string).collect(),
        None => DEFAULT_NOTEHEADS.map(str::to_string).to_vec(),
    };
    let input_keys = inputs
        .unwrap_or_default()
        .split_whitespace()
        .map(number::integer::<u8>)
        .collect::<Option<Vec<u8>>>()?;
    let technique_placement = match placement {
        Some(text) => lookup(TECHNIQUE_PLACEMENTS, &text)?,
        None => TechniquePlacement::default(),
    };
    Some(DrumArticulation {
        name: name.unwrap_or_default(),
        staff_line: number_or(staff_line, DEFAULT_LINE)?,
        noteheads: heads.try_into().ok()?,
        technique_placement,
        technique_symbol: symbol.unwrap_or_default(),
        input_keys,
        rse_sound: rse_sound.unwrap_or_default(),
        output_key: number_or(output, DEFAULT_KEY)?,
    })
}

fn parse(elements: &Element) -> Option<DrumKit> {
    let [name, kind, soundbank, list] = DRUM_ELEMENT_FIELDS;
    let mut groups = Vec::new();
    let mut articulations = Vec::new();
    for (_, element) in elements.elements() {
        if element.name != DRUM_ELEMENT {
            return None;
        }
        let before = articulations.len();
        if let Some((_, held)) = element.child(list) {
            for (_, child) in held.elements() {
                if child.name != DRUM_ARTICULATION {
                    return None;
                }
                articulations.push(articulation(child)?);
            }
        }
        groups.push(DrumElement {
            name: field(element, name).unwrap_or_default(),
            kind: field(element, kind).unwrap_or_else(|| DEFAULT_KIND.to_string()),
            soundbank: field(element, soundbank).unwrap_or_default(),
            count: articulations.len() - before,
        });
    }
    DrumKit::new(groups, articulations).ok()
}

pub(crate) fn read_kit(elements: &Element) -> KitRead {
    match parse(elements) {
        Some(kit) if canon::drum_elements(&kit) == *elements => KitRead::Canonical(kit),
        Some(kit) => KitRead::Lenient(kit),
        None => KitRead::Untyped,
    }
}
