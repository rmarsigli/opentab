use std::collections::{BTreeSet, HashSet};

use opentab_model::{Bar, Beat, ElementId, MasterBar, Note, NoteKind, Score, Staff};

use crate::drums::DrumKit;
use crate::select::Selection;

const CDATA_OPEN: &str = "<![CDATA[";
const CDATA_CLOSE: &str = "]]>";
const LYRICS: &str = "<Lyrics";
const OFFSET_OPEN: &str = "<Offset>";
const OFFSET_CLOSE: &str = "</Offset>";
const FERMATAS: [&str; 2] = ["<Fermata>", "<Fermata "];
const HAIRPIN: &str = "<Hairpin>";
const LEGATO: &str = "<Legato ";
const LEGATO_ENDS: [&str; 2] = ["origin=\"true\"", "destination=\"true\""];
const CHORD_OPEN: &str = "<Chord>";
const CHORD_CLOSE: &str = "</Chord>";
const DIAGRAMS: &str = "name=\"DiagramCollection\"";
const ITEM: &str = "<Item ";
const ITEM_ID: &str = "id=\"";
const DIAGRAM: &str = "<Diagram";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Unrendered {
    ChordDiagrams,
    Fermatas,
    Hairpins,
    LegatoSlurs,
    Lyrics,
    MultiBarRests,
    PercussionArticulations,
}

pub const UNRENDERED: [Unrendered; 7] = [
    Unrendered::ChordDiagrams,
    Unrendered::Fermatas,
    Unrendered::Hairpins,
    Unrendered::LegatoSlurs,
    Unrendered::Lyrics,
    Unrendered::MultiBarRests,
    Unrendered::PercussionArticulations,
];

impl Unrendered {
    pub fn name(self) -> &'static str {
        match self {
            Unrendered::ChordDiagrams => "chord_diagrams",
            Unrendered::Fermatas => "fermatas",
            Unrendered::Hairpins => "hairpins",
            Unrendered::LegatoSlurs => "legato_slurs",
            Unrendered::Lyrics => "lyrics",
            Unrendered::MultiBarRests => "multi_bar_rests",
            Unrendered::PercussionArticulations => "percussion_articulations",
        }
    }
}

fn payloads(score: &Score, id: ElementId) -> impl Iterator<Item = &str> {
    score
        .extensions
        .0
        .get(&id)
        .into_iter()
        .flatten()
        .map(|payload| payload.data.as_str())
}

fn visible(text: &str) -> bool {
    text.chars().any(|character| !character.is_whitespace())
}

pub fn has_visible_text(fragment: &str) -> bool {
    let mut rest = fragment;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix(CDATA_OPEN) {
            let (inner, tail) = after.split_once(CDATA_CLOSE).unwrap_or((after, ""));
            if visible(inner) {
                return true;
            }
            rest = tail;
        } else if rest.starts_with('<') {
            rest = rest.split_once('>').map_or("", |(_, tail)| tail);
        } else {
            let end = rest.find('<').unwrap_or(rest.len());
            if visible(&rest[..end]) {
                return true;
            }
            rest = &rest[end..];
        }
    }
    false
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

fn holds_fermata(data: &str) -> bool {
    FERMATAS.iter().any(|tag| data.contains(tag))
}

fn holds_legato(data: &str) -> bool {
    data.split(LEGATO).skip(1).any(|attributes| {
        let tag = attributes.split('>').next().unwrap_or_default();
        LEGATO_ENDS.iter().any(|end| tag.contains(end))
    })
}

fn chord_reference(data: &str) -> Option<String> {
    let (_, after) = data.split_once(CHORD_OPEN)?;
    let (inner, _) = after.split_once(CHORD_CLOSE)?;
    let inner = inner.trim();
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

struct Walk<'a> {
    score: &'a Score,
    found: BTreeSet<Unrendered>,
}

impl Walk<'_> {
    fn note(&mut self, note: &Note, kit: Option<&DrumKit>) {
        if let (NoteKind::Percussion { articulation }, Some(kit)) = (note.kind, kit)
            && !kit.entry(articulation).fully_drawn()
        {
            self.found.insert(Unrendered::PercussionArticulations);
        }
    }

    fn beat(&mut self, beat: &Beat, diagrams: &HashSet<String>, kit: Option<&DrumKit>) {
        for data in payloads(self.score, beat.id.0) {
            if data.contains(HAIRPIN) {
                self.found.insert(Unrendered::Hairpins);
            }
            if holds_legato(data) {
                self.found.insert(Unrendered::LegatoSlurs);
            }
            if holds_lyrics(data) {
                self.found.insert(Unrendered::Lyrics);
            }
            if beat.effects.chord_name.is_some()
                && chord_reference(data).is_some_and(|id| diagrams.contains(&id))
            {
                self.found.insert(Unrendered::ChordDiagrams);
            }
        }
        for note in &beat.notes {
            self.note(note, kit);
        }
    }

    fn staff(&mut self, staff: &Staff, bars: (usize, usize), kit: Option<&DrumKit>) {
        let diagrams: HashSet<String> = payloads(self.score, staff.id.0)
            .flat_map(diagram_ids)
            .collect();
        for bar in staff.bars.iter().take(bars.1 + 1).skip(bars.0) {
            for voice in &bar.voices {
                for beat in &voice.beats {
                    self.beat(beat, &diagrams, kit);
                }
            }
        }
    }
}

fn beats(bar: Option<&Bar>) -> impl Iterator<Item = &Beat> {
    bar.into_iter()
        .flat_map(|bar| &bar.voices)
        .flat_map(|voice| &voice.beats)
}

fn continues(before: &MasterBar, after: &MasterBar) -> bool {
    !after.repeat_start
        && after.marker.is_none()
        && after.alternate_endings == 0
        && after.tempo_changes.is_empty()
        && after.time_signature == before.time_signature
        && after.key_signature == before.key_signature
        && before.repeat_end.is_none()
        && before.alternate_endings == 0
        && !before.double_bar
}

fn multi_bar_rest(score: &Score, selection: &Selection) -> bool {
    let silent_at = |index: usize| {
        let bars: Vec<Option<&Bar>> = selection
            .sources
            .iter()
            .map(|(track, staff)| {
                score
                    .tracks
                    .get(*track)
                    .and_then(|track| track.staves.get(*staff))
                    .and_then(|staff| staff.bars.get(index))
            })
            .collect();
        (index > 0 || !score.anacrusis)
            && bars
                .iter()
                .all(|bar| beats(*bar).all(|beat| beat.notes.is_empty()))
            && bars.iter().any(|bar| beats(*bar).next().is_some())
    };
    (selection.first..selection.last).any(|index| {
        match (
            score.master_bars.get(index),
            score.master_bars.get(index + 1),
        ) {
            (Some(before), Some(after)) => {
                continues(before, after) && silent_at(index) && silent_at(index + 1)
            }
            _ => false,
        }
    })
}

pub fn unrendered(score: &Score, selection: &Selection) -> Vec<Unrendered> {
    let mut walk = Walk {
        score,
        found: BTreeSet::new(),
    };
    let bars = (selection.first, selection.last);
    for master in score.master_bars.iter().take(bars.1 + 1).skip(bars.0) {
        if payloads(score, master.id.0).any(holds_fermata) {
            walk.found.insert(Unrendered::Fermatas);
        }
    }
    for index in &selection.tracks {
        let Some(track) = score.tracks.get(*index) else {
            continue;
        };
        if payloads(score, track.id.0).any(holds_lyrics) {
            walk.found.insert(Unrendered::Lyrics);
        }
    }
    for (track, staff) in &selection.sources {
        let Some(model) = score.tracks.get(*track) else {
            continue;
        };
        let kit = selection.kits.get(*track).and_then(Option::as_ref);
        if let Some(staff) = model.staves.get(*staff) {
            walk.staff(staff, bars, kit);
        }
    }
    if multi_bar_rest(score, selection) {
        walk.found.insert(Unrendered::MultiBarRests);
    }
    walk.found.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_snake_case_and_sorted_like_the_variants() {
        let names: Vec<&str> = UNRENDERED.iter().map(|feature| feature.name()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names, sorted);
        let mut variants = UNRENDERED.to_vec();
        variants.sort();
        assert_eq!(variants, UNRENDERED.to_vec());
        for name in names {
            assert!(
                name.chars()
                    .all(|character| character.is_ascii_lowercase() || character == '_'),
                "{name}"
            );
        }
    }

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
        let ids = diagram_ids(collection);
        let mut ids: Vec<String> = ids.into_iter().collect();
        ids.sort();
        assert_eq!(ids, vec!["0".to_string(), "2".to_string()]);
        assert!(
            diagram_ids(&collection.replace("DiagramCollection", "DiagramWorkingSet")).is_empty()
        );
    }
}
