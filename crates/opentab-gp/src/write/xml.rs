use crate::dom::{self, Element, Node};
use crate::error::{GpError, Refusal};

const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n";

#[derive(Debug)]
pub(crate) struct Fail {
    steps: Vec<String>,
    reason: Refusal,
}

impl Fail {
    pub fn new(reason: Refusal) -> Fail {
        Fail {
            steps: Vec::new(),
            reason,
        }
    }

    pub fn into_error(mut self) -> GpError {
        self.steps.reverse();
        let path = if self.steps.is_empty() {
            "score".to_string()
        } else {
            self.steps.join(".")
        };
        GpError::Unsupported {
            path,
            reason: self.reason,
        }
    }
}

impl From<Refusal> for Fail {
    fn from(reason: Refusal) -> Self {
        Fail::new(reason)
    }
}

pub(crate) trait Within<T> {
    fn within<F: FnOnce() -> String>(self, step: F) -> Result<T, Fail>;
}

impl<T> Within<T> for Result<T, Fail> {
    fn within<F: FnOnce() -> String>(self, step: F) -> Result<T, Fail> {
        self.map_err(|mut fail| {
            fail.steps.push(step());
            fail
        })
    }
}

impl<T> Within<T> for Result<T, Refusal> {
    fn within<F: FnOnce() -> String>(self, step: F) -> Result<T, Fail> {
        self.map_err(|reason| Fail {
            steps: vec![step()],
            reason,
        })
    }
}

fn is_text_char(character: char) -> bool {
    matches!(character,
        '\t' | '\n' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..)
}

pub(crate) fn check_escaped(text: &str) -> Result<(), Refusal> {
    match text
        .chars()
        .find(|character| *character != '\r' && !is_text_char(*character))
    {
        Some(character) => Err(Refusal::Character {
            code: u32::from(character),
        }),
        None => Ok(()),
    }
}

pub(crate) fn check_text(text: &str) -> Result<(), Refusal> {
    match text.chars().find(|character| !is_text_char(*character)) {
        Some(character) => Err(Refusal::Character {
            code: u32::from(character),
        }),
        None => Ok(()),
    }
}

pub(crate) fn text(name: &str, value: &str) -> Result<Element, Refusal> {
    check_text(value)?;
    let mut element = Element::new(name);
    element.children = dom::cdata_nodes(value);
    Ok(element)
}

fn pretty(element: &Element) -> bool {
    element.has_elements()
        && element.children.iter().all(|node| {
            matches!(
                node,
                Node::Element(_) | Node::Comment(_) | Node::Instruction(_)
            )
        })
}

pub(crate) fn write_pretty(element: &Element, out: &mut String) {
    if !pretty(element) {
        dom::write_element(element, out);
        return;
    }
    let mut open = Element::new(&element.name);
    open.attributes.clone_from(&element.attributes);
    let mut head = String::new();
    dom::write_element(&open, &mut head);
    if let Some(stripped) = head.strip_suffix("/>") {
        out.push_str(stripped);
    } else {
        out.push_str(&head);
    }
    out.push_str(">\n");
    for child in &element.children {
        match child {
            Node::Element(child) => write_pretty(child, out),
            other => dom::write_node(other, out),
        }
        out.push('\n');
    }
    out.push_str("</");
    out.push_str(&element.name);
    out.push('>');
}

pub(crate) fn document(root: &Element) -> String {
    let mut out = String::from(DECLARATION);
    write_pretty(root, &mut out);
    out.push('\n');
    out
}
