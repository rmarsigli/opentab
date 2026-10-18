use opentab_model::{InstrumentKind, Pitch, Score, Staff, Track};

use crate::canon;
use crate::dom::{Element, Node};
use crate::error::Refusal;
use crate::number;
use crate::read::{ChordItem, chord_items};
use crate::write::payload::{self, Env, Item};
use crate::write::state::{Layouts, Owners, StaffContext};
use crate::write::xml::{self, Fail, Within};

const PROPERTIES: &str = "Properties";

fn staff_property<'i>(items: &'i [Item], key: &str) -> Option<&'i Element> {
    items
        .iter()
        .filter(|item| item.parent == PROPERTIES)
        .filter_map(Item::element)
        .find(|element| element.key() == key)
}

fn malformed(problem: &str) -> Refusal {
    Refusal::MalformedPayload {
        problem: problem.to_string(),
    }
}

fn own_tuning(property: &Element) -> Result<Option<Vec<Pitch>>, Refusal> {
    let Some((_, pitches)) = property.child("Pitches") else {
        return Ok(None);
    };
    let mut tuning = Vec::new();
    for token in pitches.text().split_whitespace() {
        let pitch = number::integer::<u8>(token).ok_or_else(|| malformed("invalid tuning"))?;
        tuning.push(Pitch(pitch));
    }
    tuning.reverse();
    Ok(Some(tuning))
}

fn own_capo(property: &Element) -> Result<Option<u8>, Refusal> {
    let Some((_, fret)) = property.child("Fret") else {
        return Ok(None);
    };
    number::integer::<u8>(&fret.text())
        .map(Some)
        .ok_or_else(|| malformed("invalid capo"))
}

fn staff_context(
    track: &Track,
    track_index: usize,
    staff_index: usize,
    items: &[Item],
) -> Result<StaffContext, Refusal> {
    let mut tuning = track.tuning.strings.clone();
    let mut capo = track.capo;
    if staff_index > 0 {
        if let Some(property) = staff_property(items, "Property@Tuning")
            && let Some(own) = own_tuning(property)?
        {
            tuning = own;
        }
        if let Some(property) = staff_property(items, "Property@CapoFret")
            && let Some(own) = own_capo(property)?
        {
            capo = own;
        }
    }
    let chords: Vec<ChordItem> = staff_property(items, "Property@DiagramCollection")
        .map(chord_items)
        .unwrap_or_default();
    Ok(StaffContext {
        track: track_index,
        staff: staff_index,
        kind: track.instrument.kind,
        tuning,
        capo,
        transpose: track.transpose,
        chords,
    })
}

pub(crate) fn staff_contexts(
    score: &Score,
    owners: &mut Owners<'_>,
) -> Result<Vec<StaffContext>, Fail> {
    let mut contexts = Vec::new();
    for (track_index, track) in score.tracks.iter().enumerate() {
        for (staff_index, staff) in track.staves.iter().enumerate() {
            let items = owners
                .items(staff.id.0)
                .within(|| format!("tracks[{track_index}].staves[{staff_index}]"))?;
            let context = staff_context(track, track_index, staff_index, &items)
                .within(|| format!("tracks[{track_index}].staves[{staff_index}]"))?;
            contexts.push(context);
        }
    }
    Ok(contexts)
}

fn staff_element(
    track: &Track,
    staff: &Staff,
    index: usize,
    owners: &mut Owners<'_>,
) -> Result<Element, Refusal> {
    let mut properties = Element::new(PROPERTIES);
    if index == 0 {
        properties.push(Element::property(
            "CapoFret",
            Element::leaf("Fret", &track.capo.to_string()),
        ));
        if !track.tuning.strings.is_empty() {
            let pitches: Vec<String> = track
                .tuning
                .strings
                .iter()
                .rev()
                .map(|pitch| pitch.0.to_string())
                .collect();
            properties.push(Element::property(
                "Tuning",
                Element::leaf("Pitches", &pitches.join(" ")),
            ));
        }
    }
    let mut element = Element::new("Staff").with_child(properties);
    let items = payload::surviving(owners.items(staff.id.0)?, &Env::Plain)?;
    apply_own(&mut element, items)?;
    Ok(element)
}

pub(crate) fn apply_own(element: &mut Element, items: Vec<Item>) -> Result<(), Refusal> {
    let split = payload::split(items)?;
    if !split.rhythm.is_empty() || !split.tempo.is_empty() {
        return Err(malformed("parent root does not apply here"));
    }
    payload::apply(element, split.own)
}

fn sounds(program: u8) -> Element {
    let midi = Element::new("MIDI").with_child(Element::leaf("Program", &program.to_string()));
    Element::new("Sounds").with_child(Element::new("Sound").with_child(midi))
}

fn track_element(
    track: &Track,
    index: usize,
    layouts: &Layouts,
    owners: &mut Owners<'_>,
) -> Result<Element, Fail> {
    let items = payload::surviving(owners.items(track.id.0)?, &Env::Track(track))?;
    let id = Layouts::id(&layouts.tracks, index);
    let mut element = Element::new("Track").with_attribute("id", &id.to_string());
    element.push(xml::text("Name", &track.name)?);
    element.push(xml::text("ShortName", &track.short_name)?);
    element.push(
        Element::new("InstrumentSet").with_child(canon::instrument_type(track.instrument.kind)),
    );
    element.push(
        Element::new("Transpose")
            .with_child(Element::leaf(
                "Chromatic",
                &track.transpose.chromatic().to_string(),
            ))
            .with_child(Element::leaf(
                "Octave",
                &track.transpose.octave().to_string(),
            )),
    );
    let targets_sounds = items
        .iter()
        .any(|item| item.parent == "Sounds" || item.parent.starts_with("Sounds/"));
    if track.instrument.midi_program != 0 || targets_sounds {
        element.push(sounds(track.instrument.midi_program));
    }
    let mut staves = Element::new("Staves");
    for (staff_index, staff) in track.staves.iter().enumerate() {
        let staff = staff_element(track, staff, staff_index, owners)
            .within(|| format!("staves[{staff_index}]"))?;
        staves.children.push(Node::Element(staff));
    }
    element.push(staves);
    apply_own(&mut element, items)?;
    Ok(element)
}

pub(crate) fn tracks_pool(
    score: &Score,
    layouts: &Layouts,
    owners: &mut Owners<'_>,
) -> Result<Element, Fail> {
    let mut pool = Element::new("Tracks");
    for (index, track) in score.tracks.iter().enumerate() {
        let element =
            track_element(track, index, layouts, owners).within(|| format!("tracks[{index}]"))?;
        pool.push(element);
    }
    Ok(pool)
}

pub(crate) fn check_kind(kind: InstrumentKind, percussion: bool) -> Result<(), Refusal> {
    if (kind == InstrumentKind::Percussion) == percussion {
        Ok(())
    } else {
        Err(Refusal::NoteKind)
    }
}
