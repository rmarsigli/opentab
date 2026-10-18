use crate::{Command, ParseError};

const BOM: char = '\u{feff}';

#[derive(Debug, Clone, PartialEq)]
pub struct NumberedCommand {
    pub line: usize,
    pub command: Command,
}

pub fn parse_jsonl_lines(input: &str) -> Result<Vec<NumberedCommand>, ParseError> {
    let input = input.strip_prefix(BOM).unwrap_or(input);
    let mut commands = Vec::new();
    for (index, raw) in input.lines().enumerate() {
        let line = index.saturating_add(1);
        let text = raw.trim();
        if text.is_empty() || text.starts_with("//") {
            continue;
        }
        let command = serde_json::from_str(text).map_err(|error| ParseError {
            line,
            message: error.to_string(),
        })?;
        commands.push(NumberedCommand { line, command });
    }
    Ok(commands)
}

pub fn parse_jsonl(input: &str) -> Result<Vec<Command>, ParseError> {
    let numbered = parse_jsonl_lines(input)?;
    Ok(numbered
        .into_iter()
        .map(|numbered| numbered.command)
        .collect())
}
