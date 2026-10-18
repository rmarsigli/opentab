use opentab_model::MasterBar;

use crate::dom::{Element, Node};

pub(crate) const ANCHORS: &str = "bars";
const LOOSE: &str = "-";
const SYNC_SEPARATOR: char = ':';
const SYNC_POINT: &str = "SyncPoint";
const AUTOMATION: &str = "Automation";
const LYRICS: &str = "Lyrics";
const LINE: &str = "Line";
pub(crate) const BAR: &str = "Bar";
pub(crate) const OFFSET: &str = "Offset";
pub(crate) const VALUE: &str = "Value";
pub(crate) const BAR_INDEX: &str = "BarIndex";
pub(crate) const BAR_OCCURRENCE: &str = "BarOccurrence";
const MAX_UNROLL_STEPS: usize = 1 << 22;
const ENDINGS: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SiteKind {
    Bar,
    Sync,
    Offset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Site {
    pub kind: SiteKind,
    pub path: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Anchor {
    Loose,
    Bar(u64),
    Sync(u64, i64),
}

pub(crate) fn sites(root: &Element) -> Vec<Site> {
    let mut found = Vec::new();
    let mut path = Vec::new();
    collect(root, &mut path, &mut found);
    found
}

fn collect(element: &Element, path: &mut Vec<usize>, found: &mut Vec<Site>) {
    if element.name == AUTOMATION && element.child(BAR).is_some() {
        let sync = element
            .child("Type")
            .is_some_and(|(_, kind)| kind.text().trim() == SYNC_POINT);
        let kind = if sync { SiteKind::Sync } else { SiteKind::Bar };
        found.push(Site {
            kind,
            path: path.clone(),
        });
        return;
    }
    if element.name == LYRICS {
        for (index, line) in element.elements() {
            if line.name == LINE && line.child(OFFSET).is_some() {
                path.push(index);
                found.push(Site {
                    kind: SiteKind::Offset,
                    path: path.clone(),
                });
                path.pop();
            }
        }
        return;
    }
    for (index, child) in element.elements() {
        path.push(index);
        collect(child, path, found);
        path.pop();
    }
}

pub(crate) fn at<'e>(root: &'e Element, path: &[usize]) -> Option<&'e Element> {
    let mut current = root;
    for index in path {
        current = match current.children.get(*index) {
            Some(Node::Element(child)) => child,
            _ => return None,
        };
    }
    Some(current)
}

pub(crate) fn at_mut<'e>(root: &'e mut Element, path: &[usize]) -> Option<&'e mut Element> {
    let mut current = root;
    for index in path {
        current = match current.children.get_mut(*index) {
            Some(Node::Element(child)) => child,
            _ => return None,
        };
    }
    Some(current)
}

pub(crate) fn number(element: &Element, name: &str) -> Option<usize> {
    let (_, child) = element.child(name)?;
    match child.children.as_slice() {
        [Node::Text(text)] => {
            let value: usize = text.parse().ok()?;
            (value.to_string() == *text).then_some(value)
        }
        _ => None,
    }
}

pub(crate) fn nested(element: &Element, outer: &str, name: &str) -> Option<usize> {
    let (_, inner) = element.child(outer)?;
    number(inner, name)
}

pub(crate) fn set_number(element: &mut Element, name: &str, value: usize) -> bool {
    let target = element.children.iter_mut().find_map(|node| match node {
        Node::Element(child) if child.name == name => Some(child),
        _ => None,
    });
    match target {
        Some(child) => {
            child.children = vec![Node::Text(value.to_string())];
            true
        }
        None => false,
    }
}

pub(crate) fn set_nested(element: &mut Element, outer: &str, name: &str, value: usize) -> bool {
    let target = element.children.iter_mut().find_map(|node| match node {
        Node::Element(child) if child.name == outer => Some(child),
        _ => None,
    });
    target.is_some_and(|inner| set_number(inner, name, value))
}

pub(crate) fn render(anchors: &[Anchor]) -> String {
    anchors
        .iter()
        .map(|anchor| match anchor {
            Anchor::Loose => LOOSE.to_string(),
            Anchor::Bar(id) => id.to_string(),
            Anchor::Sync(id, delta) => format!("{id}{SYNC_SEPARATOR}{delta}"),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn plain<T: std::str::FromStr + ToString>(text: &str) -> Option<T> {
    let value: T = text.parse().ok()?;
    (value.to_string() == text).then_some(value)
}

pub(crate) fn parse(text: &str) -> Option<Vec<Anchor>> {
    text.split(' ')
        .map(|token| {
            if token == LOOSE {
                return Some(Anchor::Loose);
            }
            match token.split_once(SYNC_SEPARATOR) {
                Some((id, delta)) => Some(Anchor::Sync(plain(id)?, plain(delta)?)),
                None => Some(Anchor::Bar(plain(token)?)),
            }
        })
        .collect()
}

pub(crate) struct Playback {
    positions: Vec<Vec<usize>>,
}

impl Playback {
    pub fn of(master_bars: &[MasterBar]) -> Playback {
        let mut positions = vec![Vec::new(); master_bars.len()];
        let mut index = 0usize;
        let mut start = 0usize;
        let mut pass = 1u32;
        let mut ending: Option<u8> = None;
        let mut jumped = false;
        let mut played = 0usize;
        let mut steps = 0usize;
        while let Some(bar) = master_bars.get(index) {
            if steps >= MAX_UNROLL_STEPS {
                break;
            }
            steps += 1;
            if bar.repeat_start && !jumped {
                start = index;
                pass = 1;
            }
            jumped = false;
            if bar.alternate_endings != 0 {
                ending = Some(bar.alternate_endings);
            } else if bar.repeat_start {
                ending = None;
            }
            if let Some(mask) = ending {
                let covered = pass <= ENDINGS && mask & (1u8 << (pass - 1)) != 0;
                if !covered {
                    index += 1;
                    continue;
                }
            }
            if let Some(slot) = positions.get_mut(index) {
                slot.push(played);
            }
            played += 1;
            if let Some(count) = bar.repeat_end
                && pass < u32::from(count)
            {
                pass += 1;
                index = start;
                ending = None;
                jumped = true;
                continue;
            }
            index += 1;
        }
        Playback { positions }
    }

    pub fn position(&self, index: usize, occurrence: usize) -> Option<usize> {
        self.positions.get(index)?.get(occurrence).copied()
    }
}
