mod payload;

use std::collections::{BTreeSet, HashSet};

use opentab_model::{
    Bar, Beat, ElementId, HarmonicKind, MasterBar, Note, NoteKind, Score, Staff, Track,
};

pub use payload::{
    LyricLine, chord_reference, diagram_ids, has_visible_text, holds_legato, holds_lyrics,
    holds_unmapped_whammy, lyric_lines, syllables, visible_text,
};

use crate::display::{Page, Primitive};
use crate::drums::DrumKit;
use crate::marks::clean;
use crate::select::Selection;
use crate::structure::StaffKind;
use crate::text::font;

const NONE_VALUE: &str = "None";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Unrendered {
    Accents,
    Arpeggios,
    Bends,
    ChordDiagrams,
    Copyright,
    Directions,
    FadeIn,
    Fermatas,
    Golpe,
    Hairpins,
    HammerOnPullOff,
    HarmonicTypes,
    Instructions,
    LegatoSlurs,
    LetRing,
    LetRingThroughout,
    Lyrics,
    MissingCharacters,
    MultiBarRests,
    Notices,
    Ornaments,
    PalmMute,
    PercussionArticulations,
    Pop,
    RightHandFingering,
    SecondaryVoiceRhythm,
    SimileMarks,
    Slap,
    SlashNotation,
    Slides,
    Staccato,
    Strokes,
    Tapping,
    Trill,
    TripletFeel,
    Vibrato,
    Wah,
    WhammyBar,
}

pub const UNRENDERED: [Unrendered; 38] = [
    Unrendered::Accents,
    Unrendered::Arpeggios,
    Unrendered::Bends,
    Unrendered::ChordDiagrams,
    Unrendered::Copyright,
    Unrendered::Directions,
    Unrendered::FadeIn,
    Unrendered::Fermatas,
    Unrendered::Golpe,
    Unrendered::Hairpins,
    Unrendered::HammerOnPullOff,
    Unrendered::HarmonicTypes,
    Unrendered::Instructions,
    Unrendered::LegatoSlurs,
    Unrendered::LetRing,
    Unrendered::LetRingThroughout,
    Unrendered::Lyrics,
    Unrendered::MissingCharacters,
    Unrendered::MultiBarRests,
    Unrendered::Notices,
    Unrendered::Ornaments,
    Unrendered::PalmMute,
    Unrendered::PercussionArticulations,
    Unrendered::Pop,
    Unrendered::RightHandFingering,
    Unrendered::SecondaryVoiceRhythm,
    Unrendered::SimileMarks,
    Unrendered::Slap,
    Unrendered::SlashNotation,
    Unrendered::Slides,
    Unrendered::Staccato,
    Unrendered::Strokes,
    Unrendered::Tapping,
    Unrendered::Trill,
    Unrendered::TripletFeel,
    Unrendered::Vibrato,
    Unrendered::Wah,
    Unrendered::WhammyBar,
];

impl Unrendered {
    pub fn name(self) -> &'static str {
        match self {
            Unrendered::Accents => "accents",
            Unrendered::Arpeggios => "arpeggios",
            Unrendered::Bends => "bends",
            Unrendered::ChordDiagrams => "chord_diagrams",
            Unrendered::Copyright => "copyright",
            Unrendered::Directions => "directions",
            Unrendered::FadeIn => "fade_in",
            Unrendered::Fermatas => "fermatas",
            Unrendered::Golpe => "golpe",
            Unrendered::Hairpins => "hairpins",
            Unrendered::HammerOnPullOff => "hammer_on_pull_off",
            Unrendered::HarmonicTypes => "harmonic_types",
            Unrendered::Instructions => "instructions",
            Unrendered::LegatoSlurs => "legato_slurs",
            Unrendered::LetRing => "let_ring",
            Unrendered::LetRingThroughout => "let_ring_throughout",
            Unrendered::Lyrics => "lyrics",
            Unrendered::MissingCharacters => "missing_characters",
            Unrendered::MultiBarRests => "multi_bar_rests",
            Unrendered::Notices => "notices",
            Unrendered::Ornaments => "ornaments",
            Unrendered::PalmMute => "palm_mute",
            Unrendered::PercussionArticulations => "percussion_articulations",
            Unrendered::Pop => "pop",
            Unrendered::RightHandFingering => "right_hand_fingering",
            Unrendered::SecondaryVoiceRhythm => "secondary_voice_rhythm",
            Unrendered::SimileMarks => "simile_marks",
            Unrendered::Slap => "slap",
            Unrendered::SlashNotation => "slash_notation",
            Unrendered::Slides => "slides",
            Unrendered::Staccato => "staccato",
            Unrendered::Strokes => "strokes",
            Unrendered::Tapping => "tapping",
            Unrendered::Trill => "trill",
            Unrendered::TripletFeel => "triplet_feel",
            Unrendered::Vibrato => "vibrato",
            Unrendered::Wah => "wah",
            Unrendered::WhammyBar => "whammy_bar",
        }
    }
}

pub const MARKED: [(&str, Unrendered); 10] = [
    ("Fermata", Unrendered::Fermatas),
    ("Hairpin", Unrendered::Hairpins),
    ("TripletFeel", Unrendered::TripletFeel),
    ("Arpeggio", Unrendered::Arpeggios),
    ("Wah", Unrendered::Wah),
    ("Golpe", Unrendered::Golpe),
    ("Ornament", Unrendered::Ornaments),
    ("SimileMark", Unrendered::SimileMarks),
    ("Slashed", Unrendered::SlashNotation),
    ("LetRingThroughout", Unrendered::LetRingThroughout),
];

pub fn marked(data: &str, tag: &str) -> bool {
    let open = format!("<{tag}");
    data.split(open.as_str()).skip(1).any(|rest| {
        let Some(next) = rest.chars().next() else {
            return false;
        };
        if !matches!(next, '>' | ' ' | '/') {
            return false;
        }
        let close = format!("</{tag}>");
        let inner = rest
            .strip_prefix('>')
            .and_then(|body| body.split(close.as_str()).next())
            .map(|body| visible_text(body).trim().to_string());
        inner.as_deref() != Some(NONE_VALUE)
    })
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Staves {
    standard: bool,
    tab: bool,
}

struct Walk<'a> {
    score: &'a Score,
    found: BTreeSet<Unrendered>,
}

impl Walk<'_> {
    fn add(&mut self, wanted: bool, feature: Unrendered) {
        if wanted {
            self.found.insert(feature);
        }
    }

    fn element(&mut self, id: ElementId) {
        let score = self.score;
        for data in payloads(score, id) {
            for (tag, feature) in MARKED {
                self.add(marked(data, tag), feature);
            }
            self.add(holds_legato(data), Unrendered::LegatoSlurs);
            self.add(holds_unmapped_whammy(data), Unrendered::WhammyBar);
        }
    }

    fn note(&mut self, note: &Note, staves: Staves, kit: Option<&DrumKit>) {
        self.element(note.id.0);
        let effects = &note.effects;
        let hidden = !staves.tab;
        self.add(hidden && effects.palm_mute, Unrendered::PalmMute);
        self.add(hidden && effects.let_ring, Unrendered::LetRing);
        self.add(hidden && effects.vibrato.is_some(), Unrendered::Vibrato);
        self.add(
            hidden && effects.hammer_on_pull_off,
            Unrendered::HammerOnPullOff,
        );
        self.add(hidden && effects.slide.is_some(), Unrendered::Slides);
        self.add(hidden && effects.bend.is_some(), Unrendered::Bends);
        self.add(hidden && effects.tap.is_some(), Unrendered::Tapping);
        let typed = effects
            .harmonic
            .is_some_and(|harmonic| harmonic.kind != HarmonicKind::Natural);
        self.add(hidden && typed, Unrendered::HarmonicTypes);
        let plain = !staves.standard;
        self.add(plain && effects.accent.is_some(), Unrendered::Accents);
        self.add(plain && effects.staccato, Unrendered::Staccato);
        self.add(
            effects.right_hand_finger.is_some(),
            Unrendered::RightHandFingering,
        );
        self.add(effects.trill.is_some(), Unrendered::Trill);
        if let NoteKind::Percussion { articulation } = note.kind {
            let lost = kit.is_none_or(|kit| !kit.entry(articulation).fully_drawn());
            self.add(lost, Unrendered::PercussionArticulations);
        }
    }

    fn beat(&mut self, beat: &Beat, staves: Staves, diagrams: &HashSet<String>) {
        self.element(beat.id.0);
        let score = self.score;
        for data in payloads(score, beat.id.0) {
            self.add(holds_lyrics(data), Unrendered::Lyrics);
            let diagram = beat.effects.chord_name.is_some()
                && chord_reference(data).is_some_and(|id| diagrams.contains(&id));
            self.add(diagram, Unrendered::ChordDiagrams);
        }
        let effects = &beat.effects;
        self.add(effects.fade_in, Unrendered::FadeIn);
        let whammy = effects.tremolo_bar.is_some_and(|bar| bar.moves());
        self.add(whammy, Unrendered::WhammyBar);
        self.add(effects.slap, Unrendered::Slap);
        self.add(effects.pop, Unrendered::Pop);
        self.add(!staves.tab && effects.stroke.is_some(), Unrendered::Strokes);
    }

    fn bar(
        &mut self,
        bar: &Bar,
        staves: Staves,
        diagrams: &HashSet<String>,
        kit: Option<&DrumKit>,
    ) {
        self.element(bar.id.0);
        let sounding = bar
            .voices
            .iter()
            .filter(|voice| voice.beats.iter().any(|beat| !beat.notes.is_empty()))
            .count();
        self.add(
            staves.tab && !staves.standard && sounding >= 2,
            Unrendered::SecondaryVoiceRhythm,
        );
        for voice in &bar.voices {
            self.element(voice.id.0);
            for beat in &voice.beats {
                self.beat(beat, staves, diagrams);
                for note in &beat.notes {
                    self.note(note, staves, kit);
                }
            }
        }
    }

    fn staff(
        &mut self,
        staff: &Staff,
        bars: (usize, usize),
        staves: Staves,
        kit: Option<&DrumKit>,
    ) {
        self.element(staff.id.0);
        let score = self.score;
        let diagrams: HashSet<String> = payloads(score, staff.id.0).flat_map(diagram_ids).collect();
        for bar in staff.bars.iter().take(bars.1 + 1).skip(bars.0) {
            self.bar(bar, staves, &diagrams, kit);
        }
    }
}

fn sung_bars(track: &Track, line: &LyricLine) -> Vec<usize> {
    let Some(staff) = track.staves.first() else {
        return Vec::new();
    };
    let mut left = line.syllables;
    let mut out = Vec::new();
    for (index, bar) in staff.bars.iter().enumerate().skip(line.offset) {
        if left == 0 {
            break;
        }
        let sung = bar
            .voices
            .first()
            .into_iter()
            .flat_map(|voice| &voice.beats)
            .filter(|beat| beat.effects.grace.is_none() && !beat.notes.is_empty())
            .take(left)
            .count();
        if sung > 0 {
            out.push(index);
        }
        left -= sung;
    }
    out
}

fn track_lyrics(score: &Score, track: &Track, bars: (usize, usize)) -> bool {
    payloads(score, track.id.0)
        .flat_map(lyric_lines)
        .any(|line| {
            sung_bars(track, &line)
                .iter()
                .any(|bar| (bars.0..=bars.1).contains(bar))
        })
}

fn silent(bar: Option<&Bar>) -> impl Iterator<Item = &Beat> {
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
                .all(|bar| silent(*bar).all(|beat| beat.notes.is_empty()))
            && bars.iter().any(|bar| silent(*bar).next().is_some())
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

fn texts(items: &[Primitive], out: &mut Vec<(char, crate::display::Font)>) {
    for item in items {
        match item {
            Primitive::Text(run) => {
                out.extend(run.text.chars().map(|character| (character, run.font)))
            }
            Primitive::Group(group) => texts(&group.items, out),
            _ => {}
        }
    }
}

pub fn missing_characters(pages: &[Page]) -> bool {
    let mut found = Vec::new();
    for page in pages {
        texts(&page.items, &mut found);
    }
    found
        .iter()
        .any(|(character, face)| !character.is_whitespace() && !font(*face).has(*character))
}

fn staves_of(selection: &Selection, track: usize, staff: usize) -> Staves {
    selection
        .specs
        .iter()
        .filter(|spec| spec.track == track && spec.staff == staff)
        .fold(Staves::default(), |staves, spec| Staves {
            standard: staves.standard || spec.kind == StaffKind::Standard,
            tab: staves.tab || spec.kind == StaffKind::Tab,
        })
}

pub fn unrendered(score: &Score, selection: &Selection, pages: &[Page]) -> Vec<Unrendered> {
    let mut walk = Walk {
        score,
        found: BTreeSet::new(),
    };
    let bars = (selection.first, selection.last);
    for master in score.master_bars.iter().take(bars.1 + 1).skip(bars.0) {
        walk.element(master.id.0);
        walk.add(!master.directions.is_empty(), Unrendered::Directions);
    }
    for index in &selection.tracks {
        let Some(track) = score.tracks.get(*index) else {
            continue;
        };
        walk.element(track.id.0);
        walk.add(track_lyrics(score, track, bars), Unrendered::Lyrics);
    }
    for (track, staff) in &selection.sources {
        let Some(model) = score.tracks.get(*track) else {
            continue;
        };
        let kit = selection.kits.get(*track).and_then(Option::as_ref);
        if let Some(found) = model.staves.get(*staff) {
            walk.staff(found, bars, staves_of(selection, *track, *staff), kit);
        }
    }
    walk.add(multi_bar_rest(score, selection), Unrendered::MultiBarRests);
    let meta = &score.meta;
    walk.add(!clean(&meta.copyright).is_empty(), Unrendered::Copyright);
    walk.add(
        !clean(&meta.instructions).is_empty(),
        Unrendered::Instructions,
    );
    walk.add(!clean(&meta.notices).is_empty(), Unrendered::Notices);
    walk.add(missing_characters(pages), Unrendered::MissingCharacters);
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
        variants.dedup();
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
    fn a_marked_element_counts_unless_it_says_none() {
        assert!(marked("<Keep><Wah>Open</Wah></Keep>", "Wah"));
        assert!(marked("<Keep><Slashed/></Keep>", "Slashed"));
        assert!(marked(
            "<Keep><Fermatas><Fermata><Type>Long</Type></Fermata></Fermatas></Keep>",
            "Fermata"
        ));
        assert!(!marked("<Keep><Fermatas/></Keep>", "Fermata"));
        assert!(!marked("<Keep><Wah>None</Wah></Keep>", "Wah"));
        assert!(!marked("<Keep><WahWah>Open</WahWah></Keep>", "Wah"));
        assert!(!marked("", "Wah"));
    }
}
