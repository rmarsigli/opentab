use opentab_gp::{GpError, Refusal, read_gp, write_gp};
use opentab_model::{OpaquePayload, Score};

use crate::support::Doc;

fn base() -> Score {
    Doc::default().read()
}

fn with_track_payload(data: &str) -> Score {
    let mut score = base();
    let track = score.tracks[0].id.0;
    score
        .extensions
        .0
        .entry(track)
        .or_default()
        .push(OpaquePayload {
            format: "gpif".to_string(),
            data: data.to_string(),
        });
    score
}

fn malformed(score: &Score) -> String {
    match write_gp(score) {
        Err(GpError::Unsupported {
            reason: Refusal::MalformedPayload { problem },
            ..
        }) => problem,
        other => panic!(
            "expected a malformed payload refusal, got {:?}",
            other.map(|bytes| bytes.len())
        ),
    }
}

#[test]
fn parent_steps_must_be_xml_names() {
    for parent in ["a b", "X><Y", "Good/ba&d", "1st", "Na me#0", "-x"] {
        let data = format!("<Keep parent=\"{}\" index=\"0\"><A/></Keep>", {
            let mut escaped = String::new();
            for character in parent.chars() {
                match character {
                    '<' => escaped.push_str("&lt;"),
                    '>' => escaped.push_str("&gt;"),
                    '&' => escaped.push_str("&amp;"),
                    other => escaped.push(other),
                }
            }
            escaped
        });
        let problem = malformed(&with_track_payload(&data));
        assert!(
            problem.contains("is not an XML name"),
            "{parent}: {problem}"
        );
    }
}

#[test]
fn property_steps_take_any_attribute_text() {
    let score = with_track_payload(
        "<Keep parent=\"Extra/Property@a &amp; b &lt;c&gt;\" index=\"0\"><A/></Keep>",
    );
    let bytes = write_gp(&score).expect("a property name is an attribute value");
    let again = read_gp(&bytes).expect("the output is well formed");
    assert_eq!(again.tracks, score.tracks);
}

#[test]
fn kept_attribute_names_must_be_xml_names() {
    let score = with_track_payload(
        "<KeepAttribute parent=\"\" index=\"0\" name=\"x=&quot;1&quot; y\" value=\"v\"/>",
    );
    assert!(malformed(&score).contains("is not an XML name"));
}

#[test]
fn kept_nodes_must_use_xml_names() {
    for node in ["<A x&amp;y=\"1\"/>", "<A 1x=\"1\"/>"] {
        let data = format!("<Keep parent=\"\" index=\"0\">{node}</Keep>");
        let score = with_track_payload(&data);
        let refused = matches!(
            write_gp(&score),
            Err(GpError::Unsupported {
                reason: Refusal::MalformedPayload { .. },
                ..
            })
        );
        assert!(refused, "{node}");
    }
}

#[test]
fn a_huge_parent_path_is_refused_without_building_it() {
    let parent = format!("{}x", "x/".repeat(500_000));
    let data = format!("<Keep parent=\"{parent}\" index=\"0\"><A/></Keep>");
    let problem = malformed(&with_track_payload(&data));
    assert!(problem.contains("more than 256 steps"), "{problem}");
}

#[test]
fn parent_steps_plus_node_depth_stay_within_the_reader_limit() {
    let parent = vec!["x"; 250].join("/");
    let deep = format!("{}{}", "<A>".repeat(8), "</A>".repeat(8));
    let data = format!("<Keep parent=\"{parent}\" index=\"0\">{deep}</Keep>");
    let problem = malformed(&with_track_payload(&data));
    assert!(problem.contains("deeper than 256 levels"), "{problem}");
    let shallow = vec!["x"; 200].join("/");
    let data = format!("<Keep parent=\"{shallow}\" index=\"0\"><A/></Keep>");
    let score = with_track_payload(&data);
    let bytes = write_gp(&score).expect("within the limit");
    let again = read_gp(&bytes).expect("our reader accepts what we write");
    assert_eq!(again.tracks, score.tracks);
}

#[test]
fn a_comment_that_breaks_xml_is_refused() {
    for comment in ["<!--a--b-->", "<!--a--->"] {
        let data = format!("<Keep parent=\"\" index=\"0\">{comment}</Keep>");
        let refused = matches!(
            write_gp(&with_track_payload(&data)),
            Err(GpError::Unsupported {
                reason: Refusal::MalformedPayload { .. },
                ..
            })
        );
        assert!(refused, "{comment}");
    }
}

#[test]
fn parent_ordinals_must_be_plain_numbers() {
    for parent in ["Sounds/Sound#", "Sounds/Sound#+1", "Sounds/Sound#x"] {
        let data = format!("<Keep parent=\"{parent}\" index=\"0\"><A/></Keep>");
        let problem = malformed(&with_track_payload(&data));
        assert!(
            problem.contains("invalid parent ordinal"),
            "{parent}: {problem}"
        );
    }
}
