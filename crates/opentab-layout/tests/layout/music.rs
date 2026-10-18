use opentab_layout::glyphs::Metrics;
use opentab_layout::{
    A4, ClefSign, DisplayList, Group, GroupId, LayoutOptions, Line, Micrometres, Page, PageSize,
    PlacedGlyph, Primitive, Rect, StaffKind, StaffLayout, StaffMode, SystemLayout, Units, layout,
    notehead_glyphs,
};
use opentab_model::{
    Bar, Beat, BeatEffects, BeatId, Clef, Duration, Grace, KeySignature, MasterBar, Note,
    NoteEffects, NoteId, NoteKind, NoteValue, Score, TimeSignature, Tuplet, Voice, VoiceSlot,
    blank_score,
};

use crate::support::{check, glyph_name, ink};

pub const TALL_MM: i64 = 3_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Play(Duration, Vec<(u8, u8)>),
    Rest(Duration),
    Grace(Grace, NoteValue, Vec<(u8, u8)>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Measure {
    pub time: (u8, u8),
    pub key: i8,
    pub voices: Vec<(u8, Vec<Event>)>,
}

pub fn plain(value: NoteValue) -> Duration {
    Duration::new(value, 0, None).unwrap()
}

pub fn dotted(value: NoteValue, dots: u8) -> Duration {
    Duration::new(value, dots, None).unwrap()
}

pub fn tuplet(value: NoteValue, num: u8, den: u8) -> Duration {
    Duration::new(value, 0, Some(Tuplet::new(num, den).unwrap())).unwrap()
}

pub fn play(value: NoteValue, notes: &[(u8, u8)]) -> Event {
    Event::Play(plain(value), notes.to_vec())
}

pub fn measure(time: (u8, u8), key: i8, voices: Vec<(u8, Vec<Event>)>) -> Measure {
    Measure { time, key, voices }
}

fn notes(score: &mut Score, frets: &[(u8, u8)]) -> Vec<Note> {
    frets
        .iter()
        .map(|(string, fret)| Note {
            id: score.alloc().unwrap(),
            kind: NoteKind::Fretted {
                string: *string,
                fret: *fret,
            },
            effects: NoteEffects::default(),
        })
        .collect()
}

fn beat(score: &mut Score, event: &Event) -> Beat {
    let (duration, notes, grace) = match event {
        Event::Play(duration, frets) => (*duration, notes(score, frets), None),
        Event::Rest(duration) => (*duration, Vec::new(), None),
        Event::Grace(kind, value, frets) => (plain(*value), notes(score, frets), Some(*kind)),
    };
    Beat {
        id: score.alloc().unwrap(),
        duration,
        notes,
        effects: BeatEffects {
            grace,
            ..BeatEffects::default()
        },
    }
}

pub fn build(measures: &[Measure]) -> Score {
    let mut score = blank_score().unwrap();
    let template = score.master_bars[0].clone();
    score.master_bars = measures
        .iter()
        .map(|spec| MasterBar {
            id: score.ids.next().unwrap().into(),
            time_signature: TimeSignature::new(spec.time.0, spec.time.1).unwrap(),
            key_signature: KeySignature::new(spec.key, false).unwrap(),
            tempo_changes: Vec::new(),
            ..template.clone()
        })
        .collect();
    let bars: Vec<Bar> = measures
        .iter()
        .map(|spec| {
            let voices = spec
                .voices
                .iter()
                .map(|(slot, events)| Voice {
                    id: score.alloc().unwrap(),
                    slot: VoiceSlot::new(*slot).unwrap(),
                    beats: events.iter().map(|event| beat(&mut score, event)).collect(),
                })
                .collect();
            Bar {
                id: score.alloc().unwrap(),
                clef: Clef::Treble,
                voices,
            }
        })
        .collect();
    score.tracks[0].staves[0].bars = bars;
    score
}

pub fn note_at(score: &mut Score, bar: usize, voice: usize, beat: usize, note: usize) -> &mut Note {
    &mut score.tracks[0].staves[0].bars[bar].voices[voice].beats[beat].notes[note]
}

pub fn beat_id(score: &Score, bar: usize, voice: usize, beat: usize) -> BeatId {
    score.tracks[0].staves[0].bars[bar].voices[voice].beats[beat].id
}

pub fn note_id(score: &Score, bar: usize, voice: usize, beat: usize, note: usize) -> NoteId {
    score.tracks[0].staves[0].bars[bar].voices[voice].beats[beat].notes[note].id
}

pub fn options(mode: StaffMode) -> LayoutOptions {
    LayoutOptions {
        staff_mode: Some(mode),
        ..LayoutOptions::default()
    }
}

pub fn tall(mode: Option<StaffMode>) -> LayoutOptions {
    LayoutOptions {
        staff_mode: mode,
        page: PageSize {
            width: A4.width,
            height: Micrometres::from_mm(TALL_MM),
        },
        ..LayoutOptions::default()
    }
}

pub fn render(score: &Score, options: &LayoutOptions) -> DisplayList {
    let list = layout(score, options).unwrap();
    check(score, options, &list);
    list
}

pub fn find_group(items: &[Primitive], id: GroupId) -> Option<&Group> {
    for item in items {
        if let Primitive::Group(group) = item {
            if group.id == id {
                return Some(group);
            }
            if let Some(found) = find_group(&group.items, id) {
                return Some(found);
            }
        }
    }
    None
}

pub fn group_in(list: &DisplayList, id: GroupId) -> &Group {
    list.pages
        .iter()
        .find_map(|page| find_group(&page.items, id))
        .unwrap_or_else(|| panic!("group {} is drawn", id.0))
}

pub fn glyphs(items: &[Primitive]) -> Vec<PlacedGlyph> {
    let mut out = Vec::new();
    for item in items {
        match item {
            Primitive::Glyph(glyph) => out.push(*glyph),
            Primitive::Group(group) => out.extend(glyphs(&group.items)),
            _ => {}
        }
    }
    out
}

pub fn own_lines(items: &[Primitive]) -> Vec<Line> {
    items
        .iter()
        .filter_map(|item| match item {
            Primitive::Line(line) => Some(*line),
            _ => None,
        })
        .collect()
}

pub fn named(glyphs: &[PlacedGlyph]) -> Vec<&'static str> {
    glyphs
        .iter()
        .map(|glyph| glyph_name(glyph.codepoint))
        .collect()
}

pub fn note_glyphs(list: &DisplayList, id: NoteId) -> Vec<&'static str> {
    named(&glyphs(&group_in(list, GroupId(id.0.0)).items))
}

pub fn is_notehead(glyph: &PlacedGlyph) -> bool {
    notehead_glyphs()
        .iter()
        .any(|head| head.codepoint == glyph.codepoint)
}

pub fn is_accidental(glyph: &PlacedGlyph) -> bool {
    glyph_name(glyph.codepoint).starts_with("accidental")
}

pub fn head_of(list: &DisplayList, id: NoteId) -> PlacedGlyph {
    glyphs(&group_in(list, GroupId(id.0.0)).items)
        .into_iter()
        .find(is_notehead)
        .unwrap()
}

pub fn stem_thickness() -> Units {
    Metrics::bravura().engraving_defaults().stem_thickness
}

pub fn stem_of(list: &DisplayList, id: BeatId) -> Line {
    own_lines(&group_in(list, GroupId(id.0.0)).items)
        .into_iter()
        .find(|line| line.from.x == line.to.x && line.thickness == stem_thickness())
        .unwrap()
}

pub fn points_up(stem: &Line) -> bool {
    stem.to.y < stem.from.y
}

pub struct Row<'a> {
    pub staff: &'a StaffLayout,
    pub bars: Vec<&'a Group>,
}

pub fn rows<'a>(page: &'a Page) -> Vec<(&'a SystemLayout, Vec<Row<'a>>)> {
    let mut groups = page.items.iter().filter_map(|item| match item {
        Primitive::Group(group) => Some(group),
        _ => None,
    });
    let mut out = Vec::new();
    for system in &page.systems {
        let mut rows = Vec::new();
        let mut index = 0;
        while index < system.staves.len() {
            let track = system.staves[index].track;
            let group = groups.next().expect("a group per drawn track");
            let mut items = group.items.iter();
            while index < system.staves.len() && system.staves[index].track == track {
                let staff = &system.staves[index];
                let mut bars = Vec::new();
                for _ in 0..staff.lines {
                    assert!(matches!(items.next(), Some(Primitive::Line(_))));
                }
                for _ in 0..staff.bars.len() {
                    match items.next() {
                        Some(Primitive::Group(bar)) => bars.push(bar),
                        other => panic!("expected a bar group, found {other:?}"),
                    }
                }
                rows.push(Row { staff, bars });
                index += 1;
            }
        }
        out.push((system, rows));
    }
    out
}

#[derive(Debug, Clone, Copy)]
pub struct Mark {
    pub rect: Rect,
    pub head: bool,
    pub beat: GroupId,
    pub note: GroupId,
}

pub fn marks(bar: &Group) -> Vec<Mark> {
    let mut out = Vec::new();
    for item in &bar.items {
        let Primitive::Group(beat) = item else {
            continue;
        };
        for inner in &beat.items {
            let Primitive::Group(note) = inner else {
                continue;
            };
            for glyph in glyphs(&note.items) {
                if is_notehead(&glyph) || is_accidental(&glyph) {
                    out.push(Mark {
                        rect: ink(&glyph),
                        head: is_notehead(&glyph),
                        beat: beat.id,
                        note: note.id,
                    });
                }
            }
        }
    }
    out
}

pub fn overlaps(a: Rect, b: Rect) -> bool {
    a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y
}

pub fn top_step(clef: ClefSign) -> i64 {
    match clef {
        ClefSign::Bass => 26,
        _ => 38,
    }
}

pub fn natural_midi(step: i64) -> i64 {
    let letter = step.rem_euclid(7);
    let semitones = [0, 2, 4, 5, 7, 9, 11][usize::try_from(letter).unwrap()];
    (step.div_euclid(7) + 1) * 12 + semitones
}

pub fn half_spaces(y: Units, top: Units) -> i64 {
    let offset = y - top;
    assert_eq!(offset.0 % 512, 0, "a head between staff positions");
    offset.0 / 512
}

pub fn standard_rows(list: &DisplayList) -> Vec<(&Page, &SystemLayout, Row<'_>)> {
    let mut out = Vec::new();
    for page in &list.pages {
        for (system, rows) in rows(page) {
            for row in rows {
                if row.staff.kind == StaffKind::Standard {
                    out.push((page, system, row));
                }
            }
        }
    }
    out
}
