use std::collections::HashSet;

const CDATA_OPEN: &str = "<![CDATA[";
const CDATA_CLOSE: &str = "]]>";
const LYRICS: &str = "<Lyrics";
const LINE_OPEN: &str = "<Line>";
const LINE_CLOSE: &str = "</Line>";
const TEXT_OPEN: &str = "<Text>";
const TEXT_CLOSE: &str = "</Text>";
const OFFSET_OPEN: &str = "<Offset>";
const OFFSET_CLOSE: &str = "</Offset>";
const LEGATO: &str = "<Legato ";
const LEGATO_ENDS: [&str; 2] = ["origin=\"true\"", "destination=\"true\""];
const CHORD_OPEN: &str = "<Chord>";
const CHORD_CLOSE: &str = "</Chord>";
const DIAGRAMS: &str = "name=\"DiagramCollection\"";
const ITEM: &str = "<Item ";
const ITEM_ID: &str = "id=\"";
const DIAGRAM: &str = "<Diagram";
const SYLLABLE_BREAKS: [char; 1] = ['-'];
const UNMAPPED_WHAMMY: &str = "guard=\"tremolo-bar:none\"";
const WHAMMY: &str = "<Whammy ";
const WHAMMY_VALUES: [&str; 3] = ["originValue=\"", "middleValue=\"", "destinationValue=\""];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricLine {
    pub offset: usize,
    pub syllables: usize,
}

fn visible(text: &str) -> bool {
    text.chars().any(|character| !character.is_whitespace())
}

pub fn visible_text(fragment: &str) -> String {
    let mut out = String::new();
    let mut rest = fragment;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix(CDATA_OPEN) {
            let (inner, tail) = after.split_once(CDATA_CLOSE).unwrap_or((after, ""));
            out.push_str(inner);
            rest = tail;
        } else if rest.starts_with('<') {
            rest = rest.split_once('>').map_or("", |(_, tail)| tail);
        } else {
            let end = rest.find('<').unwrap_or(rest.len());
            out.push_str(&rest[..end]);
            rest = &rest[end..];
        }
    }
    out
}

pub fn has_visible_text(fragment: &str) -> bool {
    visible(&visible_text(fragment))
}

fn without_offsets(data: &str) -> String {
    let mut out = String::with_capacity(data.len());
    let mut rest = data;
    while let Some((before, after)) = rest.split_once(OFFSET_OPEN) {
        out.push_str(before);
        rest = after.split_once(OFFSET_CLOSE).map_or("", |(_, tail)| tail);
    }
    out.push_str(rest);
    out
}

pub fn holds_lyrics(data: &str) -> bool {
    data.contains(LYRICS) && has_visible_text(&without_offsets(data))
}

fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let (_, after) = text.split_once(open)?;
    after.split_once(close).map(|(inner, _)| inner)
}

pub fn syllables(text: &str) -> usize {
    text.split(|character: char| character.is_whitespace() || SYLLABLE_BREAKS.contains(&character))
        .filter(|piece| !piece.is_empty())
        .count()
}

pub fn lyric_lines(data: &str) -> Vec<LyricLine> {
    if !data.contains(LYRICS) {
        return Vec::new();
    }
    data.split(LINE_OPEN)
        .skip(1)
        .filter_map(|line| {
            let line = line.split(LINE_CLOSE).next().unwrap_or_default();
            let text = between(line, TEXT_OPEN, TEXT_CLOSE)?;
            let count = syllables(&visible_text(text));
            (count > 0).then(|| LyricLine {
                offset: between(line, OFFSET_OPEN, OFFSET_CLOSE)
                    .and_then(|offset| offset.trim().parse().ok())
                    .unwrap_or(0),
                syllables: count,
            })
        })
        .collect()
}

pub fn holds_legato(data: &str) -> bool {
    data.split(LEGATO).skip(1).any(|attributes| {
        let tag = attributes.split('>').next().unwrap_or_default();
        LEGATO_ENDS.iter().any(|end| tag.contains(end))
    })
}

pub fn chord_reference(data: &str) -> Option<String> {
    let inner = between(data, CHORD_OPEN, CHORD_CLOSE)?.trim();
    let inner = inner
        .strip_prefix(CDATA_OPEN)
        .and_then(|text| text.strip_suffix(CDATA_CLOSE))
        .unwrap_or(inner);
    Some(inner.trim().to_string())
}

pub fn diagram_ids(data: &str) -> HashSet<String> {
    if !data.contains(DIAGRAMS) {
        return HashSet::new();
    }
    data.split(ITEM)
        .skip(1)
        .filter(|item| item.contains(DIAGRAM))
        .filter_map(|item| {
            let tag = item.split('>').next()?;
            let (_, after) = tag.split_once(ITEM_ID)?;
            let (id, _) = after.split_once('"')?;
            Some(id.to_string())
        })
        .collect()
}

fn moves(value: &str) -> bool {
    value
        .trim()
        .trim_start_matches(['-', '+'])
        .chars()
        .any(|character| character.is_ascii_digit() && character != '0')
}

pub fn holds_unmapped_whammy(data: &str) -> bool {
    if !data.contains(UNMAPPED_WHAMMY) {
        return false;
    }
    data.split(WHAMMY).skip(1).any(|rest| {
        let tag = rest.split('>').next().unwrap_or_default();
        WHAMMY_VALUES.iter().any(|name| {
            tag.split(name)
                .nth(1)
                .and_then(|value| value.split('"').next())
                .is_some_and(moves)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_text_ignores_tags_attributes_blank_cdata_and_offsets() {
        assert!(!has_visible_text("<a x=\"word\"><![CDATA[  ]]></a>"));
        assert!(has_visible_text("<a><![CDATA[ la ]]></a>"));
        assert!(has_visible_text("<a> la </a>"));
        assert!(!has_visible_text("<a>\n\t</a><b/>"));
        assert!(!has_visible_text("<![CDATA[ "));
        let track = "<Keep parent=\"\" index=\"18\" bars=\"1 1\"><Lyrics dispatched=\"true\"><Line><Text><![CDATA[]]></Text><Offset>0</Offset></Line></Lyrics></Keep>";
        assert!(!holds_lyrics(track));
        assert!(holds_lyrics(
            &track.replace("<![CDATA[]]>", "<![CDATA[Hal-le-lu-jah]]>")
        ));
        let beat = "<Keep parent=\"\" index=\"6\"><Lyrics><Line><![CDATA[A-]]></Line><Line><![CDATA[]]></Line></Lyrics></Keep>";
        assert!(holds_lyrics(beat));
        assert!(!holds_lyrics(
            "<Keep><Text><![CDATA[not lyrics]]></Text></Keep>"
        ));
    }

    #[test]
    fn lyric_lines_give_their_start_bar_and_syllable_count() {
        let track = "<Keep><Lyrics dispatched=\"true\">\
            <Line><Text><![CDATA[A-ma-zing grace]]></Text><Offset>2</Offset></Line>\
            <Line><Text><![CDATA[  ]]></Text><Offset>0</Offset></Line>\
            <Line><Text>how sweet</Text><Offset>x</Offset></Line></Lyrics></Keep>";
        assert_eq!(
            lyric_lines(track),
            vec![
                LyricLine {
                    offset: 2,
                    syllables: 4
                },
                LyricLine {
                    offset: 0,
                    syllables: 2
                }
            ]
        );
        assert!(lyric_lines("<Keep><Line><Text>a</Text></Line></Keep>").is_empty());
        assert_eq!(syllables(" -a--b c- "), 3);
    }

    #[test]
    fn legato_counts_only_when_one_end_is_set() {
        assert!(holds_legato(
            "<Keep><Legato origin=\"true\" destination=\"false\"/></Keep>"
        ));
        assert!(holds_legato(
            "<Keep><Legato origin=\"false\" destination=\"true\"/></Keep>"
        ));
        assert!(!holds_legato(
            "<Keep><Legato origin=\"false\" destination=\"false\"/></Keep>"
        ));
        assert!(!holds_legato(
            "<Keep><Property name=\"origin=&quot;true&quot;\"/></Keep>"
        ));
    }

    #[test]
    fn only_a_moving_whammy_the_model_could_not_hold_counts() {
        let kept = "<Keep parent=\"\" index=\"3\" guard=\"tremolo-bar:none\"><Whammy originValue=\"0\" middleValue=\"-50.25\" destinationValue=\"0\"/></Keep>";
        assert!(holds_unmapped_whammy(kept));
        assert!(!holds_unmapped_whammy(&kept.replace("-50.25", "-0.0")));
        assert!(!holds_unmapped_whammy(&kept.replace(
            "tremolo-bar:none",
            "tremolo-bar:0 0;35 -100;35 -100;99 -200"
        )));
        assert!(!holds_unmapped_whammy(
            "<Keep guard=\"tremolo-bar:none\"><Hairpin>Crescendo</Hairpin></Keep>"
        ));
    }

    #[test]
    fn chord_references_and_diagram_items_are_read_from_their_payloads() {
        assert_eq!(
            chord_reference("<Keep guard=\"chord:=E\"><Chord><![CDATA[6]]></Chord></Keep>"),
            Some("6".to_string())
        );
        assert_eq!(
            chord_reference("<Keep><Chord> 7 </Chord></Keep>"),
            Some("7".to_string())
        );
        assert_eq!(
            chord_reference("<Keep><Hairpin>Crescendo</Hairpin></Keep>"),
            None
        );
        let collection = "<Keep parent=\"Properties\"><Property name=\"DiagramCollection\"><Items>\
            <Item id=\"0\" name=\"A5\"><Diagram stringCount=\"6\"/></Item>\
            <Item id=\"1\" name=\"N.C.\"/>\
            <Item id=\"2\" name=\"D\"><Chord/><Diagram/></Item></Items></Property></Keep>";
        let mut ids: Vec<String> = diagram_ids(collection).into_iter().collect();
        ids.sort();
        assert_eq!(ids, vec!["0".to_string(), "2".to_string()]);
        assert!(
            diagram_ids(&collection.replace("DiagramCollection", "DiagramWorkingSet")).is_empty()
        );
    }
}
