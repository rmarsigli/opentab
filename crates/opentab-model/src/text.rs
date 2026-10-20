#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextForm {
    Escaped,
    Literal,
}

fn is_xml_char(character: char) -> bool {
    matches!(character,
        '\t' | '\n' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..)
}

pub fn unwritable_character(text: &str, form: TextForm) -> Option<char> {
    text.chars().find(|character| {
        let escaped_return = form == TextForm::Escaped && *character == '\r';
        !escaped_return && !is_xml_char(*character)
    })
}
