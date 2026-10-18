use opentab_command::{Command, MetaField, ParseError, parse_jsonl};
use opentab_model::{BeatId, ElementId};

fn title() -> Command {
    Command::SetMeta {
        field: MetaField::Title,
        value: "Song".to_owned(),
    }
}

#[test]
fn blank_lines_and_comment_lines_are_skipped() {
    let input = "\n// header\n{\"op\": \"set_meta\", \"field\": \"title\", \"value\": \"Song\"}\n   \n  // indented\n{\"op\": \"remove_beat\", \"beat\": 4}\n\n";
    let commands = parse_jsonl(input).unwrap();
    assert_eq!(
        commands,
        vec![
            title(),
            Command::RemoveBeat {
                beat: BeatId(ElementId(4))
            }
        ]
    );
}

#[test]
fn an_empty_input_is_an_empty_batch() {
    assert_eq!(parse_jsonl(""), Ok(Vec::new()));
    assert_eq!(parse_jsonl("\n\n// only comments\n"), Ok(Vec::new()));
}

#[test]
fn a_bad_third_line_is_reported_as_line_three() {
    let input = "{\"op\": \"set_meta\", \"field\": \"title\", \"value\": \"Song\"}\n// note\n{\"op\": \"remove_beat\", \"beat\": }\n{\"op\": \"remove_beat\", \"beat\": 1}\n";
    let error = parse_jsonl(input).unwrap_err();
    assert_eq!(error.line, 3);
    assert!(error.to_string().starts_with("line 3: "));
}

#[test]
fn an_unknown_op_is_an_error_on_its_line() {
    let input = "\n{\"op\": \"transpose_everything\"}\n";
    let ParseError { line, message } = parse_jsonl(input).unwrap_err();
    assert_eq!(line, 2);
    assert!(message.contains("transpose_everything"), "{message}");
}

#[test]
fn crlf_line_endings_and_a_byte_order_mark_are_accepted() {
    let input = "\u{feff}{\"op\": \"set_meta\", \"field\": \"title\", \"value\": \"Song\"}\r\n// c\r\n{\"op\": \"set_meta\", \"field\": \"title\", \"value\": \"Song\"}\r\n";
    assert_eq!(parse_jsonl(input), Ok(vec![title(), title()]));
}

#[test]
fn a_second_line_error_after_crlf_keeps_its_line_number() {
    let input = "{\"op\": \"set_meta\", \"field\": \"title\", \"value\": \"Song\"}\r\nnot json\r\n";
    assert_eq!(parse_jsonl(input).unwrap_err().line, 2);
}
