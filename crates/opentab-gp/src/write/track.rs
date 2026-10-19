use opentab_model::{InstrumentKind, MasterBar, Pitch, Score, Staff, Track};

use crate::canon;
use crate::dom::{Element, Node};
use crate::error::Refusal;
use crate::number;
use crate::read::{ChordItem, chord_items};
use crate::write::mixer::{
    check_track, lanes, midi_connection, patch_parameters, rse_element, sounds_element,
};
use crate::write::payload::{self, Content, Env, Item};
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
        generated_chords: Vec::new(),
    })
}

fn next_chord_id(chords: &[ChordItem], after: &mut u64) -> String {
    loop {
        let id = after.to_string();
        *after = after.saturating_add(1);
        if !chords.iter().any(|item| item.id == id) {
            return id;
        }
    }
}

fn generate_chords(context: &mut StaffContext, staff: &Staff) -> Result<(), Fail> {
    let mut after = context
        .chords
        .iter()
        .filter_map(|item| number::integer::<u64>(&item.id))
        .max()
        .map_or(0, |highest| highest.saturating_add(1));
    for (bar_index, bar) in staff.bars.iter().enumerate() {
        for (voice_index, voice) in bar.voices.iter().enumerate() {
            for (beat_index, beat) in voice.beats.iter().enumerate() {
                let Some(name) = &beat.effects.chord_name else {
                    continue;
                };
                if context.chords.iter().any(|item| item.name == *name) {
                    continue;
                }
                xml::check_escaped(name).within(|| {
                    format!(
                        "bars[{bar_index}].voices[{voice_index}].beats[{beat_index}].effects.chord_name"
                    )
                })?;
                let item = ChordItem {
                    id: next_chord_id(&context.chords, &mut after),
                    name: name.clone(),
                };
                context.chords.push(ChordItem {
                    id: item.id.clone(),
                    name: item.name.clone(),
                });
                context.generated_chords.push(item);
            }
        }
    }
    Ok(())
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
            let mut context = staff_context(track, track_index, staff_index, &items)
                .within(|| format!("tracks[{track_index}].staves[{staff_index}]"))?;
            generate_chords(&mut context, staff)
                .within(|| format!("tracks[{track_index}].staves[{staff_index}]"))?;
            contexts.push(context);
        }
    }
    Ok(contexts)
}

fn add_chords(properties: &mut Element, items: &mut [Item], context: Option<&StaffContext>) {
    let Some(context) = context else {
        return;
    };
    if context.generated_chords.is_empty() {
        return;
    }
    let strings = context.tuning.len();
    let elements: Vec<Element> = context
        .generated_chords
        .iter()
        .map(|item| canon::chord_item(&item.id, &item.name, strings))
        .collect();
    let kept = items.iter_mut().find(|item| {
        item.guard.is_none()
            && item.parent == PROPERTIES
            && item
                .element()
                .is_some_and(|element| element.key() == "Property@DiagramCollection")
    });
    let Some(Item {
        content: Content::Node(Node::Element(collection)),
        ..
    }) = kept
    else {
        properties.push(canon::diagram_collection(elements));
        return;
    };
    let list = collection.children.iter_mut().find_map(|node| match node {
        Node::Element(child) if child.name == "Items" => Some(child),
        _ => None,
    });
    match list {
        Some(list) => list
            .children
            .extend(elements.into_iter().map(Node::Element)),
        None => {
            let mut list = Element::new("Items");
            list.children = elements.into_iter().map(Node::Element).collect();
            collection.push(list);
        }
    }
}

fn staff_element(
    track: &Track,
    staff: &Staff,
    context: Option<&StaffContext>,
    index: usize,
    owners: &mut Owners<'_>,
) -> Result<Element, Refusal> {
    let mut items = payload::surviving(owners.items(staff.id.0)?, &Env::Plain)?;
    let mut properties = Element::new(PROPERTIES);
    let kept = |key: &str| index > 0 && staff_property(&items, key).is_some();
    if !kept("Property@CapoFret") {
        properties.push(canon::capo_property(track.capo));
    }
    if !track.tuning.strings.is_empty() && !kept("Property@Tuning") {
        properties.push(canon::tuning_property(&track.tuning.strings));
    }
    add_chords(&mut properties, &mut items, context);
    let mut element = Element::new("Staff").with_child(properties);
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

fn has_drum_kit(items: &[Item]) -> bool {
    items.iter().any(|item| {
        item.parent == "InstrumentSet"
            && item
                .element()
                .is_some_and(|element| element.name == "Elements" && element.has_elements())
    })
}

struct TrackSite<'s> {
    index: usize,
    layouts: &'s Layouts,
    contexts: &'s [StaffContext],
    master_bars: &'s [MasterBar],
}

fn track_element(
    track: &Track,
    site: &TrackSite<'_>,
    owners: &mut Owners<'_>,
) -> Result<Element, Fail> {
    let index = site.index;
    let items = payload::surviving(owners.items(track.id.0)?, &Env::Track(track))?;
    if track.instrument.kind == InstrumentKind::Percussion && !has_drum_kit(&items) {
        return Err(Fail::new(Refusal::DrumKit));
    }
    check_track(track, &items).map_err(Fail::new)?;
    let lanes = lanes(track, site.master_bars, &items)?;
    let layouts = site.layouts;
    let contexts = site.contexts;
    let id = Layouts::id(&layouts.tracks, index);
    let mut element = Element::new("Track").with_attribute("id", &id.to_string());
    element.push(xml::text("Name", &track.name).within(|| "name".to_string())?);
    element.push(xml::text("ShortName", &track.short_name).within(|| "short_name".to_string())?);
    element.push(
        Element::new("InstrumentSet").with_child(canon::instrument_type(
            track.instrument.kind,
            track.instrument.set,
        )),
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
    if let Some(rse) = rse_element(track, lanes.strip, &items) {
        element.push(rse);
    }
    element.push(sounds_element(track)?);
    if let Some(connection) = midi_connection(track, &items) {
        element.push(connection);
    }
    let mut staves = Element::new("Staves");
    for (staff_index, staff) in track.staves.iter().enumerate() {
        let context = contexts
            .iter()
            .find(|context| context.track == index && context.staff == staff_index);
        let staff = staff_element(track, staff, context, staff_index, owners)
            .within(|| format!("staves[{staff_index}]"))?;
        staves.children.push(Node::Element(staff));
    }
    element.push(staves);
    if let Some(automations) = lanes.sounds {
        element.push(automations);
    }
    apply_own(&mut element, items)?;
    patch_parameters(&mut element, &track.midi);
    Ok(element)
}

pub(crate) fn tracks_pool(
    score: &Score,
    layouts: &Layouts,
    staves: &[StaffContext],
    owners: &mut Owners<'_>,
) -> Result<Element, Fail> {
    let mut pool = Element::new("Tracks");
    for (index, track) in score.tracks.iter().enumerate() {
        let site = TrackSite {
            index,
            layouts,
            contexts: staves,
            master_bars: &score.master_bars,
        };
        let element = track_element(track, &site, owners).within(|| format!("tracks[{index}]"))?;
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
