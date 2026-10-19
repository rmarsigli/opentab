use std::collections::BTreeMap;

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

const BYTE_ORDER_MARK: char = '\u{feff}';
const TRACK_ROOT: [&str; 3] = ["GPIF", "Tracks", "Track"];
const MASTER_BAR_ROOT: [&str; 3] = ["GPIF", "MasterBars", "MasterBar"];
const MASTER_TRACK_ROOT: [&str; 2] = ["GPIF", "MasterTrack"];

pub type Inventory = BTreeMap<String, Vec<String>>;

struct Open {
    name: String,
    text: String,
}

struct Walk {
    stack: Vec<Open>,
    owner: Option<String>,
    tracks: usize,
    master_bars: usize,
    inventory: Inventory,
}

fn entity(name: &str) -> String {
    match name {
        "lt" => "<".to_string(),
        "gt" => ">".to_string(),
        "amp" => "&".to_string(),
        "apos" => "'".to_string(),
        "quot" => "\"".to_string(),
        other => {
            let code = other
                .strip_prefix("#x")
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| other.strip_prefix('#').and_then(|dec| dec.parse().ok()));
            code.and_then(char::from_u32)
                .map_or_else(|| format!("&{other};"), String::from)
        }
    }
}

impl Walk {
    fn names(&self) -> Vec<&str> {
        self.stack.iter().map(|open| open.name.as_str()).collect()
    }

    fn path(&self) -> String {
        self.names().join("/")
    }

    fn record(&mut self, entry: String) {
        if let Some(owner) = &self.owner {
            self.inventory.entry(owner.clone()).or_default().push(entry);
        }
    }

    fn open(&mut self, start: &BytesStart<'_>) {
        let name = start.name().as_ref().to_string();
        self.stack.push(Open {
            name,
            text: String::new(),
        });
        let names = self.names();
        if names == TRACK_ROOT {
            self.owner = Some(format!("Track#{}", self.tracks));
            self.tracks += 1;
        } else if names == MASTER_BAR_ROOT {
            self.owner = Some(format!("MasterBar#{}", self.master_bars));
            self.master_bars += 1;
        } else if names == MASTER_TRACK_ROOT {
            self.owner = Some("MasterTrack".to_string());
        }
        let path = self.path();
        for attribute in start.attributes().flatten() {
            let key = attribute.key.as_ref().to_string();
            let value = attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .map(|value| value.into_owned())
                .unwrap_or_default();
            self.record(format!("{path}@{key}={value}"));
        }
    }

    fn close(&mut self) {
        let path = self.path();
        let names_are_root = {
            let names = self.names();
            names == TRACK_ROOT || names == MASTER_BAR_ROOT || names == MASTER_TRACK_ROOT
        };
        if let Some(open) = self.stack.pop() {
            let text = open.text.trim().to_string();
            self.record(format!("{path}={text}"));
        }
        if names_are_root {
            self.owner = None;
        }
    }

    fn text(&mut self, text: &str) {
        if let Some(open) = self.stack.last_mut() {
            open.text.push_str(text);
        }
    }
}

pub fn inventory(gpif: &str) -> Inventory {
    let text = gpif.trim_start_matches(BYTE_ORDER_MARK);
    let mut reader = Reader::from_str(text);
    let mut walk = Walk {
        stack: Vec::new(),
        owner: None,
        tracks: 0,
        master_bars: 0,
        inventory: Inventory::new(),
    };
    loop {
        match reader.read_event().expect("well-formed GPIF") {
            Event::Start(start) => walk.open(&start),
            Event::Empty(start) => {
                walk.open(&start);
                walk.close();
            }
            Event::End(_) => walk.close(),
            Event::Text(content) => {
                let decoded = content.xml10_content().into_owned();
                walk.text(&decoded);
            }
            Event::CData(content) => {
                walk.text(&content.into_inner());
            }
            Event::GeneralRef(reference) => {
                let name = reference.into_inner().into_owned();
                walk.text(&entity(&name));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    for entries in walk.inventory.values_mut() {
        entries.sort();
    }
    walk.inventory
}

pub fn first_difference(before: &Inventory, after: &Inventory) -> Option<String> {
    let keys: Vec<&String> = before.keys().chain(after.keys()).collect();
    for key in keys {
        let left = before.get(key).cloned().unwrap_or_default();
        let right = after.get(key).cloned().unwrap_or_default();
        if left != right {
            let lost: Vec<&String> = left.iter().filter(|entry| !right.contains(entry)).collect();
            let gained: Vec<&String> = right.iter().filter(|entry| !left.contains(entry)).collect();
            return Some(format!(
                "{key}: lost {:?} gained {:?}",
                lost.iter().take(5).collect::<Vec<_>>(),
                gained.iter().take(5).collect::<Vec<_>>()
            ));
        }
    }
    None
}

#[test]
fn the_inventory_counts_paths_attributes_and_text_per_owner() {
    let gpif = "<GPIF><Tracks><Track id=\"0\"><Name><![CDATA[a&b]]></Name><X>1 &amp; 2</X></Track><Track id=\"1\"/></Tracks><MasterBars><MasterBar><Bars>0 1</Bars></MasterBar></MasterBars><Bars/></GPIF>";
    let found = inventory(gpif);
    assert_eq!(
        found.get("Track#0").cloned().unwrap_or_default(),
        vec![
            "GPIF/Tracks/Track/Name=a&b".to_string(),
            "GPIF/Tracks/Track/X=1 & 2".to_string(),
            "GPIF/Tracks/Track=".to_string(),
            "GPIF/Tracks/Track@id=0".to_string(),
        ]
    );
    assert_eq!(found.len(), 3);
    let mut moved = found.clone();
    if let Some(entries) = moved.get_mut("Track#0") {
        entries.retain(|entry| !entry.contains("/X="));
    }
    assert!(first_difference(&found, &moved).is_some_and(|text| text.contains("lost")));
    assert_eq!(first_difference(&found, &found), None);
}

pub const MASTER_BAR_ORDER: [&str; 11] = [
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

pub fn master_bar_children(gpif: &str) -> Vec<Vec<String>> {
    let text = gpif.trim_start_matches(BYTE_ORDER_MARK);
    let mut reader = Reader::from_str(text);
    let mut stack: Vec<String> = Vec::new();
    let mut bars: Vec<Vec<String>> = Vec::new();
    loop {
        let (start, empty) = match reader.read_event().expect("well-formed GPIF") {
            Event::Start(start) => (start.name().as_ref().to_string(), false),
            Event::Empty(start) => (start.name().as_ref().to_string(), true),
            Event::End(_) => {
                stack.pop();
                continue;
            }
            Event::Eof => break,
            _ => continue,
        };
        let name = start;
        let parents: Vec<&str> = stack.iter().map(String::as_str).collect();
        if parents == MASTER_BAR_ROOT {
            if let Some(children) = bars.last_mut() {
                children.push(name.clone());
            }
        } else if name == "MasterBar" && parents == MASTER_BAR_ROOT[..2] {
            bars.push(Vec::new());
        }
        if !empty {
            stack.push(name);
        }
    }
    bars
}

pub fn out_of_order(children: &[String]) -> Option<String> {
    let ranks: Vec<usize> = children
        .iter()
        .filter_map(|name| MASTER_BAR_ORDER.iter().position(|known| known == name))
        .collect();
    let sorted = ranks.windows(2).all(|pair| pair[0] <= pair[1]);
    (!sorted).then(|| children.join(" "))
}

#[test]
fn master_bar_children_are_listed_per_bar_and_checked_against_the_documented_order() {
    let gpif = "<GPIF><MasterBars><MasterBar><Key><Mode>Major</Mode></Key><Time>4/4</Time><Bars>0</Bars><XProperties/></MasterBar><MasterBar><Time>3/4</Time><XProperties/><Bars>1</Bars></MasterBar></MasterBars></GPIF>";
    let found = master_bar_children(gpif);
    assert_eq!(
        found,
        vec![
            vec!["Key", "Time", "Bars", "XProperties"],
            vec!["Time", "XProperties", "Bars"],
        ]
    );
    assert_eq!(out_of_order(&found[0]), None);
    assert_eq!(
        out_of_order(&found[1]),
        Some("Time XProperties Bars".to_string())
    );
}

pub fn ordered_directions(gpif: &str) -> Vec<String> {
    let text = gpif.trim_start_matches(BYTE_ORDER_MARK);
    let mut reader = Reader::from_str(text);
    let mut found: Vec<String> = Vec::new();
    let mut inside = false;
    let mut child: Option<String> = None;
    let mut entries: Vec<String> = Vec::new();
    loop {
        match reader.read_event().expect("well-formed GPIF") {
            Event::Start(start) => {
                let name = start.name().as_ref().to_string();
                if name == "Directions" {
                    inside = true;
                    entries.clear();
                } else if inside {
                    child = Some(name);
                }
            }
            Event::Empty(start) => {
                let name = start.name().as_ref().to_string();
                if name == "Directions" {
                    found.push(String::new());
                } else if inside {
                    entries.push(format!("{name}:"));
                }
            }
            Event::Text(content) if inside => {
                if let Some(name) = &child {
                    entries.push(format!("{name}:{}", content.xml10_content()));
                }
            }
            Event::End(end) => {
                let name = end.name().as_ref().to_string();
                if name == "Directions" {
                    inside = false;
                    found.push(entries.join(" "));
                } else {
                    child = None;
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    found
}

#[test]
fn ordered_directions_keep_the_order_of_targets_and_jumps() {
    let gpif = "<GPIF><MasterBar><Directions><Jump>DaCoda</Jump><Target>Coda</Target></Directions></MasterBar><MasterBar><Directions/></MasterBar></GPIF>";
    assert_eq!(
        ordered_directions(gpif),
        vec!["Jump:DaCoda Target:Coda".to_string(), String::new()]
    );
}

#[test]
fn the_master_track_is_an_owner_of_its_own() {
    let gpif = "<GPIF><MasterTrack><Automations><Automation><Linear>true</Linear></Automation></Automations></MasterTrack></GPIF>";
    let found = inventory(gpif);
    let entries = found.get("MasterTrack").cloned().unwrap_or_default();
    assert!(
        entries.contains(&"GPIF/MasterTrack/Automations/Automation/Linear=true".to_string()),
        "{entries:?}"
    );
}

#[derive(Debug, Default, Clone)]
struct Tree {
    name: String,
    attributes: Vec<String>,
    text: String,
    children: Vec<Tree>,
}

impl Tree {
    fn child(&self, name: &str) -> Option<&Tree> {
        self.children.iter().find(|child| child.name == name)
    }

    fn id(&self) -> Option<&str> {
        self.attributes
            .iter()
            .find_map(|attribute| attribute.strip_prefix("id="))
    }

    fn render(&self) -> String {
        let inner: Vec<String> = self.children.iter().map(Tree::render).collect();
        format!(
            "<{} {}>{}{}</{}>",
            self.name,
            self.attributes.join(" "),
            self.text.trim(),
            inner.join(""),
            self.name
        )
    }
}

fn open_tree(start: &BytesStart<'_>) -> Tree {
    Tree {
        name: start.name().as_ref().to_string(),
        attributes: start
            .attributes()
            .flatten()
            .map(|attribute| {
                let value = attribute
                    .normalized_value(XmlVersion::Implicit1_0)
                    .map(|value| value.into_owned())
                    .unwrap_or_default();
                format!("{}={value}", attribute.key.as_ref())
            })
            .collect(),
        ..Tree::default()
    }
}

fn tree(gpif: &str) -> Tree {
    let text = gpif.trim_start_matches(BYTE_ORDER_MARK);
    let mut reader = Reader::from_str(text);
    let mut stack: Vec<Tree> = vec![Tree::default()];
    loop {
        match reader.read_event().expect("well-formed GPIF") {
            Event::Start(start) => stack.push(open_tree(&start)),
            Event::Empty(start) => {
                let done = open_tree(&start);
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(done);
                }
            }
            Event::End(_) => {
                let done = stack.pop().unwrap_or_default();
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(done);
                }
            }
            Event::Text(content) => {
                if let Some(open) = stack.last_mut() {
                    open.text.push_str(&content.xml10_content());
                }
            }
            Event::CData(content) => {
                if let Some(open) = stack.last_mut() {
                    open.text.push_str(&content.into_inner());
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    stack.pop().unwrap_or_default()
}

fn pool<'t>(root: &'t Tree, name: &str) -> BTreeMap<String, &'t Tree> {
    root.child(name)
        .map(|pool| {
            pool.children
                .iter()
                .filter_map(|item| item.id().map(|id| (id.to_string(), item)))
                .collect()
        })
        .unwrap_or_default()
}

fn tokens(tree: Option<&Tree>) -> Vec<String> {
    tree.map(|found| found.text.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default()
}

const SPEED_PROPERTY: &str = "id=688062467";

fn note_effects(note: &Tree) -> Option<String> {
    let trill = note.child("Trill").map(Tree::render);
    let speed = note.child("XProperties").and_then(|container| {
        container
            .children
            .iter()
            .find(|property| property.attributes.iter().any(|a| a == SPEED_PROPERTY))
            .map(Tree::render)
    });
    (trill.is_some() || speed.is_some()).then(|| format!("{trill:?} {speed:?}"))
}

pub fn effect_references(gpif: &str) -> Vec<String> {
    let document = tree(gpif);
    let root = document.child("GPIF").unwrap_or(&document);
    let (bars, voices, beats, notes) = (
        pool(root, "Bars"),
        pool(root, "Voices"),
        pool(root, "Beats"),
        pool(root, "Notes"),
    );
    let mut found = Vec::new();
    let masters = root
        .child("MasterBars")
        .map_or(&[][..], |list| &list.children[..]);
    for (index, master) in masters.iter().enumerate() {
        for bar in tokens(master.child("Bars")) {
            let Some(bar) = bars.get(&bar) else { continue };
            for voice in tokens(bar.child("Voices")) {
                let Some(voice) = voices.get(&voice) else {
                    continue;
                };
                for beat in tokens(voice.child("Beats")) {
                    let Some(beat) = beats.get(&beat) else {
                        continue;
                    };
                    if let Some(whammy) = beat.child("Whammy") {
                        found.push(format!("{index}: {}", whammy.render()));
                    }
                    for note in tokens(beat.child("Notes")) {
                        if let Some(effects) = notes.get(&note).and_then(|note| note_effects(note))
                        {
                            found.push(format!("{index}: {effects}"));
                        }
                    }
                }
            }
        }
    }
    found
}

#[test]
fn effect_references_follow_shared_beats_and_notes() {
    let gpif = concat!(
        "<GPIF><MasterBars><MasterBar><Bars>0</Bars></MasterBar><MasterBar><Bars>0</Bars></MasterBar></MasterBars>",
        "<Bars><Bar id=\"0\"><Voices>0 -1</Voices></Bar></Bars>",
        "<Voices><Voice id=\"0\"><Beats>0 1</Beats></Voice></Voices>",
        "<Beats><Beat id=\"0\"><Whammy originValue=\"0\"/><Notes>0</Notes></Beat><Beat id=\"1\"/></Beats>",
        "<Notes><Note id=\"0\"><Trill>45</Trill><XProperties><XProperty id=\"688062467\"><Int>120</Int></XProperty></XProperties></Note></Notes>",
        "</GPIF>"
    );
    let found = effect_references(gpif);
    assert_eq!(found.len(), 4, "{found:?}");
    assert!(
        found[0].starts_with("0: <Whammy originValue=0>"),
        "{found:?}"
    );
    assert!(
        found[1].contains("<Trill >45</Trill>") && found[1].contains("<Int >120</Int>"),
        "{found:?}"
    );
    assert!(found[2].starts_with("1: "), "{found:?}");
}

pub const NOTE_ORDER: [&str; 11] = [
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

pub fn note_children(gpif: &str) -> Vec<Vec<String>> {
    let document = tree(gpif);
    let root = document.child("GPIF").unwrap_or(&document);
    root.child("Notes")
        .map(|notes| {
            notes
                .children
                .iter()
                .map(|note| {
                    note.children
                        .iter()
                        .map(|child| child.name.clone())
                        .collect()
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn out_of_note_order(children: &[String]) -> Option<String> {
    let ranks: Vec<usize> = children
        .iter()
        .filter_map(|name| NOTE_ORDER.iter().position(|known| known == name))
        .collect();
    let sorted = ranks.windows(2).all(|pair| pair[0] <= pair[1]);
    let containers = children
        .iter()
        .filter(|name| *name == "XProperties")
        .count();
    (!sorted || containers > 1).then(|| children.join(" "))
}

#[test]
fn note_children_are_listed_and_checked_against_the_writer_order() {
    let gpif = "<GPIF><Notes><Note id=\"0\"><Trill>45</Trill><InstrumentArticulation>0</InstrumentArticulation><Properties/></Note><Note id=\"1\"><InstrumentArticulation>0</InstrumentArticulation><Trill>45</Trill><XProperties/><XProperties/></Note></Notes></GPIF>";
    let found = note_children(gpif);
    assert_eq!(
        found[0],
        vec!["Trill", "InstrumentArticulation", "Properties"]
    );
    assert_eq!(out_of_note_order(&found[0]), None);
    assert!(out_of_note_order(&found[1]).is_some());
    assert!(
        out_of_note_order(&[
            "Properties".to_string(),
            "XProperties".to_string(),
            "XProperties".to_string()
        ])
        .is_some()
    );
}

const KIT_ROOT: [&str; 5] = ["GPIF", "Tracks", "Track", "InstrumentSet", "Elements"];

struct KitWalk {
    stack: Vec<Open>,
    tracks: Vec<Vec<String>>,
}

impl KitWalk {
    fn inside(&self) -> bool {
        self.stack.len() >= KIT_ROOT.len()
            && self
                .stack
                .iter()
                .zip(KIT_ROOT)
                .all(|(open, name)| open.name == name)
    }

    fn open(&mut self, name: String) {
        if self.stack.len() == 2 && name == "Track" {
            self.tracks.push(Vec::new());
        }
        self.stack.push(Open {
            name,
            text: String::new(),
        });
    }

    fn close(&mut self) {
        let inside = self.inside();
        let path: Vec<String> = self
            .stack
            .iter()
            .skip(KIT_ROOT.len())
            .map(|open| open.name.clone())
            .collect();
        if let Some(open) = self.stack.pop()
            && inside
            && let Some(entries) = self.tracks.last_mut()
        {
            entries.push(format!("{}={}", path.join("/"), open.text.trim()));
        }
    }
}

pub fn ordered_drum_kits(gpif: &str) -> Vec<Vec<String>> {
    let text = gpif.trim_start_matches(BYTE_ORDER_MARK);
    let mut reader = Reader::from_str(text);
    let mut walk = KitWalk {
        stack: Vec::new(),
        tracks: Vec::new(),
    };
    loop {
        match reader.read_event().expect("well-formed GPIF") {
            Event::Start(start) => walk.open(start.name().as_ref().to_string()),
            Event::Empty(start) => {
                walk.open(start.name().as_ref().to_string());
                walk.close();
            }
            Event::End(_) => walk.close(),
            Event::Text(content) => {
                if let Some(open) = walk.stack.last_mut() {
                    open.text.push_str(&content.xml10_content());
                }
            }
            Event::CData(content) => {
                if let Some(open) = walk.stack.last_mut() {
                    open.text.push_str(&content.into_inner());
                }
            }
            Event::GeneralRef(reference) => {
                let name = reference.into_inner().into_owned();
                if let Some(open) = walk.stack.last_mut() {
                    open.text.push_str(&entity(&name));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    walk.tracks
}

#[test]
fn ordered_drum_kits_list_each_track_kit_in_document_order() {
    let gpif = "<GPIF><Tracks><Track><InstrumentSet><Type>drumKit</Type><Elements><Element><Name>B &amp; A</Name><Articulations><Articulation><OutputMidiNumber>9</OutputMidiNumber></Articulation><Articulation><OutputMidiNumber>3</OutputMidiNumber></Articulation></Articulations></Element></Elements></InstrumentSet></Track><Track><InstrumentSet><Type>voice</Type></InstrumentSet></Track></Tracks></GPIF>";
    assert_eq!(
        ordered_drum_kits(gpif),
        vec![
            vec![
                "Element/Name=B & A".to_string(),
                "Element/Articulations/Articulation/OutputMidiNumber=9".to_string(),
                "Element/Articulations/Articulation=".to_string(),
                "Element/Articulations/Articulation/OutputMidiNumber=3".to_string(),
                "Element/Articulations/Articulation=".to_string(),
                "Element/Articulations=".to_string(),
                "Element=".to_string(),
                "=".to_string(),
            ],
            Vec::new(),
        ]
    );
}

pub const INSTRUMENT_SET_ORDER: [&str; 4] = ["Name", "Type", "LineCount", "Elements"];

pub fn instrument_set_children(gpif: &str) -> Vec<Vec<String>> {
    let document = tree(gpif);
    let root = document.child("GPIF").unwrap_or(&document);
    root.child("Tracks")
        .map(|tracks| {
            tracks
                .children
                .iter()
                .map(|track| {
                    track
                        .child("InstrumentSet")
                        .map(|set| {
                            set.children
                                .iter()
                                .map(|child| child.name.clone())
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn out_of_set_order(children: &[String]) -> Option<String> {
    let ranks: Vec<usize> = children
        .iter()
        .filter_map(|name| INSTRUMENT_SET_ORDER.iter().position(|known| known == name))
        .collect();
    let sorted = ranks.windows(2).all(|pair| pair[0] < pair[1]);
    (!sorted).then(|| children.join(" "))
}

#[test]
fn instrument_set_children_are_listed_per_track_and_checked() {
    let gpif = "<GPIF><Tracks><Track><InstrumentSet><Name>K</Name><Type>drumKit</Type><LineCount>5</LineCount><Elements/></InstrumentSet></Track><Track><InstrumentSet><Elements/><Type>voice</Type></InstrumentSet></Track></Tracks></GPIF>";
    let found = instrument_set_children(gpif);
    assert_eq!(found[0], INSTRUMENT_SET_ORDER.to_vec());
    assert_eq!(out_of_set_order(&found[0]), None);
    assert_eq!(
        out_of_set_order(&found[1]),
        Some("Elements Type".to_string())
    );
}
