use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroI64;

use opentab_layout::glyphs::{Metrics, NAMED, NOTEHEAD_BLACK, TIME_SIG_DIGITS, TUPLET_DIGITS};
use opentab_layout::{
    BarLayout, DisplayList, GroupId, LayoutOptions, Line, MARGIN, MIN_STAFF_SPACE, Micrometres,
    Page, PageSize, PlacedGlyph, Point, Primitive, Rect, STAFF_SPACE, StaffKind, StaffMode,
    StaffSpace, SystemLayout, Units, layout,
};
use opentab_model::{
    Bar, Beat, BeatEffects, Clef, Duration, Grace, InstrumentKind, KeySignature, MasterBar, Note,
    NoteEffects, NoteKind, NoteValue, Score, Staff, TimeSignature, Track, Tuplet, Voice, VoiceSlot,
    blank_score,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    Note(NoteValue, u8, Option<(u8, u8)>),
    Grace(NoteValue),
}

pub const DEFAULT_STRING: u8 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarSpec {
    pub time: (u8, u8),
    pub voices: Vec<Vec<Item>>,
}

pub fn note(value: NoteValue) -> Item {
    Item::Note(value, 0, None)
}

pub fn whole_bars(count: usize) -> Vec<BarSpec> {
    (0..count)
        .map(|_| BarSpec {
            time: (4, 4),
            voices: vec![vec![note(NoteValue::Whole)]],
        })
        .collect()
}

pub fn quarter_bars(count: usize) -> Vec<BarSpec> {
    (0..count)
        .map(|_| BarSpec {
            time: (4, 4),
            voices: vec![vec![note(NoteValue::Quarter); 4]],
        })
        .collect()
}

fn beat(score: &mut Score, item: Item) -> Beat {
    let (duration, grace) = match item {
        Item::Note(value, dots, tuplet) => {
            let tuplet = tuplet.map(|(num, den)| Tuplet::new(num, den).unwrap());
            (Duration::new(value, dots, tuplet).unwrap(), None)
        }
        Item::Grace(value) => (
            Duration::new(value, 0, None).unwrap(),
            Some(Grace::BeforeBeat),
        ),
    };
    let notes = vec![Note {
        id: score.alloc().unwrap(),
        kind: NoteKind::Fretted {
            string: DEFAULT_STRING,
            fret: 0,
        },
        effects: NoteEffects::default(),
    }];
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

pub fn bar(score: &mut Score, spec: &BarSpec) -> Bar {
    let voices = spec
        .voices
        .iter()
        .enumerate()
        .map(|(slot, items)| Voice {
            id: score.alloc().unwrap(),
            slot: VoiceSlot::new(u8::try_from(slot).unwrap()).unwrap(),
            beats: items.iter().map(|item| beat(score, *item)).collect(),
        })
        .collect();
    Bar {
        id: score.alloc().unwrap(),
        clef: Clef::Treble,
        voices,
    }
}

pub fn guitar(specs: &[BarSpec]) -> Score {
    let mut score = blank_score().unwrap();
    let template = score.master_bars[0].clone();
    score.master_bars = specs
        .iter()
        .map(|spec| MasterBar {
            id: score.ids.next().unwrap().into(),
            time_signature: TimeSignature::new(spec.time.0, spec.time.1).unwrap(),
            key_signature: KeySignature::default(),
            tempo_changes: Vec::new(),
            ..template.clone()
        })
        .collect();
    let bars = specs.iter().map(|spec| bar(&mut score, spec)).collect();
    score.tracks[0].staves[0].bars = bars;
    score
}

pub fn add_track(score: &mut Score, name: &str, strings: usize, kind: InstrumentKind) -> usize {
    let mut track: Track = score.tracks[0].clone();
    track.id = score.alloc().unwrap();
    track.name = name.to_string();
    track.short_name = name.chars().take(3).collect();
    track.instrument.kind = kind;
    track.tuning.strings = (0..strings)
        .map(|index| opentab_model::Pitch(u8::try_from(40 + index).unwrap()))
        .collect();
    let empty: Vec<Bar> = (0..score.master_bars.len())
        .map(|_| Bar {
            id: score.alloc().unwrap(),
            clef: Clef::Treble,
            voices: Vec::new(),
        })
        .collect();
    track.staves = vec![Staff {
        id: score.alloc().unwrap(),
        bars: empty,
    }];
    score.tracks.push(track);
    score.tracks.len() - 1
}

pub fn systems(list: &DisplayList) -> Vec<&SystemLayout> {
    list.pages.iter().flat_map(|page| &page.systems).collect()
}

pub fn flatten(items: &[Primitive]) -> Vec<&Primitive> {
    let mut out = Vec::new();
    for item in items {
        match item {
            Primitive::Group(group) => out.extend(flatten(&group.items)),
            other => out.push(other),
        }
    }
    out
}

pub fn lines(page: &Page) -> Vec<Line> {
    flatten(&page.items)
        .into_iter()
        .filter_map(|item| match item {
            Primitive::Line(line) => Some(*line),
            _ => None,
        })
        .collect()
}

pub fn texts(page: &Page) -> Vec<String> {
    flatten(&page.items)
        .into_iter()
        .filter_map(|item| match item {
            Primitive::Text(run) => Some(run.text.clone()),
            _ => None,
        })
        .collect()
}

pub fn system_texts(page: &Page) -> Vec<String> {
    page.items
        .iter()
        .filter_map(|item| match item {
            Primitive::Text(run) => Some(run.text.clone()),
            _ => None,
        })
        .collect()
}

pub fn content(page: &Page) -> Rect {
    let margin = page.units(MARGIN);
    Rect::new(
        Point {
            x: margin,
            y: margin,
        },
        Point {
            x: page.width() - margin,
            y: page.height() - margin,
        },
    )
}

fn inside(outer: Rect, inner: Rect) -> bool {
    outer.min.x <= inner.min.x
        && outer.min.y <= inner.min.y
        && inner.max.x <= outer.max.x
        && inner.max.y <= outer.max.y
}

pub fn expected_staves(score: &Score, options: &LayoutOptions) -> Vec<(usize, usize, StaffKind)> {
    let tracks: BTreeSet<usize> = match &options.tracks {
        Some(chosen) => chosen.iter().copied().collect(),
        None => (0..score.tracks.len()).collect(),
    };
    let mut keys = Vec::new();
    for index in tracks {
        let track = &score.tracks[index];
        let strings = track.tuning.strings.len();
        let tab_possible = track.instrument.kind == InstrumentKind::Fretted && strings > 0;
        let kinds = match (tab_possible, options.staff_mode) {
            (false, _) | (true, Some(StaffMode::Standard)) => vec![StaffKind::Standard],
            (true, Some(StaffMode::Tab)) => vec![StaffKind::Tab],
            (true, Some(StaffMode::Both)) => vec![StaffKind::Standard, StaffKind::Tab],
            (true, None) if (4..=8).contains(&strings) => {
                vec![StaffKind::Standard, StaffKind::Tab]
            }
            (true, None) => vec![StaffKind::Standard],
        };
        for staff in 0..track.staves.len() {
            for kind in &kinds {
                keys.push((index, staff, *kind));
            }
        }
    }
    keys
}

pub fn check(score: &Score, options: &LayoutOptions, list: &DisplayList) {
    assert!(!list.pages.is_empty());
    let all = systems(list);
    let count = all.len();
    let first_page = &list.pages[0];
    for page in &list.pages {
        assert!(!page.systems.is_empty(), "a page without systems");
        assert_eq!(page.staff_space, first_page.staff_space);
        assert_eq!(page.size, options.page);
        let area = content(page);
        let mut previous_bottom = area.min.y;
        for system in &page.systems {
            assert!(
                inside(area, system.rect),
                "{:?} outside {area:?}",
                system.rect
            );
            assert!(system.rect.min.y >= previous_bottom, "systems overlap");
            previous_bottom = system.rect.max.y;
        }
    }
    let keys = expected_staves(score, options);
    let (first_bar, last_bar) = options
        .bars
        .map_or((0, score.master_bars.len() - 1), |range| {
            (range.first, range.last)
        });
    let mut per_staff: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut owners: BTreeMap<GroupId, (usize, usize, usize)> = BTreeMap::new();
    assert_eq!(list.staff_space, first_page.staff_space);
    for (position, system) in all.iter().enumerate() {
        assert_eq!(system.index, position);
        let found: Vec<(usize, usize, StaffKind)> = system
            .staves
            .iter()
            .map(|staff| (staff.track, staff.staff, staff.kind))
            .collect();
        assert_eq!(found, keys, "staves of system {position}");
        let reference = &system.staves[0].bars;
        assert!(!reference.is_empty());
        let natural = reference[0].start_width
            + reference[1..]
                .iter()
                .fold(Units::ZERO, |total, bar| total + bar.inline_width);
        assert_eq!(
            system.natural, natural,
            "natural width of system {position}"
        );
        if reference.len() >= 2 {
            assert!(natural <= system.available, "system {position} overflows");
        }
        let last = position + 1 == count;
        if last {
            let relaxed = Units(natural.0 * 3 / 2).min(system.available);
            assert_eq!(system.rect.width(), relaxed, "last system width");
        } else {
            assert_eq!(
                system.rect.width(),
                system.available,
                "system {position} justified"
            );
            let next = &all[position + 1].staves[0].bars[0];
            assert!(
                natural + next.inline_width > system.available,
                "system {position} breaks early"
            );
        }
        let page = list
            .pages
            .iter()
            .find(|page| page.systems.iter().any(|candidate| candidate == *system))
            .unwrap();
        let drawn = lines(page);
        for (row, staff) in system.staves.iter().enumerate() {
            per_staff
                .entry(row)
                .or_default()
                .extend(staff.bars.iter().map(|bar| bar.index));
            let (top, bottom) = staff.span();
            for line in 0..staff.lines {
                let y = staff.line_y(line);
                assert!(
                    drawn.iter().any(|drawn_line| drawn_line.from
                        == Point {
                            x: system.rect.min.x,
                            y
                        }
                        && drawn_line.to
                            == Point {
                                x: system.rect.max.x,
                                y
                            }),
                    "staff line {line} of row {row}"
                );
            }
            assert_eq!(staff.bars.len(), reference.len());
            assert_eq!(staff.bars[0].rect.min.x, system.rect.min.x);
            assert_eq!(
                staff.bars[staff.bars.len() - 1].rect.max.x,
                system.rect.max.x
            );
            for (bar, aligned) in staff.bars.iter().zip(reference) {
                assert_eq!(bar.rect.min.x, aligned.rect.min.x);
                assert_eq!(bar.rect.max.x, aligned.rect.max.x);
                assert_eq!((bar.rect.min.y, bar.rect.max.y), (top, bottom));
                let number = if score.anacrusis {
                    bar.index
                } else {
                    bar.index + 1
                };
                assert_eq!(bar.number, number);
                assert!(bar.content >= bar.rect.min.x);
                let mut previous = bar.content;
                for column in &bar.columns {
                    assert!(column.x > previous, "columns of bar {}", bar.index);
                    assert!(column.x < bar.rect.max.x);
                    previous = column.x;
                }
                let crowded = list.crowded.contains(&bar.index);
                if !crowded {
                    for pair in bar.columns.windows(2) {
                        assert!(
                            pair[1].x - pair[0].x >= notehead(),
                            "columns of bar {} overlap",
                            bar.index
                        );
                    }
                }
                let owner = (staff.track, staff.staff, bar.index);
                assert_eq!(
                    *owners.entry(bar.group).or_insert(owner),
                    owner,
                    "group id {} is shared",
                    bar.group.0
                );
                for glyph in bar_glyphs(page, bar.group) {
                    let ink = ink(&glyph);
                    assert!(
                        ink.min.x >= bar.rect.min.x && ink.max.x <= bar.rect.max.x,
                        "glyph U+{:04X} outside bar {}",
                        u32::from(glyph.codepoint),
                        bar.index
                    );
                    assert!(inside(content(page), ink), "glyph outside the content area");
                }
                for slot in &bar.beats {
                    let x = bar.x_of(slot).unwrap();
                    assert!(x < bar.rect.max.x, "beat x of bar {}", bar.index);
                    assert!(x >= bar.content, "grace before content");
                    let column = &bar.columns[slot.column];
                    match slot.grace {
                        Some(grace) => {
                            assert!(grace.order < grace.count && grace.count <= column.graces);
                            assert!(x < column.x);
                        }
                        None => assert_eq!(x, column.x),
                    }
                }
                let barline = drawn.iter().any(|line| {
                    line.from.x == line.to.x
                        && line.from.y == top
                        && line.to.y == bottom
                        && (line.from.x + line.thickness.half() == bar.rect.max.x
                            || (line.thickness == Units(512)
                                && line.from.x - line.thickness.half() == bar.rect.max.x))
                });
                assert!(barline, "barline of bar {} in row {row}", bar.index);
            }
            for pair in staff.bars.windows(2) {
                assert_eq!(pair[0].rect.max.x, pair[1].rect.min.x);
                assert!(pair[0].rect.min.x < pair[0].rect.max.x);
            }
        }
    }
    let expected: Vec<usize> = (first_bar..=last_bar).collect();
    assert_eq!(per_staff.len(), keys.len());
    for (row, indices) in per_staff {
        assert_eq!(indices, expected, "bars of row {row}");
    }
}

fn point(point: Point) -> String {
    format!("{},{}", point.x.0, point.y.0)
}

fn dump_items(items: &[Primitive], depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth);
    for item in items {
        let line = match item {
            Primitive::Glyph(glyph) => format!(
                "glyph U+{:04X} {} size {}",
                u32::from(glyph.codepoint),
                point(glyph.origin),
                glyph.staff_space.0
            ),
            Primitive::Line(line) => format!(
                "line {} {} thick {}{}",
                point(line.from),
                point(line.to),
                line.thickness.0,
                line.dash.map_or(String::new(), |dash| format!(
                    " dash {} {}",
                    dash.on.0, dash.off.0
                ))
            ),
            Primitive::Shape(shape) => format!("shape {:?} {:?}", shape.style, shape.segments),
            Primitive::Text(run) => format!(
                "text {:?} {} em {} {:?} {:?}",
                run.text,
                point(run.origin),
                run.em.0,
                run.anchor,
                run.font
            ),
            Primitive::FilledRect(rect) => format!(
                "rect {} {} {:?}",
                point(rect.rect.min),
                point(rect.rect.max),
                rect.colour
            ),
            Primitive::Group(group) => {
                out.push_str(&format!("{indent}group {}\n", group.id.0));
                dump_items(&group.items, depth + 1, out);
                continue;
            }
        };
        out.push_str(&format!("{indent}{line}\n"));
    }
}

pub fn dump(list: &DisplayList) -> String {
    let mut out = String::new();
    for (number, page) in list.pages.iter().enumerate() {
        out.push_str(&format!(
            "page {number} staff space {} um\n",
            page.staff_space.micrometres().0
        ));
        for system in &page.systems {
            out.push_str(&format!(
                "system {} {} {} natural {} available {}\n",
                system.index,
                point(system.rect.min),
                point(system.rect.max),
                system.natural.0,
                system.available.0
            ));
            for staff in &system.staves {
                out.push_str(&format!(
                    "  staff track {} staff {} {:?} top {} lines {} spacing {}\n",
                    staff.track,
                    staff.staff,
                    staff.kind,
                    staff.top.0,
                    staff.lines,
                    staff.line_spacing.0
                ));
                for bar in &staff.bars {
                    let columns: Vec<String> = bar
                        .columns
                        .iter()
                        .map(|column| format!("{}@{}", column.ticks, column.x.0))
                        .collect();
                    out.push_str(&format!(
                        "    bar {} number {} group {} {} {} content {} widths {} {} {:?} ticks {}/{} [{}]\n",
                        bar.index,
                        bar.number,
                        bar.group.0,
                        point(bar.rect.min),
                        point(bar.rect.max),
                        bar.content.0,
                        bar.start_width.0,
                        bar.inline_width.0,
                        bar.clef,
                        bar.span_ticks,
                        bar.ticks_per_whole,
                        columns.join(" ")
                    ));
                }
            }
        }
        dump_items(&page.items, 0, &mut out);
    }
    out
}

pub fn notehead() -> Units {
    Metrics::bravura().bbox(NOTEHEAD_BLACK.name).unwrap().max.x
}

pub fn glyph_name(codepoint: char) -> &'static str {
    NAMED
        .iter()
        .chain(TIME_SIG_DIGITS.iter())
        .chain(TUPLET_DIGITS.iter())
        .find(|glyph| glyph.codepoint == codepoint)
        .map(|glyph| glyph.name)
        .unwrap_or_else(|| panic!("unexpected glyph U+{:04X}", u32::from(codepoint)))
}

pub fn ink(glyph: &PlacedGlyph) -> Rect {
    let bbox = Metrics::bravura()
        .bbox(glyph_name(glyph.codepoint))
        .unwrap();
    let divisor = NonZeroI64::new(STAFF_SPACE.0).unwrap();
    let scale = |value: Units| value.scaled(glyph.staff_space.0, divisor);
    Rect::new(
        Point {
            x: glyph.origin.x + scale(bbox.min.x),
            y: glyph.origin.y + scale(bbox.min.y),
        },
        Point {
            x: glyph.origin.x + scale(bbox.max.x),
            y: glyph.origin.y + scale(bbox.max.y),
        },
    )
}

fn groups_with<'a>(items: &'a [Primitive], id: GroupId, out: &mut Vec<&'a [Primitive]>) {
    for item in items {
        if let Primitive::Group(group) = item {
            if group.id == id {
                out.push(&group.items);
            }
            groups_with(&group.items, id, out);
        }
    }
}

pub fn bar_glyphs(page: &Page, id: GroupId) -> Vec<PlacedGlyph> {
    let mut found = Vec::new();
    groups_with(&page.items, id, &mut found);
    found
        .into_iter()
        .flat_map(|items| flatten(items))
        .filter_map(|item| match item {
            Primitive::Glyph(glyph) => Some(*glyph),
            _ => None,
        })
        .collect()
}

pub fn bar_of(list: &DisplayList, index: usize) -> &BarLayout {
    systems(list)
        .into_iter()
        .flat_map(|system| &system.staves[0].bars)
        .find(|bar| bar.index == index)
        .unwrap()
}

pub const TALL_PAGE_MM: i64 = 20_000;

pub fn assert_too_tall(score: &Score, options: &LayoutOptions) {
    let tall = LayoutOptions {
        page: PageSize {
            width: options.page.width,
            height: Micrometres::from_mm(TALL_PAGE_MM),
        },
        staff_space: options.staff_space.min(MIN_STAFF_SPACE),
        ..options.clone()
    };
    let list = layout(score, &tall).expect("a page tall enough holds every system");
    let tallest = systems(&list)
        .iter()
        .map(|system| system.rect.height())
        .max()
        .unwrap();
    let floor = StaffSpace::new(options.staff_space.min(MIN_STAFF_SPACE)).unwrap();
    let room = floor.units(options.page.height) - floor.units(MARGIN) - floor.units(MARGIN);
    assert!(
        tallest > room,
        "PageTooSmall although the tallest system ({}) fits the page ({})",
        tallest.0,
        room.0
    );
}
