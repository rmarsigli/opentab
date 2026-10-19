use opentab_model::{Bar, Beat, BeatEffects, Duration, KeySignature, Voice};

use crate::canon;
use crate::codes::{
    BRUSH_DURATION, CLEFS, DYNAMICS, FADE_IN, GRACES, NOTE_VALUES, STROKE_DIRECTIONS, TREMOLOS,
    name_of,
};
use crate::dom::{Element, Node};
use crate::error::Refusal;
use crate::guard;
use crate::write::note::{NoteSite, write_note};
use crate::write::payload::{self, Env, Item};
use crate::write::state::{Layouts, Owners, Pools, StaffContext};
use crate::write::track::apply_own;
use crate::write::xml::{self, Fail, Within};

const EMPTY_SLOT: &str = "-1";
const SLOTS: usize = 4;
const DEFAULT_STROKE_TICKS: u16 = 480;

pub(crate) struct Builder<'s, 'o> {
    pub owners: &'s mut Owners<'o>,
    pub layouts: &'s Layouts,
    pub pools: &'s mut Pools,
}

fn id_list(ids: impl Iterator<Item = usize>) -> String {
    ids.map(|id| id.to_string()).collect::<Vec<_>>().join(" ")
}

pub(crate) struct BarSite<'s> {
    pub staff: &'s StaffContext,
    pub key: KeySignature,
    pub previous_bar: Option<&'s Bar>,
}

pub(crate) fn write_bar(
    build: &mut Builder<'_, '_>,
    site: &BarSite<'_>,
    bar: &Bar,
) -> Result<(), Fail> {
    let items = payload::surviving(build.owners.items(bar.id.0)?, &Env::Bar(bar))?;
    let generated = build.pools.bars.len();
    let id = Layouts::id(&build.layouts.bars, generated);
    let mut slots: Vec<String> = vec![EMPTY_SLOT.to_string(); SLOTS];
    let mut previous = None;
    let mut voice_ids = Vec::with_capacity(bar.voices.len());
    for voice in &bar.voices {
        let slot = voice.slot.get();
        if previous.is_some_and(|previous| slot <= previous) {
            return Err(Fail::new(Refusal::VoiceSlots));
        }
        previous = Some(slot);
        let generated_voice = build.pools.voices.len() + voice_ids.len();
        let voice_id = Layouts::id(&build.layouts.voices, generated_voice);
        if let Some(entry) = slots.get_mut(usize::from(slot)) {
            *entry = voice_id.to_string();
        }
        voice_ids.push(voice_id);
    }
    let mut element = Element::new("Bar").with_attribute("id", &id.to_string());
    element.push(Element::leaf("Clef", name_of(CLEFS, bar.clef)));
    element.push(Element::leaf("Voices", &slots.join(" ")));
    apply_own(&mut element, items)?;
    build.pools.bars.push(element);
    for (index, (voice, voice_id)) in bar.voices.iter().zip(voice_ids).enumerate() {
        let carried = guard::carried(site.previous_bar, voice.slot);
        write_voice(build, site, voice, voice_id, carried).within(|| format!("voices[{index}]"))?;
    }
    Ok(())
}

fn write_voice(
    build: &mut Builder<'_, '_>,
    site: &BarSite<'_>,
    voice: &Voice,
    id: usize,
    carried: Option<&Beat>,
) -> Result<(), Fail> {
    let items = payload::surviving(build.owners.items(voice.id.0)?, &Env::Plain)?;
    let first = build.pools.beats.len();
    let beat_ids =
        (0..voice.beats.len()).map(|offset| Layouts::id(&build.layouts.beats, first + offset));
    let mut element = Element::new("Voice").with_attribute("id", &id.to_string());
    element.push(Element::leaf("Beats", &id_list(beat_ids)));
    apply_own(&mut element, items)?;
    build.pools.voices.push(element);
    let mut previous = carried;
    for (index, beat) in voice.beats.iter().enumerate() {
        write_beat(build, site, beat, previous).within(|| format!("beats[{index}]"))?;
        previous = Some(beat);
    }
    Ok(())
}

fn rhythm_element(duration: Duration, items: Vec<Item>) -> Result<Element, Refusal> {
    let mut element = Element::new("Rhythm").with_attribute("id", "");
    element.push(Element::leaf(
        "NoteValue",
        name_of(NOTE_VALUES, duration.value()),
    ));
    if duration.dots() > 0 {
        element.push(
            Element::new("AugmentationDot").with_attribute("count", &duration.dots().to_string()),
        );
    }
    if let Some(tuplet) = duration.tuplet() {
        element.push(
            Element::new("PrimaryTuplet")
                .with_attribute("num", &tuplet.num().to_string())
                .with_attribute("den", &tuplet.den().to_string()),
        );
    }
    payload::apply(&mut element, items)?;
    Ok(element)
}

fn intern_rhythm(pools: &mut Pools, element: Element) -> usize {
    let mut key = String::new();
    xml::write_pretty(&element, &mut key);
    if let Some(index) = pools.rhythm_keys.get(&key) {
        return *index;
    }
    let index = pools.rhythms.len();
    pools.rhythm_keys.insert(key, index);
    pools.rhythms.push(element);
    index
}

fn flag(name: &str) -> Element {
    Element::property(name, Element::new("Enable"))
}

fn has_guarded(items: &[Item], parent: &str, key: &str) -> bool {
    items.iter().any(|item| {
        item.guard.is_some()
            && item.parent == parent
            && item.element().is_some_and(|element| element.key() == key)
    })
}

fn chord(effects: &BeatEffects, staff: &StaffContext, items: &[Item]) -> Option<Element> {
    let name = effects.chord_name.as_ref()?;
    if has_guarded(items, "", "Chord") {
        return None;
    }
    staff
        .chords
        .iter()
        .find(|item| item.name == *name)
        .map(|item| canon::chord(&item.id))
}

fn beat_properties(effects: &BeatEffects) -> (Element, Element) {
    let mut properties = Element::new("Properties");
    let mut extra = Element::new("XProperties");
    if let Some(stroke) = effects.stroke {
        properties.push(Element::property(
            "Brush",
            Element::leaf("Direction", name_of(STROKE_DIRECTIONS, stroke.direction)),
        ));
        let ticks = stroke.duration.ticks();
        if ticks != DEFAULT_STROKE_TICKS {
            extra.push(
                Element::new("XProperty")
                    .with_attribute("id", BRUSH_DURATION)
                    .with_child(Element::leaf("Int", &ticks.to_string())),
            );
        }
    }
    if effects.pop {
        properties.push(flag("Popped"));
    }
    if effects.slap {
        properties.push(flag("Slapped"));
    }
    (properties, extra)
}

fn write_beat(
    build: &mut Builder<'_, '_>,
    site: &BarSite<'_>,
    beat: &Beat,
    previous: Option<&Beat>,
) -> Result<(), Fail> {
    let staff = site.staff;
    let items = payload::surviving(build.owners.items(beat.id.0)?, &Env::Beat(&beat.effects))?;
    let split = payload::split(items)?;
    if !split.tempo.is_empty() {
        return Err(Fail::new(Refusal::MalformedPayload {
            problem: "parent root does not apply here".to_string(),
        }));
    }
    let rhythm = rhythm_element(beat.duration, split.rhythm)?;
    let rhythm = intern_rhythm(build.pools, rhythm);
    let effects = &beat.effects;
    let generated = build.pools.beats.len();
    let id = Layouts::id(&build.layouts.beats, generated);
    let mut element = Element::new("Beat").with_attribute("id", &id.to_string());
    if let Some(grace) = effects.grace {
        element.push(Element::leaf("GraceNotes", name_of(GRACES, grace)));
    }
    element.push(Element::leaf("Dynamic", name_of(DYNAMICS, effects.dynamic)));
    element.push(Element::new("Rhythm").with_attribute("ref", &rhythm.to_string()));
    if let Some(bar) = effects.tremolo_bar {
        element.push(canon::whammy(bar));
    }
    if let Some(tremolo) = effects.tremolo_picking {
        element.push(Element::leaf("Tremolo", name_of(TREMOLOS, tremolo.value())));
    }
    if effects.fade_in {
        element.push(Element::leaf("Fadding", FADE_IN));
    }
    if let Some(text) = &effects.text {
        element.push(xml::text("FreeText", text).within(|| "effects.text".to_string())?);
    }
    if let Some(chord) = chord(effects, staff, &split.own) {
        element.push(chord);
    }
    let first_note = build.pools.notes.len();
    if !beat.notes.is_empty() {
        let note_ids = (0..beat.notes.len())
            .map(|offset| Layouts::id(&build.layouts.notes, first_note + offset));
        element.push(Element::leaf("Notes", &id_list(note_ids)));
    }
    let (properties, extra) = beat_properties(effects);
    if properties.has_elements() {
        element.push(properties);
    }
    if extra.has_elements() {
        element.push(extra);
    }
    payload::apply(&mut element, split.own)?;
    build.pools.beats.push(element);
    build.pools.beat_rhythms.push(rhythm);
    for (index, note) in beat.notes.iter().enumerate() {
        let note_id = Layouts::id(&build.layouts.notes, first_note + index);
        let note_site = NoteSite {
            staff,
            key: site.key,
            previous,
        };
        let note = write_note(build.owners, &note_site, note, note_id)
            .within(|| format!("notes[{index}]"))?;
        build.pools.notes.push(note);
    }
    Ok(())
}

pub(crate) fn patch_rhythms(pools: &mut Pools, ids: &[usize]) {
    for (beat, rhythm) in pools.beats.iter_mut().zip(&pools.beat_rhythms) {
        let id = Layouts::id(ids, *rhythm).to_string();
        for node in &mut beat.children {
            if let Node::Element(child) = node
                && child.name == "Rhythm"
                && let Some(reference) = child
                    .attributes
                    .iter_mut()
                    .find(|attribute| attribute.name == "ref")
            {
                reference.value.clone_from(&id);
                break;
            }
        }
    }
}
