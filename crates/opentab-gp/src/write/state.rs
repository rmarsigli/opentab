use std::collections::{BTreeMap, BTreeSet};

use opentab_model::{ElementId, Extensions, InstrumentKind, Pitch, Transpose};

use crate::dom::Element;
use crate::error::Refusal;
use crate::read::ChordItem;
use crate::write::payload::{self, Item};

pub(crate) struct Owners<'s> {
    extensions: &'s Extensions,
    used: BTreeSet<ElementId>,
}

impl<'s> Owners<'s> {
    pub fn new(extensions: &'s Extensions) -> Owners<'s> {
        Owners {
            extensions,
            used: BTreeSet::new(),
        }
    }

    pub fn items(&mut self, id: ElementId) -> Result<Vec<Item>, Refusal> {
        self.used.insert(id);
        match self.extensions.0.get(&id) {
            Some(payloads) => payload::parse_all(payloads),
            None => Ok(Vec::new()),
        }
    }

    pub fn dangling(&self) -> Option<ElementId> {
        self.extensions
            .0
            .keys()
            .find(|id| !self.used.contains(id))
            .copied()
    }
}

pub(crate) struct StaffContext {
    pub track: usize,
    pub staff: usize,
    pub kind: InstrumentKind,
    pub tuning: Vec<Pitch>,
    pub capo: u8,
    pub transpose: Transpose,
    pub chords: Vec<ChordItem>,
}

pub(crate) struct Layouts {
    pub tracks: Vec<usize>,
    pub bars: Vec<usize>,
    pub voices: Vec<usize>,
    pub beats: Vec<usize>,
    pub notes: Vec<usize>,
}

impl Layouts {
    pub fn id(ids: &[usize], generated: usize) -> usize {
        ids.get(generated).copied().unwrap_or(generated)
    }
}

#[derive(Default)]
pub(crate) struct Pools {
    pub bars: Vec<Element>,
    pub voices: Vec<Element>,
    pub beats: Vec<Element>,
    pub notes: Vec<Element>,
    pub rhythms: Vec<Element>,
    pub rhythm_keys: BTreeMap<String, usize>,
    pub beat_rhythms: Vec<usize>,
}
