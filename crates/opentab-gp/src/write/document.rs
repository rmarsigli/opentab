use std::collections::BTreeMap;

use opentab_model::{MasterBar, Score, TempoChange};

use crate::canon::{self, GP_REVISION, GP_VERSION};
use crate::codes::{MINOR, TEMPO_UNITS};
use crate::dom::{Element, Node};
use crate::error::Refusal;
use crate::guard;
use crate::limits::MAX_XML_DEPTH;
use crate::read::POOLS;
use crate::write::anchor::BarAnchors;
use crate::write::beat::{BarSite, Builder, patch_rhythms, write_bar};
use crate::write::payload::{self, Env, Item, layout, renumber};
use crate::write::state::{Layouts, Owners, Pools, StaffContext};
use crate::write::track::{staff_contexts, tracks_pool};
use crate::write::xml::{self, Fail, Within};

const MAX_ALTERNATE_ENDINGS: u8 = 8;

fn meta(score: &Score) -> Result<Element, Fail> {
    let meta = &score.meta;
    let fields = [
        ("Title", "title", &meta.title),
        ("SubTitle", "subtitle", &meta.subtitle),
        ("Artist", "artist", &meta.artist),
        ("Album", "album", &meta.album),
        ("Words", "words", &meta.words),
        ("Music", "music", &meta.music),
        ("Copyright", "copyright", &meta.copyright),
        ("Tabber", "tab_author", &meta.tab_author),
        ("Instructions", "instructions", &meta.instructions),
        ("Notices", "notices", &meta.notices),
    ];
    let mut element = Element::new("Score");
    for (name, field, value) in fields {
        element.push(xml::text(name, value).within(|| format!("meta.{field}"))?);
    }
    Ok(element)
}

fn tempo_unit(change: &TempoChange) -> u8 {
    TEMPO_UNITS
        .iter()
        .find(|(_, unit)| *unit == change.unit)
        .map(|(code, _)| *code)
        .unwrap_or_default()
}

fn tempo_element(
    bar: usize,
    master: &MasterBar,
    change: &TempoChange,
    items: Vec<Item>,
) -> Result<Element, Fail> {
    let replayed = items.iter().any(|item| {
        item.guard.is_some()
            && item.parent.is_empty()
            && item
                .element()
                .is_some_and(|element| element.name == "Position")
    });
    let mut element = Element::new("Automation");
    element.push(Element::leaf("Type", "Tempo"));
    element.push(Element::leaf("Bar", &bar.to_string()));
    if !replayed {
        let position = canon::tempo_position(change.position, master.time_signature)
            .ok_or(Refusal::TempoPosition {
                numer: *change.position.numer(),
                denom: *change.position.denom(),
            })
            .map_err(Fail::new)
            .within(|| "position".to_string())?;
        element.push(Element::leaf("Position", &position));
    }
    element.push(Element::leaf(
        "Value",
        &format!("{} {}", change.bpm.bpm(), tempo_unit(change)),
    ));
    payload::apply(&mut element, items)?;
    Ok(element)
}

fn alternate_endings(mask: u8) -> String {
    (1..=MAX_ALTERNATE_ENDINGS)
        .filter(|ending| mask & (1 << (ending - 1)) != 0)
        .map(|ending| ending.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

fn tempo_target(items: &[Item], ordinal: usize, master: &MasterBar, claimed: &[bool]) -> usize {
    let Some(wanted) = items.iter().find_map(|item| item.guard.as_deref()) else {
        return ordinal;
    };
    let fits = |index: &usize| {
        !claimed.get(*index).copied().unwrap_or(true)
            && master.tempo_changes.get(*index).is_some_and(|change| {
                guard::tempo_position(change.position, master.time_signature) == wanted
            })
    };
    if fits(&ordinal) {
        return ordinal;
    }
    (0..master.tempo_changes.len())
        .find(fits)
        .unwrap_or(ordinal)
}

struct MasterOutput {
    element: Element,
    tempos: Vec<Element>,
}

fn master_bar(
    index: usize,
    master: &MasterBar,
    bars: &str,
    owners: &mut Owners<'_>,
) -> Result<MasterOutput, Fail> {
    let split = payload::split(owners.items(master.id.0)?)?;
    if !split.rhythm.is_empty() {
        return Err(Fail::new(Refusal::MalformedPayload {
            problem: "parent root does not apply here".to_string(),
        }));
    }
    let own = payload::surviving(split.own, &Env::MasterBar(master))?;
    let mut tempo_items: BTreeMap<usize, Vec<Item>> = BTreeMap::new();
    let mut claimed = vec![false; master.tempo_changes.len()];
    for (ordinal, items) in split.tempo {
        let target = tempo_target(&items, ordinal, master, &claimed);
        if let Some(change) = master.tempo_changes.get(target) {
            if let Some(flag) = claimed.get_mut(target) {
                *flag = true;
            }
            let kept = payload::surviving(items, &Env::Tempo(change, master.time_signature))?;
            tempo_items.entry(target).or_default().extend(kept);
        }
    }
    let key = master.key_signature;
    let mode = if key.minor() { MINOR } else { "Major" };
    let mut element = Element::new("MasterBar");
    element.push(
        Element::new("Key")
            .with_child(Element::leaf(
                "AccidentalCount",
                &key.accidentals().to_string(),
            ))
            .with_child(Element::leaf("Mode", mode)),
    );
    let time = master.time_signature;
    element.push(Element::leaf(
        "Time",
        &format!("{}/{}", time.numerator(), time.denominator()),
    ));
    if master.repeat_start || master.repeat_end.is_some() {
        element.push(
            Element::new("Repeat")
                .with_attribute("start", &master.repeat_start.to_string())
                .with_attribute("end", &master.repeat_end.is_some().to_string())
                .with_attribute("count", &master.repeat_end.unwrap_or(0).to_string()),
        );
    }
    if let Some(marker) = &master.marker {
        element.push(
            Element::new("Section")
                .with_child(
                    xml::text("Letter", &marker.letter).within(|| "marker.letter".to_string())?,
                )
                .with_child(xml::text("Text", &marker.text).within(|| "marker.text".to_string())?),
        );
    }
    if master.alternate_endings != 0 {
        element.push(Element::leaf(
            "AlternateEndings",
            &alternate_endings(master.alternate_endings),
        ));
    }
    if master.double_bar {
        element.push(Element::new("DoubleBar"));
    }
    if let Some(directions) = canon::directions(&master.directions) {
        element.push(directions);
    }
    element.push(Element::leaf("Bars", bars));
    payload::apply(&mut element, own)?;
    let mut tempos = Vec::with_capacity(master.tempo_changes.len());
    for (ordinal, change) in master.tempo_changes.iter().enumerate() {
        let items = tempo_items.remove(&ordinal).unwrap_or_default();
        let tempo = tempo_element(index, master, change, items)
            .within(|| format!("tempo_changes[{ordinal}]"))?;
        tempos.push(tempo);
    }
    Ok(MasterOutput { element, tempos })
}

fn pool_items<'i>(items: &'i [Item], name: &str) -> Vec<&'i Item> {
    items.iter().filter(|item| item.parent == name).collect()
}

fn pool_layout(items: &[Item], pool: &str, item: &str, generated: usize) -> Vec<usize> {
    let kept: Vec<Item> = pool_items(items, pool).into_iter().cloned().collect();
    layout(generated, &kept, item)
}

fn counts(score: &Score) -> (usize, usize, usize, usize) {
    let mut bars = 0;
    let mut voices = 0;
    let mut beats = 0;
    let mut notes = 0;
    for track in &score.tracks {
        for staff in &track.staves {
            bars += staff.bars.len();
            for bar in &staff.bars {
                voices += bar.voices.len();
                for voice in &bar.voices {
                    beats += voice.beats.len();
                    for beat in &voice.beats {
                        notes += beat.notes.len();
                    }
                }
            }
        }
    }
    (bars, voices, beats, notes)
}

fn check_bars(score: &Score) -> Result<(), Fail> {
    let expected = score.master_bars.len();
    for (track_index, track) in score.tracks.iter().enumerate() {
        for (staff_index, staff) in track.staves.iter().enumerate() {
            let found = staff.bars.len();
            if found != expected {
                return Err(Fail::new(Refusal::BarCount { expected, found }))
                    .within(|| format!("tracks[{track_index}].staves[{staff_index}]"));
            }
        }
    }
    Ok(())
}

fn write_pools(
    score: &Score,
    staves: &[StaffContext],
    layouts: &Layouts,
    owners: &mut Owners<'_>,
) -> Result<Pools, Fail> {
    let mut pools = Pools::default();
    let mut build = Builder {
        owners,
        layouts,
        pools: &mut pools,
    };
    for (bar_index, master) in score.master_bars.iter().enumerate() {
        for staff in staves {
            let Some(model) = score
                .tracks
                .get(staff.track)
                .and_then(|track| track.staves.get(staff.staff))
            else {
                continue;
            };
            let Some(bar) = model.bars.get(bar_index) else {
                continue;
            };
            let site = BarSite {
                staff,
                key: master.key_signature,
                previous_bar: bar_index
                    .checked_sub(1)
                    .and_then(|previous| model.bars.get(previous)),
            };
            write_bar(&mut build, &site, bar).within(|| {
                format!(
                    "tracks[{}].staves[{}].bars[{bar_index}]",
                    staff.track, staff.staff
                )
            })?;
        }
    }
    Ok(pools)
}

fn pool(name: &str, elements: Vec<Element>) -> Element {
    let mut pool = Element::new(name);
    pool.children = elements.into_iter().map(Node::Element).collect();
    pool
}

fn check_not_empty(score: &Score) -> Result<(), Fail> {
    if score.master_bars.is_empty() {
        return Err(Fail::new(Refusal::NoMasterBars)).within(|| "master_bars".to_string());
    }
    if score.tracks.is_empty() {
        return Err(Fail::new(Refusal::NoTracks)).within(|| "tracks".to_string());
    }
    Ok(())
}

pub(crate) fn write_gpif(score: &Score) -> Result<String, Fail> {
    check_not_empty(score)?;
    check_bars(score)?;
    let anchors = BarAnchors::new(&score.master_bars);
    let mut owners = Owners::new(&score.extensions, &anchors);
    let document = payload::parse_all(&score.document_extensions)
        .and_then(|items| payload::surviving(items, &Env::Plain))
        .and_then(|items| anchors.resolve_all(items))
        .within(|| "document_extensions".to_string())?;
    if document.iter().any(|item| item.parent.starts_with('*')) {
        return Err(Fail::new(Refusal::MalformedPayload {
            problem: "parent root does not apply here".to_string(),
        }))
        .within(|| "document_extensions".to_string());
    }
    let (bars, voices, beats, notes) = counts(score);
    let layouts = Layouts {
        tracks: pool_layout(&document, "Tracks", "Track", score.tracks.len()),
        bars: pool_layout(&document, "Bars", "Bar", bars),
        voices: pool_layout(&document, "Voices", "Voice", voices),
        beats: pool_layout(&document, "Beats", "Beat", beats),
        notes: pool_layout(&document, "Notes", "Note", notes),
    };
    let staves = staff_contexts(score, &mut owners)?;
    let mut pools = write_pools(score, &staves, &layouts, &mut owners)?;
    let rhythm_ids = pool_layout(&document, "Rhythms", "Rhythm", pools.rhythms.len());
    patch_rhythms(&mut pools, &rhythm_ids);
    let staff_count = staves.len();
    let mut master_bars = Element::new("MasterBars");
    let mut automations = Element::new("Automations");
    for (index, master) in score.master_bars.iter().enumerate() {
        let bar_ids = (0..staff_count)
            .map(|staff| Layouts::id(&layouts.bars, index * staff_count + staff).to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let output = master_bar(index, master, &bar_ids, &mut owners)
            .within(|| format!("master_bars[{index}]"))?;
        master_bars.push(output.element);
        for tempo in output.tempos {
            automations.push(tempo);
        }
    }
    let mut master_track = Element::new("MasterTrack");
    if score.anacrusis {
        master_track.push(Element::new("Anacrusis"));
    }
    let track_ids = (0..score.tracks.len())
        .map(|index| Layouts::id(&layouts.tracks, index).to_string())
        .collect::<Vec<_>>()
        .join(" ");
    master_track.push(Element::leaf("Tracks", &track_ids));
    if automations.has_elements() {
        master_track.push(automations);
    }
    let tracks = tracks_pool(score, &layouts, &staves, &mut owners)?;
    if let Some(id) = owners.dangling() {
        return Err(Fail::new(Refusal::DanglingExtension { id: id.0 }));
    }
    let mut root = Element::new("GPIF");
    let carries = |name: &str| {
        document.iter().any(|item| {
            item.parent.is_empty() && item.element().is_some_and(|element| element.name == name)
        })
    };
    if !carries(GP_VERSION) {
        root.push(canon::gp_version());
    }
    if !carries(GP_REVISION) {
        root.push(canon::gp_revision());
    }
    root.push(meta(score)?);
    root.push(master_track);
    root.push(tracks);
    root.push(master_bars);
    root.push(pool("Bars", pools.bars));
    root.push(pool("Voices", pools.voices));
    root.push(pool("Beats", pools.beats));
    root.push(pool("Notes", pools.notes));
    root.push(pool("Rhythms", pools.rhythms));
    payload::apply(&mut root, document).within(|| "document_extensions".to_string())?;
    if xml::depth(&root) > MAX_XML_DEPTH {
        return Err(Fail::new(Refusal::MalformedPayload {
            problem: format!("opaque data nests deeper than {MAX_XML_DEPTH} levels"),
        }))
        .within(|| "extensions".to_string());
    }
    for (pool_name, item_name, _) in POOLS {
        if let Some(Node::Element(container)) = root
            .children
            .iter_mut()
            .find(|node| matches!(node, Node::Element(element) if element.name == pool_name))
        {
            renumber(container, item_name);
        }
    }
    Ok(xml::document(&root))
}
