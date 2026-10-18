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
        for _ in 0..MAX_UNROLL_STEPS {
            let Some(bar) = master_bars.get(index) else {
                break;
            };
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

#[cfg(test)]
mod tests {
    use opentab_model::{ElementId, KeySignature, MasterBarId, TimeSignature};

    use super::*;

    #[derive(Clone, Copy)]
    struct Spec {
        start: bool,
        end: Option<u8>,
        endings: u8,
    }

    const PLAIN: Spec = Spec {
        start: false,
        end: None,
        endings: 0,
    };

    fn bars(specs: &[Spec]) -> Vec<MasterBar> {
        specs
            .iter()
            .enumerate()
            .map(|(index, spec)| MasterBar {
                id: MasterBarId(ElementId(index as u64)),
                time_signature: TimeSignature::new(4, 4).unwrap(),
                key_signature: KeySignature::new(0, false).unwrap(),
                repeat_start: spec.start,
                repeat_end: spec.end,
                alternate_endings: spec.endings,
                double_bar: false,
                marker: None,
                tempo_changes: Vec::new(),
            })
            .collect()
    }

    fn positions(specs: &[Spec]) -> Vec<Vec<usize>> {
        let playback = Playback::of(&bars(specs));
        (0..specs.len())
            .map(|index| {
                (0..)
                    .map_while(|occurrence| playback.position(index, occurrence))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn bars_without_repeats_play_once_in_order() {
        assert_eq!(
            positions(&[PLAIN, PLAIN, PLAIN]),
            [vec![0], vec![1], vec![2]]
        );
        assert!(positions(&[]).is_empty());
    }

    #[test]
    fn a_repeat_plays_its_bars_again() {
        let start = Spec {
            start: true,
            ..PLAIN
        };
        let end = Spec {
            end: Some(2),
            ..PLAIN
        };
        assert_eq!(
            positions(&[PLAIN, start, end, PLAIN]),
            [vec![0], vec![1, 3], vec![2, 4], vec![5]]
        );
    }

    #[test]
    fn alternate_endings_play_only_on_their_passes() {
        let start = Spec {
            start: true,
            ..PLAIN
        };
        let first = Spec {
            end: Some(2),
            endings: 0b01,
            ..PLAIN
        };
        let second = Spec {
            endings: 0b10,
            ..PLAIN
        };
        assert_eq!(
            positions(&[start, first, second, PLAIN]),
            [vec![0, 2], vec![1], vec![3], vec![4]]
        );
    }

    #[test]
    fn each_ending_plays_only_on_its_own_pass() {
        let start = Spec {
            start: true,
            ..PLAIN
        };
        let first = Spec {
            end: Some(3),
            endings: 0b001,
            ..PLAIN
        };
        let second = Spec {
            end: Some(3),
            endings: 0b010,
            ..PLAIN
        };
        let third = Spec {
            endings: 0b100,
            ..PLAIN
        };
        assert_eq!(
            positions(&[start, first, second, third]),
            [vec![0, 2, 4], vec![1], vec![3], vec![5]]
        );
    }

    #[test]
    fn an_ending_mask_reaches_the_eighth_pass_and_no_further() {
        let start = Spec {
            start: true,
            ..PLAIN
        };
        let eighth = Spec {
            endings: 0b1000_0000,
            ..PLAIN
        };
        let end = |passes| Spec {
            end: Some(passes),
            ..PLAIN
        };
        assert_eq!(
            positions(&[start, end(8), eighth]),
            [
                vec![0, 2, 4, 6, 8, 10, 12, 14],
                vec![1, 3, 5, 7, 9, 11, 13, 15],
                vec![16]
            ]
        );
        assert!(positions(&[start, end(9), eighth])[2].is_empty());
    }

    #[test]
    fn a_new_repeat_start_clears_the_previous_ending() {
        let first = Spec {
            endings: 0b10,
            ..PLAIN
        };
        let start = Spec {
            start: true,
            ..PLAIN
        };
        assert_eq!(
            positions(&[first, start, PLAIN]),
            [vec![], vec![0], vec![1]]
        );
    }

    fn lyrics_with(children: Vec<Element>) -> Element {
        let mut lyrics = Element::new(LYRICS);
        for child in children {
            lyrics.push(child);
        }
        lyrics
    }

    fn line_with(name: &str, child: &str) -> Element {
        Element::new(name).with_child(Element::leaf(child, "0"))
    }

    #[test]
    fn only_lines_holding_an_offset_are_lyric_sites() {
        let lyrics = lyrics_with(vec![
            line_with(LINE, "Text"),
            line_with("Other", OFFSET),
            line_with(LINE, OFFSET),
        ]);
        assert_eq!(
            sites(&lyrics),
            [Site {
                kind: SiteKind::Offset,
                path: vec![2]
            }]
        );
    }

    #[test]
    fn only_automations_holding_a_bar_are_automation_sites() {
        let root = Element::new("Root")
            .with_child(Element::new(AUTOMATION).with_child(Element::leaf("Type", "Other")))
            .with_child(line_with("Other", BAR))
            .with_child(line_with(AUTOMATION, BAR));
        assert_eq!(
            sites(&root),
            [Site {
                kind: SiteKind::Bar,
                path: vec![2]
            }]
        );
    }
}
