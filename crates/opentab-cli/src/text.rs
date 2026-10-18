pub fn escape_controls(text: &str) -> String {
    escape(text, false)
}

pub fn escape_controls_keeping_lines(text: &str) -> String {
    escape(text, true)
}

fn escape(text: &str, keep_newlines: bool) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_control() && !(keep_newlines && character == '\n') {
            escaped.push_str(&format!("\\u{{{:x}}}", u32::from(character)));
        } else {
            escaped.push(character);
        }
    }
    escaped
}
