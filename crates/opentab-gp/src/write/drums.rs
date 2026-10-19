use std::collections::BTreeSet;

use crate::dom::Element;

const PROPERTIES: &str = "Properties";
const STRING: &str = "String";

fn kept_string(note: &Element) -> Option<usize> {
    let (_, properties) = note.child(PROPERTIES)?;
    properties
        .elements()
        .find(|(_, property)| property.key() == "Property@String")
        .and_then(|(_, property)| property.child(STRING))
        .and_then(|(_, string)| string.text().trim().parse().ok())
}

pub(crate) fn drum_strings(notes: &[Element]) -> Vec<Option<usize>> {
    let kept: Vec<Option<usize>> = notes.iter().map(kept_string).collect();
    let mut used: BTreeSet<usize> = kept.iter().flatten().copied().collect();
    let mut strings = Vec::with_capacity(notes.len());
    for (index, string) in kept.iter().enumerate() {
        if string.is_some() {
            strings.push(None);
            continue;
        }
        let free = if used.contains(&index) {
            (0..)
                .find(|candidate| !used.contains(candidate))
                .unwrap_or(index)
        } else {
            index
        };
        used.insert(free);
        strings.push(Some(free));
    }
    strings
}
