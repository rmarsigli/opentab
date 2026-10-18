use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use opentab_layout::{
    ClefSign, DisplayList, GroupId, LayoutOptions, StaffKind, Units, layout, spell, written_key,
    written_midi,
};
use opentab_model::{NoteKind, Score};

use crate::music::{glyphs, group_in, half_spaces, is_notehead, top_step};
use crate::support::systems;

const LETTERS: &str = "CDEFGAB";
const MIN_COMPARED: usize = 318;
const INTERVAL_TRANSPOSED_ENHARMONICS: [Exception; 2] = [
    Exception {
        fixture: "tunings",
        track: 5,
        staff: 0,
        bar: 0,
        string: 5,
        fret: 1,
    },
    Exception {
        fixture: "tunings",
        track: 6,
        staff: 0,
        bar: 0,
        string: 5,
        fret: 1,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Exception {
    fixture: &'static str,
    track: usize,
    staff: usize,
    bar: usize,
    string: u8,
    fret: u8,
}

impl Exception {
    fn covers(&self, fixture: &str, key: &NoteKey) -> bool {
        self.fixture == fixture
            && (self.track, self.staff, self.bar, self.string, self.fret)
                == (key.track, key.staff, key.bar, key.string, key.fret)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct NoteKey {
    track: usize,
    staff: usize,
    bar: usize,
    string: u8,
    fret: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Written {
    letter: usize,
    alter: i64,
    octave: i64,
}

impl Written {
    fn midi(self) -> i64 {
        let semitones = [0, 2, 4, 5, 7, 9, 11][self.letter];
        (self.octave + 1) * 12 + semitones + self.alter
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixtures() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root().join("fixtures/gp"))
        .unwrap()
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().into_string().ok()?;
            name.strip_suffix(".gp").map(str::to_string)
        })
        .collect();
    names.sort();
    names
}

fn blocks<'a>(text: &'a str, open: &str, close: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(open) {
        let after = &rest[start + open.len()..];
        let Some(end) = after.find(close) else {
            break;
        };
        out.push(&after[..end]);
        rest = &after[end + close.len()..];
    }
    out
}

fn tag<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&close)? + start;
    Some(text[start..end].trim())
}

fn gp_notes(fixture: &str, score: &Score) -> BTreeMap<NoteKey, Vec<Written>> {
    let path = root().join(format!("goldens/gp/{fixture}.gp/export.musicxml"));
    let text = std::fs::read_to_string(path).unwrap();
    let mut out: BTreeMap<NoteKey, Vec<Written>> = BTreeMap::new();
    for (track, part) in blocks(&text, "<part id=", "</part>")
        .into_iter()
        .enumerate()
    {
        let model_staves = score.tracks[track].staves.len();
        let staves: usize = tag(part, "staves").map_or(1, |value| value.parse().unwrap());
        let paired = staves == 2 * model_staves;
        for (bar, content) in blocks(part, "<measure", "</measure>")
            .into_iter()
            .enumerate()
        {
            for note in blocks(content, "<note", "</note>") {
                if note.contains("<rest") {
                    continue;
                }
                let number: usize = tag(note, "staff").map_or(1, |value| value.parse().unwrap());
                let staff = if paired {
                    if number.is_multiple_of(2) {
                        continue;
                    }
                    (number - 1) / 2
                } else {
                    number - 1
                };
                let (Some(step), Some(octave), Some(string), Some(fret)) = (
                    tag(note, "step"),
                    tag(note, "octave"),
                    tag(note, "string"),
                    tag(note, "fret"),
                ) else {
                    continue;
                };
                let key = NoteKey {
                    track,
                    staff,
                    bar,
                    string: string.parse().unwrap(),
                    fret: fret.parse().unwrap(),
                };
                out.entry(key).or_default().push(Written {
                    letter: LETTERS.find(step).unwrap(),
                    alter: tag(note, "alter").map_or(0, |value| value.parse().unwrap()),
                    octave: octave.parse().unwrap(),
                });
            }
        }
    }
    out
}

fn standard_row(list: &DisplayList, track: usize, staff: usize, bar: usize) -> (Units, ClefSign) {
    systems(list)
        .into_iter()
        .flat_map(|system| &system.staves)
        .filter(|row| row.track == track && row.staff == staff && row.kind == StaffKind::Standard)
        .find_map(|row| {
            row.bars
                .iter()
                .find(|layout| layout.index == bar)
                .map(|layout| (row.top, layout.clef))
        })
        .unwrap()
}

fn drawn_notes(score: &Score, list: &DisplayList) -> BTreeMap<NoteKey, Vec<Written>> {
    let mut out: BTreeMap<NoteKey, Vec<Written>> = BTreeMap::new();
    for (track_index, track) in score.tracks.iter().enumerate() {
        for (staff_index, staff) in track.staves.iter().enumerate() {
            for (bar_index, bar) in staff.bars.iter().enumerate() {
                let key = written_key(
                    score.master_bars[bar_index].key_signature,
                    track.transpose.chromatic(),
                );
                let (top, clef) = standard_row(list, track_index, staff_index, bar_index);
                for beat in bar.voices.iter().flat_map(|voice| &voice.beats) {
                    for note in &beat.notes {
                        let NoteKind::Fretted { string, fret } = note.kind else {
                            continue;
                        };
                        let Some(midi) = written_midi(track, note) else {
                            continue;
                        };
                        let head = glyphs(&group_in(list, GroupId(note.id.0.0)).items)
                            .into_iter()
                            .find(is_notehead)
                            .unwrap();
                        let step = top_step(clef) - half_spaces(head.origin.y, top);
                        out.entry(NoteKey {
                            track: track_index,
                            staff: staff_index,
                            bar: bar_index,
                            string: string + 1,
                            fret,
                        })
                        .or_default()
                        .push(Written {
                            letter: usize::try_from(step.rem_euclid(7)).unwrap(),
                            alter: spell(midi, key).alter,
                            octave: step.div_euclid(7),
                        });
                    }
                }
            }
        }
    }
    out
}

#[test]
fn drawn_pitches_match_the_guitar_pro_export_of_every_fixture_and_staff() {
    let mut compared = 0;
    let mut second_staff = 0;
    for name in fixtures() {
        let fixture = name.as_str();
        let bytes = std::fs::read(root().join(format!("fixtures/gp/{fixture}.gp"))).unwrap();
        let score = opentab_gp::read_gp(&bytes).unwrap();
        let list = layout(&score, &LayoutOptions::default()).unwrap();
        let expected = gp_notes(fixture, &score);
        let drawn = drawn_notes(&score, &list);
        assert_eq!(
            expected.keys().collect::<Vec<_>>(),
            drawn.keys().collect::<Vec<_>>(),
            "{fixture}: notes on each staff"
        );
        for (key, ours) in &drawn {
            let theirs = &expected[key];
            assert_eq!(ours.len(), theirs.len(), "{fixture} {key:?}");
            for (mine, gp) in ours.iter().zip(theirs) {
                compared += 1;
                second_staff += usize::from(key.staff > 0);
                let excepted = INTERVAL_TRANSPOSED_ENHARMONICS
                    .iter()
                    .any(|exception| exception.covers(fixture, key));
                if excepted {
                    assert_ne!(mine, gp, "{fixture} {key:?} is no longer an exception");
                    assert_eq!(mine.midi(), gp.midi(), "{fixture} {key:?}");
                } else {
                    assert_eq!(mine, gp, "{fixture} {key:?}");
                }
            }
        }
    }
    assert!(compared >= MIN_COMPARED, "{compared}");
    assert!(second_staff > 0);
}
