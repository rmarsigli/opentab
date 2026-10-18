use quick_xml::Reader;
use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::name::QName;

use crate::error::GpifProblem;
use crate::limits::{MAX_ELEMENTS, MAX_XML_DEPTH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Attribute {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Node {
    Element(Element),
    Text(String),
    CData(String),
    Comment(String),
    Instruction(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Element {
    pub name: String,
    pub attributes: Vec<Attribute>,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParseFailure {
    pub path: String,
    pub problem: GpifProblem,
}

impl Element {
    pub fn new(name: &str) -> Element {
        Element {
            name: name.to_string(),
            attributes: Vec::new(),
            children: Vec::new(),
        }
    }

    pub fn leaf(name: &str, text: &str) -> Element {
        let mut element = Element::new(name);
        if !text.is_empty() {
            element.children.push(Node::Text(text.to_string()));
        }
        element
    }

    pub fn property(name: &str, child: Element) -> Element {
        Element::new("Property")
            .with_attribute("name", name)
            .with_child(child)
    }

    pub fn with_attribute(mut self, name: &str, value: &str) -> Element {
        self.attributes.push(Attribute {
            name: name.to_string(),
            value: value.to_string(),
        });
        self
    }

    pub fn with_child(mut self, child: Element) -> Element {
        self.children.push(Node::Element(child));
        self
    }

    pub fn push(&mut self, child: Element) {
        self.children.push(Node::Element(child));
    }

    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|attribute| attribute.name == name)
            .map(|attribute| attribute.value.as_str())
    }

    pub fn elements(&self) -> impl Iterator<Item = (usize, &Element)> {
        self.children
            .iter()
            .enumerate()
            .filter_map(|(index, node)| match node {
                Node::Element(element) => Some((index, element)),
                _ => None,
            })
    }

    pub fn child(&self, name: &str) -> Option<(usize, &Element)> {
        self.elements().find(|(_, element)| element.name == name)
    }

    pub fn has_elements(&self) -> bool {
        self.elements().next().is_some()
    }

    pub fn text(&self) -> String {
        let has_cdata = self
            .children
            .iter()
            .any(|node| matches!(node, Node::CData(_)));
        let blank_text = self.children.iter().all(|node| match node {
            Node::Text(text) => text.chars().all(char::is_whitespace),
            _ => true,
        });
        let cdata_only = has_cdata && blank_text;
        let mut value = String::new();
        for node in &self.children {
            match node {
                Node::CData(text) => value.push_str(text),
                Node::Text(text) if !cdata_only => value.push_str(text),
                _ => {}
            }
        }
        value
    }

    pub fn key(&self) -> String {
        match self.name.as_str() {
            "Property" => format!("Property@{}", self.attribute("name").unwrap_or_default()),
            "XProperty" => format!("XProperty@{}", self.attribute("id").unwrap_or_default()),
            _ => self.name.clone(),
        }
    }
}

const CDATA_END: &str = "]]>";

pub(crate) fn cdata_nodes(value: &str) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut rest = value;
    while let Some(at) = rest.find(CDATA_END) {
        let (head, tail) = rest.split_at(at + 2);
        nodes.push(Node::CData(head.to_string()));
        rest = tail;
    }
    nodes.push(Node::CData(rest.to_string()));
    nodes
}

pub(crate) fn parse(text: &str) -> Result<Element, ParseFailure> {
    let mut parser = Parser {
        reader: Reader::from_str(text),
        stack: Vec::new(),
        root: None,
        elements: 0,
    };
    parser.reader.config_mut().check_end_names = true;
    parser.run()?;
    match parser.root {
        Some(root) => Ok(root),
        None => Err(ParseFailure {
            path: "/".to_string(),
            problem: GpifProblem::Syntax {
                message: "document has no root element".to_string(),
            },
        }),
    }
}

struct Parser<'a> {
    reader: Reader<&'a [u8]>,
    stack: Vec<Element>,
    root: Option<Element>,
    elements: usize,
}

impl Parser<'_> {
    fn run(&mut self) -> Result<(), ParseFailure> {
        loop {
            let event = self
                .reader
                .read_event()
                .map_err(|error| self.syntax(error.to_string()))?;
            match event {
                Event::Start(start) => {
                    let element = self.open(&start)?;
                    self.stack.push(element);
                }
                Event::Empty(start) => {
                    let element = self.open(&start)?;
                    self.close(element)?;
                }
                Event::End(_) => match self.stack.pop() {
                    Some(element) => self.close(finish(element))?,
                    None => return Err(self.syntax("unexpected end tag".to_string())),
                },
                Event::Text(text) => {
                    let content = text.xml10_content().into_owned();
                    self.push_text(&content)?;
                }
                Event::GeneralRef(reference) => {
                    let content = resolve_reference(&reference).ok_or_else(|| {
                        self.syntax(format!("unknown entity reference &{};", &*reference))
                    })?;
                    self.push_text(&content)?;
                }
                Event::CData(data) => {
                    let content = data.xml10_content().into_owned();
                    self.push_node(Node::CData(content))?;
                }
                Event::Comment(comment) => {
                    let content = comment.xml10_content().into_owned();
                    self.push_node(Node::Comment(content))?;
                }
                Event::PI(instruction) => {
                    let content = instruction.to_string();
                    self.push_node(Node::Instruction(content))?;
                }
                Event::Decl(_) => {}
                Event::DocType(_) => {
                    return Err(self.syntax("document type declarations are not supported".into()));
                }
                Event::Eof => {
                    if !self.stack.is_empty() {
                        return Err(self.syntax("unexpected end of document".to_string()));
                    }
                    return Ok(());
                }
            }
        }
    }

    fn open(&mut self, start: &BytesStart<'_>) -> Result<Element, ParseFailure> {
        if self.root.is_some() && self.stack.is_empty() {
            return Err(self.syntax("content after the root element".to_string()));
        }
        if self.stack.len() >= MAX_XML_DEPTH {
            return Err(self.failure(GpifProblem::DepthLimit {
                limit: MAX_XML_DEPTH,
            }));
        }
        self.elements += 1;
        if self.elements > MAX_ELEMENTS {
            return Err(self.failure(GpifProblem::ElementLimit {
                limit: MAX_ELEMENTS,
            }));
        }
        let name = qname(start.name());
        let mut attributes = Vec::new();
        for attribute in start.attributes() {
            let attribute = attribute.map_err(|error| self.syntax(error.to_string()))?;
            let value = attribute
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map_err(|error| self.syntax(error.to_string()))?
                .into_owned();
            attributes.push(Attribute {
                name: qname(attribute.key),
                value,
            });
        }
        Ok(Element {
            name,
            attributes,
            children: Vec::new(),
        })
    }

    fn close(&mut self, element: Element) -> Result<(), ParseFailure> {
        match self.stack.last_mut() {
            Some(parent) => {
                parent.children.push(Node::Element(element));
                Ok(())
            }
            None => {
                self.root = Some(element);
                Ok(())
            }
        }
    }

    fn push_text(&mut self, content: &str) -> Result<(), ParseFailure> {
        if let Some(parent) = self.stack.last_mut() {
            if let Some(Node::Text(existing)) = parent.children.last_mut() {
                existing.push_str(content);
                return Ok(());
            }
            parent.children.push(Node::Text(content.to_string()));
            return Ok(());
        }
        if content.chars().all(char::is_whitespace) {
            return Ok(());
        }
        Err(self.syntax("text outside the root element".to_string()))
    }

    fn push_node(&mut self, node: Node) -> Result<(), ParseFailure> {
        match self.stack.last_mut() {
            Some(parent) => {
                parent.children.push(node);
                Ok(())
            }
            None => match node {
                Node::Comment(_) | Node::Instruction(_) => Ok(()),
                _ => Err(self.syntax("character data outside the root element".to_string())),
            },
        }
    }

    fn path(&self) -> String {
        let mut path = String::new();
        for element in &self.stack {
            path.push('/');
            path.push_str(&element.name);
        }
        if path.is_empty() {
            path.push('/');
        }
        path
    }

    fn syntax(&self, message: String) -> ParseFailure {
        self.failure(GpifProblem::Syntax { message })
    }

    fn failure(&self, problem: GpifProblem) -> ParseFailure {
        ParseFailure {
            path: self.path(),
            problem,
        }
    }
}

fn finish(mut element: Element) -> Element {
    if element.has_elements() {
        element.children.retain(|node| match node {
            Node::Text(text) => !text.chars().all(char::is_whitespace),
            _ => true,
        });
    }
    element
}

fn qname(name: QName<'_>) -> String {
    name.as_ref().to_string()
}

fn resolve_reference(reference: &BytesRef<'_>) -> Option<String> {
    match reference.resolve_char_ref() {
        Ok(Some(character)) => return Some(character.to_string()),
        Ok(None) => {}
        Err(_) => return None,
    }
    let text = match &**reference {
        "lt" => "<",
        "gt" => ">",
        "amp" => "&",
        "apos" => "'",
        "quot" => "\"",
        _ => return None,
    };
    Some(text.to_string())
}

pub(crate) fn write_node(node: &Node, out: &mut String) {
    match node {
        Node::Element(element) => write_element(element, out),
        Node::Text(text) => escape_text(text, out),
        Node::CData(text) => {
            out.push_str("<![CDATA[");
            out.push_str(text);
            out.push_str("]]>");
        }
        Node::Comment(text) => {
            out.push_str("<!--");
            out.push_str(text);
            out.push_str("-->");
        }
        Node::Instruction(text) => {
            out.push_str("<?");
            out.push_str(text);
            out.push_str("?>");
        }
    }
}

pub(crate) fn write_element(element: &Element, out: &mut String) {
    out.push('<');
    out.push_str(&element.name);
    for attribute in &element.attributes {
        out.push(' ');
        out.push_str(&attribute.name);
        out.push_str("=\"");
        escape_attribute(&attribute.value, out);
        out.push('"');
    }
    if element.children.is_empty() {
        out.push_str("/>");
        return;
    }
    out.push('>');
    for child in &element.children {
        write_node(child, out);
    }
    out.push_str("</");
    out.push_str(&element.name);
    out.push('>');
}

pub(crate) fn escape_text(text: &str, out: &mut String) {
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => out.push_str("&#13;"),
            _ => out.push(character),
        }
    }
}

pub(crate) fn escape_attribute(text: &str, out: &mut String) {
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            _ => out.push(character),
        }
    }
}
