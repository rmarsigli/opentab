use opentab_model::OpaquePayload;

use crate::dom::{Attribute, Node, escape_attribute, write_node};

pub(crate) const FORMAT: &str = "gpif";
pub(crate) const KEEP: &str = "Keep";
pub(crate) const KEEP_ATTRIBUTE: &str = "KeepAttribute";
pub(crate) const RHYTHM_ROOT: &str = "*Rhythm";
pub(crate) const TEMPO_ROOT: &str = "*Tempo";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ParentPath(String);

impl ParentPath {
    pub fn owner() -> ParentPath {
        ParentPath(String::new())
    }

    pub fn tempo(index: usize) -> ParentPath {
        ParentPath(format!("{TEMPO_ROOT}#{index}"))
    }

    pub fn rhythm() -> ParentPath {
        ParentPath(RHYTHM_ROOT.to_string())
    }

    pub fn child(&self, step: &str) -> ParentPath {
        if self.0.is_empty() {
            ParentPath(step.to_string())
        } else {
            ParentPath(format!("{}/{step}", self.0))
        }
    }

    pub fn nth(&self, name: &str, ordinal: usize) -> ParentPath {
        self.child(&format!("{name}#{ordinal}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpaqueContent<'a> {
    Node(&'a Node),
    Attribute(&'a Attribute),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OpaqueItem<'a> {
    pub parent: &'a ParentPath,
    pub index: usize,
    pub guard: Option<&'a str>,
    pub content: OpaqueContent<'a>,
}

impl OpaqueItem<'_> {
    pub fn encode(&self) -> OpaquePayload {
        let mut data = String::new();
        let name = match self.content {
            OpaqueContent::Node(_) => KEEP,
            OpaqueContent::Attribute(_) => KEEP_ATTRIBUTE,
        };
        data.push('<');
        data.push_str(name);
        push_attribute(&mut data, "parent", self.parent.as_str());
        push_attribute(&mut data, "index", &self.index.to_string());
        if let Some(guard) = self.guard {
            push_attribute(&mut data, "guard", guard);
        }
        match self.content {
            OpaqueContent::Node(node) => {
                data.push('>');
                write_node(node, &mut data);
                data.push_str("</");
                data.push_str(KEEP);
                data.push('>');
            }
            OpaqueContent::Attribute(attribute) => {
                push_attribute(&mut data, "name", &attribute.name);
                push_attribute(&mut data, "value", &attribute.value);
                data.push_str("/>");
            }
        }
        OpaquePayload {
            format: FORMAT.to_string(),
            data,
        }
    }
}

fn push_attribute(data: &mut String, name: &str, value: &str) {
    data.push(' ');
    data.push_str(name);
    data.push_str("=\"");
    escape_attribute(value, data);
    data.push('"');
}
