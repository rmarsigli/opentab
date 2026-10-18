use std::collections::BTreeMap;

use opentab_model::{
    Bar, Beat, BeatEffects, MasterBar, Note, OpaquePayload, TempoChange, TimeSignature, Track,
};

use crate::dom::{self, Attribute, Element, Node};
use crate::error::Refusal;
use crate::guard::{self, PitchInputs};
use crate::limits::MAX_XML_DEPTH;
use crate::number;
use crate::opaque::{FORMAT, KEEP, KEEP_ATTRIBUTE, RHYTHM_ROOT, TEMPO_ROOT};
use crate::write::xml::{check_escaped, check_text, is_name};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Content {
    Node(Node),
    Attribute(Attribute),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Item {
    pub parent: String,
    pub index: usize,
    pub guard: Option<String>,
    pub content: Content,
}

impl Item {
    pub fn element(&self) -> Option<&Element> {
        match &self.content {
            Content::Node(Node::Element(element)) => Some(element),
            _ => None,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Split {
    pub own: Vec<Item>,
    pub rhythm: Vec<Item>,
    pub tempo: BTreeMap<usize, Vec<Item>>,
}

pub(crate) struct NoteEnv<'a> {
    pub note: &'a Note,
    pub inputs: PitchInputs<'a>,
    pub previous: Option<&'a Beat>,
}

pub(crate) enum Env<'a> {
    Plain,
    MasterBar(&'a MasterBar),
    Tempo(&'a TempoChange, TimeSignature),
    Track(&'a Track),
    Bar(&'a Bar),
    Beat(&'a BeatEffects),
    Note(&'a NoteEnv<'a>),
}

fn malformed(problem: &str) -> Refusal {
    Refusal::MalformedPayload {
        problem: problem.to_string(),
    }
}

pub(crate) fn parse(payload: &OpaquePayload) -> Result<Item, Refusal> {
    if payload.format != FORMAT {
        return Err(Refusal::PayloadFormat {
            format: payload.format.clone(),
        });
    }
    let root = dom::parse(&payload.data).map_err(|failure| Refusal::MalformedPayload {
        problem: failure.problem.to_string(),
    })?;
    let parent = root
        .attribute("parent")
        .ok_or_else(|| malformed("missing parent"))?
        .to_string();
    check_parent(&parent)?;
    let index = root
        .attribute("index")
        .and_then(number::integer::<usize>)
        .ok_or_else(|| malformed("missing or invalid index"))?;
    let guard = root.attribute("guard").map(str::to_string);
    let content = match root.name.as_str() {
        KEEP => {
            let known = ["parent", "index", "guard"];
            if root
                .attributes
                .iter()
                .any(|attribute| !known.contains(&attribute.name.as_str()))
            {
                return Err(malformed("unexpected attribute on Keep"));
            }
            let mut children = root.children.into_iter();
            match (children.next(), children.next()) {
                (Some(node), None) => Content::Node(node),
                _ => return Err(malformed("Keep must hold exactly one node")),
            }
        }
        KEEP_ATTRIBUTE => {
            let name = root
                .attribute("name")
                .ok_or_else(|| malformed("missing attribute name"))?;
            let value = root
                .attribute("value")
                .ok_or_else(|| malformed("missing attribute value"))?;
            if !root.children.is_empty() || name.is_empty() {
                return Err(malformed("invalid KeepAttribute"));
            }
            Content::Attribute(Attribute {
                name: name.to_string(),
                value: value.to_string(),
            })
        }
        _ => return Err(malformed("unknown payload element")),
    };
    match &content {
        Content::Node(node) => check_node(node)?,
        Content::Attribute(attribute) => check_attribute(attribute)?,
    }
    Ok(Item {
        parent,
        index,
        guard,
        content,
    })
}

fn check_name(name: &str) -> Result<(), Refusal> {
    if is_name(name) {
        Ok(())
    } else {
        Err(malformed(&format!("\"{name}\" is not an XML name")))
    }
}

fn check_step(step: &str) -> Result<(), Refusal> {
    if let Some(value) = step
        .strip_prefix("Property@")
        .or_else(|| step.strip_prefix("XProperty@"))
    {
        return check_escaped(value);
    }
    match step.split_once('#') {
        Some((name, ordinal)) => {
            check_name(name)?;
            if ordinal.is_empty() || !ordinal.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(malformed("invalid parent ordinal"));
            }
            Ok(())
        }
        None => check_name(step),
    }
}

fn check_parent(parent: &str) -> Result<(), Refusal> {
    if parent.is_empty() {
        return Ok(());
    }
    if parent.split('/').count() > MAX_XML_DEPTH {
        return Err(malformed(&format!(
            "parent path has more than {MAX_XML_DEPTH} steps"
        )));
    }
    for (position, step) in parent.split('/').enumerate() {
        if position == 0 && step.starts_with('*') {
            continue;
        }
        check_step(step)?;
    }
    Ok(())
}

fn check_attribute(attribute: &Attribute) -> Result<(), Refusal> {
    check_name(&attribute.name)?;
    check_escaped(&attribute.value)
}

fn check_node(node: &Node) -> Result<(), Refusal> {
    match node {
        Node::Element(element) => {
            check_name(&element.name)?;
            for attribute in &element.attributes {
                check_attribute(attribute)?;
            }
            for child in &element.children {
                check_node(child)?;
            }
            Ok(())
        }
        Node::Text(text) => check_escaped(text),
        Node::Comment(text) => {
            if text.contains("--") || text.ends_with('-') {
                return Err(malformed("a comment cannot hold \"--\" or end with \"-\""));
            }
            check_text(text)
        }
        Node::CData(text) | Node::Instruction(text) => check_text(text),
    }
}

pub(crate) fn parse_all(payloads: &[OpaquePayload]) -> Result<Vec<Item>, Refusal> {
    payloads.iter().map(parse).collect()
}

fn strip_root<'p>(parent: &'p str, root: &str) -> Option<&'p str> {
    let rest = parent.strip_prefix(root)?;
    if rest.is_empty() {
        return Some("");
    }
    rest.strip_prefix('/')
}

pub(crate) fn split(items: Vec<Item>) -> Result<Split, Refusal> {
    let mut split = Split::default();
    for mut item in items {
        if !item.parent.starts_with('*') {
            split.own.push(item);
            continue;
        }
        if let Some(rest) = strip_root(&item.parent, RHYTHM_ROOT) {
            item.parent = rest.to_string();
            split.rhythm.push(item);
            continue;
        }
        let tempo = item
            .parent
            .strip_prefix(TEMPO_ROOT)
            .and_then(|rest| rest.strip_prefix('#'))
            .ok_or_else(|| malformed("unknown parent root"))?;
        let (ordinal, rest) = match tempo.split_once('/') {
            Some((ordinal, rest)) => (ordinal, rest),
            None => (tempo, ""),
        };
        let ordinal = number::integer::<usize>(ordinal)
            .filter(|_| !ordinal.starts_with('+'))
            .ok_or_else(|| malformed("invalid tempo ordinal"))?;
        item.parent = rest.to_string();
        split.tempo.entry(ordinal).or_default().push(item);
    }
    Ok(split)
}

fn expected(kind: &str, guard: &str, env: &Env<'_>) -> Option<String> {
    let value = match (kind, env) {
        ("repeat", Env::MasterBar(bar)) => guard::repeat(bar.repeat_start, bar.repeat_end),
        ("minor", Env::MasterBar(bar)) => guard::minor(bar.key_signature.minor()),
        ("tempo-position", Env::Tempo(tempo, time)) => guard::tempo_position(tempo.position, *time),
        ("instrument", Env::Track(track)) => guard::instrument(track.instrument.kind),
        ("clef", Env::Bar(bar)) => guard::clef(bar.clef),
        ("stroke", Env::Beat(effects)) => guard::stroke(effects.stroke),
        ("grace", Env::Beat(effects)) => guard::grace(effects.grace),
        ("dynamic", Env::Beat(effects)) => guard::dynamic(effects.dynamic),
        ("tremolo", Env::Beat(effects)) => guard::tremolo(effects.tremolo_picking),
        ("fade-in", Env::Beat(effects)) => guard::fade_in(effects.fade_in),
        ("chord", Env::Beat(effects)) => guard::chord(effects.chord_name.as_deref()),
        ("flag", Env::Beat(effects)) => {
            let name = flag_name(guard)?;
            let value = match name {
                "Slapped" => effects.slap,
                "Popped" => effects.pop,
                _ => return None,
            };
            guard::flag(name, value)
        }
        (_, Env::Note(note)) => return note_expected(kind, guard, note),
        _ => return None,
    };
    Some(value)
}

fn note_expected(kind: &str, guard: &str, env: &NoteEnv<'_>) -> Option<String> {
    let effects = &env.note.effects;
    let value = match kind {
        "pitch" => guard::pitch(&env.inputs),
        "hopo" => guard::hopo(env.previous, env.note.kind),
        "note-kind" => guard::note_kind(env.note.kind),
        "accent" => guard::accent(effects.accent, effects.staccato),
        "slide" => guard::slide(effects.slide),
        "bend" => guard::bend(effects.bend),
        "harmonic" => guard::harmonic(effects.harmonic),
        "tap" => guard::tap(effects.tap),
        "vibrato" => guard::vibrato(effects.vibrato),
        "ghost" => guard::ghost(effects.ghost),
        "left-finger" => guard::left_finger(effects.left_hand_finger),
        "right-finger" => guard::right_finger(effects.right_hand_finger),
        "flag" => {
            let name = flag_name(guard)?;
            let value = match name {
                "HopoOrigin" => effects.hammer_on_pull_off,
                "Muted" => effects.dead,
                "PalmMuted" => effects.palm_mute,
                _ => return None,
            };
            guard::flag(name, value)
        }
        _ => return None,
    };
    Some(value)
}

fn flag_name(guard: &str) -> Option<&str> {
    let rest = guard.strip_prefix("flag:")?;
    rest.split_once(' ').map(|(name, _)| name)
}

pub(crate) fn surviving(items: Vec<Item>, env: &Env<'_>) -> Result<Vec<Item>, Refusal> {
    let mut kept = Vec::with_capacity(items.len());
    for item in items {
        let Some(guard) = &item.guard else {
            kept.push(item);
            continue;
        };
        let kind = guard
            .split_once(':')
            .map(|(kind, _)| kind)
            .unwrap_or_default();
        let current = expected(kind, guard, env)
            .ok_or_else(|| malformed(&format!("guard \"{guard}\" does not apply here")))?;
        if current == *guard {
            kept.push(item);
        }
    }
    Ok(kept)
}

struct Group {
    route: Vec<usize>,
    nodes: Vec<Item>,
    attributes: Vec<Item>,
}

fn create(step: &str) -> Element {
    if let Some(name) = step.strip_prefix("Property@") {
        return Element::new("Property").with_attribute("name", name);
    }
    if let Some(id) = step.strip_prefix("XProperty@") {
        return Element::new("XProperty").with_attribute("id", id);
    }
    Element::new(step.split_once('#').map_or(step, |(name, _)| name))
}

fn locate(element: &Element, step: &str) -> Option<usize> {
    if let Some((name, ordinal)) = step.split_once('#') {
        let ordinal = number::integer::<usize>(ordinal)?;
        return element
            .elements()
            .filter(|(_, child)| child.name == name)
            .nth(ordinal)
            .map(|(index, _)| index);
    }
    element
        .elements()
        .find(|(_, child)| child.key() == step)
        .map(|(index, _)| index)
}

fn resolve(root: &mut Element, parent: &str) -> Result<Vec<usize>, Refusal> {
    let mut route = Vec::new();
    if parent.is_empty() {
        return Ok(route);
    }
    let mut current = root;
    for step in parent.split('/') {
        if step.is_empty() || step.starts_with('*') {
            return Err(malformed("invalid parent path"));
        }
        let index = match locate(current, step) {
            Some(index) => index,
            None => {
                if let Some((name, ordinal)) = step.split_once('#') {
                    let ordinal = number::integer::<usize>(ordinal)
                        .ok_or_else(|| malformed("invalid parent ordinal"))?;
                    let present = current
                        .elements()
                        .filter(|(_, child)| child.name == name)
                        .count();
                    if ordinal != present {
                        return Err(malformed("parent ordinal skips an element"));
                    }
                }
                current.children.push(Node::Element(create(step)));
                current.children.len() - 1
            }
        };
        route.push(index);
        current = match current.children.get_mut(index) {
            Some(Node::Element(child)) => child,
            _ => return Err(malformed("invalid parent path")),
        };
    }
    Ok(route)
}

fn walk<'e>(root: &'e mut Element, route: &[usize]) -> Option<&'e mut Element> {
    let mut current = root;
    for index in route {
        current = match current.children.get_mut(*index) {
            Some(Node::Element(child)) => child,
            _ => return None,
        };
    }
    Some(current)
}

pub(crate) fn apply(root: &mut Element, items: Vec<Item>) -> Result<(), Refusal> {
    if items.is_empty() {
        return Ok(());
    }
    let mut grouped: BTreeMap<String, (Vec<Item>, Vec<Item>)> = BTreeMap::new();
    for item in items {
        let entry = grouped.entry(item.parent.clone()).or_default();
        match item.content {
            Content::Node(_) => entry.0.push(item),
            Content::Attribute(_) => entry.1.push(item),
        }
    }
    let mut groups = Vec::with_capacity(grouped.len());
    for (parent, (nodes, attributes)) in grouped {
        let route = resolve(root, &parent)?;
        groups.push(Group {
            route,
            nodes,
            attributes,
        });
    }
    groups.sort_by(|left, right| {
        right
            .route
            .len()
            .cmp(&left.route.len())
            .then_with(|| left.route.cmp(&right.route))
    });
    for group in groups {
        let target = walk(root, &group.route).ok_or_else(|| malformed("invalid parent path"))?;
        insert(target, group.nodes, group.attributes);
    }
    Ok(())
}

fn insert(target: &mut Element, mut nodes: Vec<Item>, mut attributes: Vec<Item>) {
    let suppressed: Vec<String> = nodes
        .iter()
        .filter(|item| item.guard.is_some())
        .filter_map(Item::element)
        .map(Element::key)
        .collect();
    if !suppressed.is_empty() {
        target.children.retain(|node| match node {
            Node::Element(child) => !suppressed.contains(&child.key()),
            _ => true,
        });
    }
    nodes.sort_by_key(|item| item.index);
    for item in nodes {
        if let Content::Node(node) = item.content {
            let at = item.index.min(target.children.len());
            target.children.insert(at, node);
        }
    }
    attributes.sort_by_key(|item| item.index);
    for item in attributes {
        if let Content::Attribute(attribute) = item.content {
            target
                .attributes
                .retain(|existing| existing.name != attribute.name);
            let at = item.index.min(target.attributes.len());
            target.attributes.insert(at, attribute);
        }
    }
}

pub(crate) fn layout(generated: usize, items: &[Item], item_name: &str) -> Vec<usize> {
    let mut sequence: Vec<Option<bool>> = vec![None; generated];
    let mut kept: Vec<&Item> = items
        .iter()
        .filter(|item| matches!(item.content, Content::Node(_)))
        .collect();
    kept.sort_by_key(|item| item.index);
    for item in kept {
        let named = item
            .element()
            .is_some_and(|element| element.name == item_name);
        let at = item.index.min(sequence.len());
        sequence.insert(at, Some(named));
    }
    let mut ids = Vec::with_capacity(generated);
    let mut ordinal = 0usize;
    for slot in sequence {
        match slot {
            None => {
                ids.push(ordinal);
                ordinal += 1;
            }
            Some(true) => ordinal += 1,
            Some(false) => {}
        }
    }
    ids
}

pub(crate) fn renumber(container: &mut Element, item_name: &str) {
    let mut ordinal = 0usize;
    for node in &mut container.children {
        let Node::Element(child) = node else {
            continue;
        };
        if child.name != item_name {
            continue;
        }
        let id = ordinal.to_string();
        match child
            .attributes
            .iter_mut()
            .find(|attribute| attribute.name == "id")
        {
            Some(attribute) => attribute.value = id,
            None => child.attributes.insert(
                0,
                Attribute {
                    name: "id".to_string(),
                    value: id,
                },
            ),
        }
        ordinal += 1;
    }
}
