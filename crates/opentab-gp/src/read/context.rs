use opentab_model::{ElementId, ModelError, Score};

use crate::dom::{Element, Node};
use crate::error::GpifProblem;
use crate::limits::{
    MAX_MODEL_ELEMENTS, MIN_RETAINED_BUDGET, PAYLOAD_OVERHEAD, RETAINED_BUDGET_FACTOR,
};
use crate::location::Location;
use crate::number;
use crate::opaque::{OpaqueContent, OpaqueItem, ParentPath};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Fault {
    pub location: Location,
    pub problem: GpifProblem,
}

impl Fault {
    pub fn at(location: &Location, problem: GpifProblem) -> Fault {
        Fault {
            location: location.clone(),
            problem,
        }
    }

    pub fn model(location: &Location, error: ModelError) -> Fault {
        Fault::at(location, GpifProblem::Model(error))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Owner {
    Document,
    Element(ElementId),
}

#[derive(Debug, Clone)]
pub(crate) struct Scope {
    pub owner: Owner,
    pub parent: ParentPath,
    pub location: Location,
}

impl Scope {
    pub fn new(owner: Owner, parent: ParentPath, location: Location) -> Scope {
        Scope {
            owner,
            parent,
            location,
        }
    }

    pub fn enter(&self, visit: &Visit<'_>) -> Scope {
        Scope {
            owner: self.owner,
            parent: self.parent.child(&visit.key),
            location: visit.location.clone(),
        }
    }
}

pub(crate) struct Visit<'e> {
    pub key: String,
    pub element: &'e Element,
    pub location: Location,
}

impl Visit<'_> {
    pub fn text(&self) -> String {
        self.element.text()
    }

    pub fn fault(&self, problem: GpifProblem) -> Fault {
        Fault::at(&self.location, problem)
    }

    pub fn invalid(&self) -> Fault {
        self.fault(GpifProblem::InvalidValue {
            text: self.text().trim().to_string(),
        })
    }

    pub fn integer<T: std::str::FromStr>(&self) -> Result<T, Fault> {
        let text = self.text();
        number::integer(&text).ok_or_else(|| {
            self.fault(GpifProblem::InvalidNumber {
                text: text.trim().to_string(),
            })
        })
    }
}

pub(crate) enum Handled {
    Unknown,
    Mapped(&'static [&'static str]),
    Replayed(String),
    Custom,
}

pub(crate) struct Context {
    pub score: Score,
    retained_bytes: u64,
    retained_budget: u64,
}

impl Context {
    pub fn new(source_size: usize) -> Context {
        let scaled = u64::try_from(source_size)
            .unwrap_or(u64::MAX)
            .saturating_mul(RETAINED_BUDGET_FACTOR);
        Context {
            score: Score::new(),
            retained_bytes: 0,
            retained_budget: scaled.max(MIN_RETAINED_BUDGET),
        }
    }

    pub fn alloc<T: From<ElementId>>(&mut self, location: &Location) -> Result<T, Fault> {
        if self.score.ids.peek().0 > MAX_MODEL_ELEMENTS {
            return Err(Fault::at(
                location,
                GpifProblem::ExpansionLimit {
                    limit: MAX_MODEL_ELEMENTS,
                },
            ));
        }
        self.score
            .alloc()
            .ok_or_else(|| Fault::at(location, GpifProblem::IdsExhausted))
    }

    pub fn keep(
        &mut self,
        scope: &Scope,
        index: usize,
        node: &Node,
        guard: Option<String>,
    ) -> Result<(), Fault> {
        let item = OpaqueItem {
            parent: &scope.parent,
            index,
            guard: guard.as_deref(),
            content: OpaqueContent::Node(node),
        };
        self.push(scope, item)
    }

    pub fn keep_attributes(
        &mut self,
        scope: &Scope,
        element: &Element,
        known: &[&str],
    ) -> Result<(), Fault> {
        for (index, attribute) in element.attributes.iter().enumerate() {
            if known.contains(&attribute.name.as_str()) {
                continue;
            }
            let item = OpaqueItem {
                parent: &scope.parent,
                index,
                guard: None,
                content: OpaqueContent::Attribute(attribute),
            };
            self.push(scope, item)?;
        }
        Ok(())
    }

    pub fn charge(&mut self, bytes: usize, location: &Location) -> Result<(), Fault> {
        let size = u64::try_from(bytes)
            .unwrap_or(u64::MAX)
            .saturating_add(PAYLOAD_OVERHEAD);
        self.retained_bytes = self.retained_bytes.saturating_add(size);
        if self.retained_bytes > self.retained_budget {
            return Err(Fault::at(
                location,
                GpifProblem::RetainedLimit {
                    limit: self.retained_budget,
                },
            ));
        }
        Ok(())
    }

    fn push(&mut self, scope: &Scope, item: OpaqueItem<'_>) -> Result<(), Fault> {
        let payload = item.encode();
        self.charge(payload.data.len(), &scope.location)?;
        match scope.owner {
            Owner::Document => self.score.document_extensions.push(payload),
            Owner::Element(id) => self.score.extensions.0.entry(id).or_default().push(payload),
        }
        Ok(())
    }

    pub fn visit<'e, F>(
        &mut self,
        scope: &Scope,
        element: &'e Element,
        mut handle: F,
    ) -> Result<(), Fault>
    where
        F: FnMut(&mut Context, &Visit<'e>) -> Result<Handled, Fault>,
    {
        let mut seen: Vec<String> = Vec::new();
        for (index, node) in element.children.iter().enumerate() {
            let Node::Element(child) = node else {
                self.keep(scope, index, node, None)?;
                continue;
            };
            let visit = Visit {
                key: child.key(),
                element: child,
                location: scope.location.child(index),
            };
            let handled = handle(self, &visit)?;
            if !matches!(handled, Handled::Unknown) {
                if seen.contains(&visit.key) {
                    return Err(visit.fault(GpifProblem::DuplicateElement {
                        name: visit.key.clone(),
                    }));
                }
                seen.push(visit.key.clone());
            }
            match handled {
                Handled::Unknown => self.keep(scope, index, node, None)?,
                Handled::Mapped(known) => {
                    let inner = scope.enter(&visit);
                    self.keep_attributes(&inner, child, known)?;
                }
                Handled::Replayed(guard) => self.keep(scope, index, node, Some(guard))?,
                Handled::Custom => {}
            }
        }
        Ok(())
    }
}

pub(crate) fn missing_element(name: &str) -> GpifProblem {
    GpifProblem::MissingElement {
        name: name.to_string(),
    }
}
