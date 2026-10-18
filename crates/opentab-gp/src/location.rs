use crate::dom::{Element, Node};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Location(Vec<usize>);

impl Location {
    pub fn root() -> Location {
        Location(Vec::new())
    }

    pub fn child(&self, index: usize) -> Location {
        let mut steps = self.0.clone();
        steps.push(index);
        Location(steps)
    }

    pub fn render(&self, root: &Element) -> String {
        let mut path = format!("/{}", root.name);
        let mut current = root;
        for index in &self.0 {
            let Some(Node::Element(child)) = current.children.get(*index) else {
                break;
            };
            path.push('/');
            path.push_str(&step(current, *index, child));
            current = child;
        }
        path
    }
}

fn step(parent: &Element, index: usize, child: &Element) -> String {
    match child.name.as_str() {
        "Property" => {
            return format!(
                "Property[@name=\"{}\"]",
                child.attribute("name").unwrap_or_default()
            );
        }
        "XProperty" => {
            return format!(
                "XProperty[@id=\"{}\"]",
                child.attribute("id").unwrap_or_default()
            );
        }
        _ => {}
    }
    let same: Vec<usize> = parent
        .elements()
        .filter(|(_, sibling)| sibling.name == child.name)
        .map(|(position, _)| position)
        .collect();
    if same.len() > 1 {
        let ordinal = same
            .iter()
            .position(|position| *position == index)
            .unwrap_or(0)
            + 1;
        format!("{}[{ordinal}]", child.name)
    } else {
        child.name.clone()
    }
}
