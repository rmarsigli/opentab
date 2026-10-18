use crate::dom::{Element, Node};

pub(crate) fn single<'e>(property: &'e Element, name: &str) -> Option<&'e Element> {
    match property.children.as_slice() {
        [Node::Element(child)] if child.name == name => Some(child),
        _ => None,
    }
}

pub(crate) fn single_text(property: &Element, name: &str) -> Option<String> {
    single(property, name)
        .filter(|child| !child.has_elements() && child.attributes.is_empty())
        .map(Element::text)
}

pub(crate) fn is_flag(property: &Element) -> bool {
    single(property, "Enable")
        .is_some_and(|enable| enable.children.is_empty() && enable.attributes.is_empty())
}

pub(crate) fn find<'e>(element: &'e Element, container: &str, key: &str) -> Option<&'e Element> {
    let (_, container) = element.child(container)?;
    container
        .elements()
        .find(|(_, child)| child.key() == key)
        .map(|(_, child)| child)
}
