use std::collections::BTreeMap;

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

const BYTE_ORDER_MARK: char = '\u{feff}';
const TRACK_ROOT: [&str; 3] = ["GPIF", "Tracks", "Track"];
const MASTER_BAR_ROOT: [&str; 3] = ["GPIF", "MasterBars", "MasterBar"];

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
            names == TRACK_ROOT || names == MASTER_BAR_ROOT
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
