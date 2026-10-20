use std::cell::OnceCell;
use std::collections::BTreeMap;

use opentab_model::MasterBar;

use crate::anchor::{self, Anchor, Playback, Site, SiteKind};
use crate::dom::{Element, Node};
use crate::error::Refusal;
use crate::write::payload::{Content, Item};

pub(crate) struct BarAnchors<'s> {
    master_bars: &'s [MasterBar],
    index: BTreeMap<u64, usize>,
    playback: OnceCell<Playback>,
}

enum Outcome {
    Kept,
    Dropped,
}

fn malformed(problem: &str) -> Refusal {
    Refusal::MalformedPayload {
        problem: problem.to_string(),
    }
}

fn remove(root: &mut Element, path: &[usize]) -> Result<Outcome, Refusal> {
    let Some((last, parent)) = path.split_last() else {
        return Ok(Outcome::Dropped);
    };
    let container =
        anchor::at_mut(root, parent).ok_or_else(|| malformed("bar anchor path is invalid"))?;
    if *last >= container.children.len() {
        return Err(malformed("bar anchor path is invalid"));
    }
    container.children.remove(*last);
    Ok(Outcome::Kept)
}

impl<'s> BarAnchors<'s> {
    pub fn new(master_bars: &'s [MasterBar]) -> BarAnchors<'s> {
        let index = master_bars
            .iter()
            .enumerate()
            .map(|(position, master)| (master.id.0.0, position))
            .collect();
        BarAnchors {
            master_bars,
            index,
            playback: OnceCell::new(),
        }
    }

    fn successor(&self, id: u64) -> usize {
        self.master_bars
            .iter()
            .position(|master| master.id.0.0 > id)
            .unwrap_or_else(|| self.master_bars.len().saturating_sub(1))
    }

    fn playback(&self) -> &Playback {
        self.playback.get_or_init(|| Playback::of(self.master_bars))
    }

    pub fn resolve_all(&self, items: Vec<Item>) -> Result<Vec<Item>, Refusal> {
        let mut kept = Vec::with_capacity(items.len());
        for item in items {
            if let Some(item) = self.resolve(item)? {
                kept.push(item);
            }
        }
        Ok(kept)
    }

    fn resolve(&self, mut item: Item) -> Result<Option<Item>, Refusal> {
        let Some(text) = item.anchors.take() else {
            return Ok(Some(item));
        };
        let anchors = anchor::parse(&text).ok_or_else(|| malformed("invalid bar anchors"))?;
        let Content::Node(Node::Element(root)) = &mut item.content else {
            return Err(malformed("bar anchors need a kept element"));
        };
        let sites = anchor::sites(root);
        if sites.len() != anchors.len() {
            return Err(malformed("bar anchors do not match the kept data"));
        }
        for (site, anchor) in sites.iter().zip(anchors).rev() {
            if let Outcome::Dropped = self.apply(root, site, anchor)? {
                return Ok(None);
            }
        }
        Ok(Some(item))
    }

    fn apply(&self, root: &mut Element, site: &Site, anchor: Anchor) -> Result<Outcome, Refusal> {
        let target = anchor::at_mut(root, &site.path)
            .ok_or_else(|| malformed("bar anchor path is invalid"))?;
        match (site.kind, anchor) {
            (_, Anchor::Loose) => Ok(Outcome::Kept),
            (SiteKind::Bar, Anchor::Bar(id)) => match self.index.get(&id) {
                Some(index) => {
                    anchor::set_number(target, anchor::BAR, *index);
                    Ok(Outcome::Kept)
                }
                None => remove(root, &site.path),
            },
            (SiteKind::Offset, Anchor::Bar(id)) => {
                let index = self
                    .index
                    .get(&id)
                    .copied()
                    .unwrap_or_else(|| self.successor(id));
                anchor::set_number(target, anchor::OFFSET, index);
                Ok(Outcome::Kept)
            }
            (SiteKind::Sync, Anchor::Sync(id)) => {
                let occurrence = anchor::nested(target, anchor::VALUE, anchor::BAR_OCCURRENCE)
                    .ok_or_else(|| malformed("a sync point lost its bar occurrence"))?;
                let placed = self.index.get(&id).and_then(|index| {
                    let position = self.playback().position(*index, occurrence)?;
                    Some((*index, position))
                });
                match placed {
                    Some((index, bar)) => {
                        anchor::set_number(target, anchor::BAR, bar);
                        anchor::set_nested(target, anchor::VALUE, anchor::BAR_INDEX, index);
                        Ok(Outcome::Kept)
                    }
                    None => remove(root, &site.path),
                }
            }
            _ => Err(malformed("bar anchor does not fit the kept data")),
        }
    }
}
