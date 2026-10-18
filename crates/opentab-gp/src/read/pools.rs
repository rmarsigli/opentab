use crate::dom::Element;
use crate::error::{GpifProblem, PoolKind};
use crate::location::Location;
use crate::number;
use crate::opaque::ParentPath;
use crate::read::context::{Context, Fault, Owner, Scope};

pub(crate) const POOLS: [(&str, &str, PoolKind); 6] = [
    ("Tracks", "Track", PoolKind::Track),
    ("Bars", "Bar", PoolKind::Bar),
    ("Voices", "Voice", PoolKind::Voice),
    ("Beats", "Beat", PoolKind::Beat),
    ("Notes", "Note", PoolKind::Note),
    ("Rhythms", "Rhythm", PoolKind::Rhythm),
];

pub(crate) struct Pool<'e> {
    pub kind: PoolKind,
    pub location: Location,
    pub items: Vec<(&'e Element, Location)>,
    referenced: Vec<bool>,
}

impl<'e> Pool<'e> {
    fn empty(kind: PoolKind) -> Pool<'e> {
        Pool {
            kind,
            location: Location::root(),
            items: Vec::new(),
            referenced: Vec::new(),
        }
    }

    pub fn fetch(&mut self, token: &str, at: &Location) -> Result<(&'e Element, Location), Fault> {
        let index = self.resolve(token, at)?;
        if let Some(flag) = self.referenced.get_mut(index) {
            *flag = true;
        }
        match self.items.get(index) {
            Some((element, location)) => Ok((*element, location.clone())),
            None => Err(Fault::at(
                at,
                GpifProblem::MissingReference {
                    kind: self.kind,
                    id: token.to_string(),
                },
            )),
        }
    }

    pub fn resolve(&self, token: &str, at: &Location) -> Result<usize, Fault> {
        let missing = || {
            Fault::at(
                at,
                GpifProblem::MissingReference {
                    kind: self.kind,
                    id: token.to_string(),
                },
            )
        };
        let value: i64 = number::integer(token).ok_or_else(|| {
            Fault::at(
                at,
                GpifProblem::InvalidNumber {
                    text: token.to_string(),
                },
            )
        })?;
        let index = usize::try_from(value).map_err(|_| missing())?;
        if index >= self.items.len() {
            return Err(missing());
        }
        Ok(index)
    }

    pub fn check_referenced(&self) -> Result<(), Fault> {
        for ((_, location), referenced) in self.items.iter().zip(&self.referenced) {
            if !referenced {
                return Err(Fault::at(
                    location,
                    GpifProblem::Unreferenced { kind: self.kind },
                ));
            }
        }
        Ok(())
    }
}

pub(crate) struct Pools<'e> {
    pub tracks: Pool<'e>,
    pub bars: Pool<'e>,
    pub voices: Pool<'e>,
    pub beats: Pool<'e>,
    pub notes: Pool<'e>,
    pub rhythms: Pool<'e>,
}

impl<'e> Pools<'e> {
    pub fn new() -> Pools<'e> {
        Pools {
            tracks: Pool::empty(PoolKind::Track),
            bars: Pool::empty(PoolKind::Bar),
            voices: Pool::empty(PoolKind::Voice),
            beats: Pool::empty(PoolKind::Beat),
            notes: Pool::empty(PoolKind::Note),
            rhythms: Pool::empty(PoolKind::Rhythm),
        }
    }

    pub fn pool_mut(&mut self, kind: PoolKind) -> &mut Pool<'e> {
        match kind {
            PoolKind::Track => &mut self.tracks,
            PoolKind::Bar => &mut self.bars,
            PoolKind::Voice => &mut self.voices,
            PoolKind::Beat => &mut self.beats,
            PoolKind::Note => &mut self.notes,
            PoolKind::Rhythm => &mut self.rhythms,
        }
    }

    pub fn check_referenced(&self) -> Result<(), Fault> {
        self.tracks.check_referenced()?;
        self.bars.check_referenced()?;
        self.voices.check_referenced()?;
        self.beats.check_referenced()?;
        self.notes.check_referenced()
    }
}

pub(crate) fn index_pool<'e>(
    ctx: &mut Context,
    pools: &mut Pools<'e>,
    container: &'e Element,
    location: &Location,
    item_name: &str,
    kind: PoolKind,
) -> Result<(), Fault> {
    let scope = Scope::new(
        Owner::Document,
        ParentPath::owner().child(&container.name),
        location.clone(),
    );
    let mut pool = Pool::empty(kind);
    pool.location = location.clone();
    for (index, node) in container.children.iter().enumerate() {
        let at = location.child(index);
        let element = match node {
            crate::dom::Node::Element(element) if element.name == item_name => element,
            _ => {
                ctx.keep(&scope, index, node, None);
                continue;
            }
        };
        let expected = pool.items.len();
        let id = element.attribute("id").ok_or_else(|| {
            Fault::at(
                &at,
                GpifProblem::MissingAttribute {
                    name: "id".to_string(),
                },
            )
        })?;
        if number::integer::<usize>(id) != Some(expected) {
            return Err(Fault::at(
                &at,
                GpifProblem::IdOutOfOrder {
                    kind,
                    expected,
                    found: id.to_string(),
                },
            ));
        }
        pool.items.push((element, at));
        pool.referenced.push(false);
    }
    *pools.pool_mut(kind) = pool;
    Ok(())
}
