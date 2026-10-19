use opentab_model::{DrumArticulation, DrumElement, DrumKit};

use crate::canon;
use crate::codes::{DRUM_ARTICULATION_FIELDS, DRUM_ELEMENT_FIELDS, TECHNIQUE_PLACEMENTS, lookup};
use crate::dom::Element;
use crate::number;

fn field(element: &Element, name: &str) -> Option<String> {
    element.child(name).map(|(_, child)| child.text())
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
    let heads: Vec<String> = noteheads?.split_whitespace().map(str::to_string).collect();
    let input_keys = inputs?
        .split_whitespace()
        .map(number::integer::<u8>)
        .collect::<Option<Vec<u8>>>()?;
    Some(DrumArticulation {
        name: name?,
        staff_line: number::integer(&staff_line?)?,
        noteheads: heads.try_into().ok()?,
        technique_placement: lookup(TECHNIQUE_PLACEMENTS, &placement?)?,
        technique_symbol: symbol?,
        input_keys,
        rse_sound: rse_sound?,
        output_key: number::integer(&output?)?,
    })
}

fn parse(elements: &Element) -> Option<DrumKit> {
    let [name, kind, soundbank, list] = DRUM_ELEMENT_FIELDS;
    let mut groups = Vec::new();
    let mut articulations = Vec::new();
    for (_, element) in elements.elements() {
        let (_, held) = element.child(list)?;
        let before = articulations.len();
        for (_, child) in held.elements() {
            articulations.push(articulation(child)?);
        }
        groups.push(DrumElement {
            name: field(element, name)?,
            kind: field(element, kind)?,
            soundbank: field(element, soundbank)?,
            count: articulations.len() - before,
        });
    }
    DrumKit::new(groups, articulations).ok()
}

pub(crate) fn typed_kit(elements: &Element) -> Option<DrumKit> {
    let kit = parse(elements)?;
    (canon::drum_elements(&kit) == *elements).then_some(kit)
}
