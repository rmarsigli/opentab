use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use opentab_gp::read_gp;
use opentab_model::{
    ElementId, IdAllocator, Note, OpaquePayload, Score, Tempo, TempoChange, TempoUnit, Track,
    to_json,
};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const RESAVE: &str = "resave.gpif";
const LATER_STAFF_TUNING: &str = "Properties/Property@Tuning";
const RESPELLED_PITCH: &str = "pitch:Properties/Property@TransposedPitch";
const RESPELLED_PITCHES: usize = 3;
const LATER_STAFF_TUNINGS: usize = 1;
const GHOST_ACCENT: &str = "ghost-accent.gp";
const GHOSTS_DROPPED_WITH_ACCENTS: usize = 3;
const GHOST_NORMALISATION: &str = "gp_drops_ghost_on_accented_notes";
const SEMITONES: i32 = 12;
const DEFAULT_BPM: f64 = 120.0;
const REPLACEMENT: char = '\u{FFFD}';
const SHOWN_DIFF_LINES: usize = 40;

const GP_DEFAULT_PAYLOADS: [&str; 57] = [
    "/AudioEngineState",
    "/Automations",
    "/Color",
    "/ConcertPitchStemOrientation",
    "/Encoding",
    "/ForcedSound",
    "/IconId",
    "/Lyrics",
    "/MidiConnection",
    "/PalmMute",
    "/PlaybackState",
    "/PlayingStyle",
    "/RSE",
    "/ScoreViews",
    "/SystemsDefautLayout",
    "/SystemsLayout",
    "/TransposedPitchStemOrientation",
    "note-kind:/InstrumentArticulation",
    "tempo-position:*Tempo/Linear",
    "tempo-position:*Tempo/Visible",
    "InstrumentSet/Elements",
    "InstrumentSet/LineCount",
    "InstrumentSet/Name",
    "Key/TransposeAs",
    "MasterBars/comment",
    "MasterTrack/RSE",
    "Notes/text",
    "Properties/Name",
    "Properties/Property@ChordCollection",
    "Properties/Property@ChordWorkingSet",
    "Properties/Property@DiagramCollection",
    "Properties/Property@DiagramWorkingSet",
    "Properties/Property@FretCount",
    "Properties/Property@PartialCapoFret",
    "Properties/Property@PartialCapoStringFlags",
    "Properties/Property@PrimaryPickupTone",
    "Properties/Property@PrimaryPickupVolume",
    "Properties/Property@Tuning/Instrument",
    "Properties/Property@Tuning/Label",
    "Properties/Property@Tuning/LabelVisible",
    "Score/FirstPageFooter",
    "Score/FirstPageHeader",
    "Score/MultiVoice",
    "Score/PageFooter",
    "Score/PageHeader",
    "Score/ScoreSystemsDefaultLayout",
    "Score/ScoreSystemsLayout",
    "Score/ScoreZoom",
    "Score/ScoreZoomPolicy",
    "Score/WordsAndMusic",
    "Sounds/Sound/Label",
    "Sounds/Sound/Name",
    "Sounds/Sound/Path",
    "Sounds/Sound/RSE",
    "Sounds/Sound/Role",
    "Sounds/Sound/MIDI/LSB",
    "Sounds/Sound/MIDI/MSB",
];

struct Pair {
    name: String,
    fixture: Score,
    golden: Score,
}

type Normalise = fn(&mut Score, &mut Score);

struct Normalisation {
    name: &'static str,
    apply: Normalise,
}

const NORMALISATIONS: [Normalisation; 9] = [
    Normalisation {
        name: "gp_writes_its_own_defaults",
        apply: gp_writes_its_own_defaults,
    },
    Normalisation {
        name: "gp_writes_the_track_tuning_on_later_staves",
        apply: gp_writes_the_track_tuning_on_later_staves,
    },
    Normalisation {
        name: "gp_spells_transposed_pitch_its_own_way",
        apply: gp_spells_transposed_pitch_its_own_way,
    },
    Normalisation {
        name: "gp_adds_a_default_tempo",
        apply: gp_adds_a_default_tempo,
    },
    Normalisation {
        name: "gp_replaces_characters_outside_the_bmp",
        apply: gp_replaces_characters_outside_the_bmp,
    },
    Normalisation {
        name: "gp_drops_empty_voices",
        apply: gp_drops_empty_voices,
    },
    Normalisation {
        name: "gp_drops_let_ring_on_staccato_notes",
        apply: gp_drops_let_ring_on_staccato_notes,
    },
    Normalisation {
        name: GHOST_NORMALISATION,
        apply: gp_drops_ghost_on_accented_notes,
    },
    Normalisation {
        name: "element_ids_follow_reading_order",
        apply: element_ids_follow_reading_order,
    },
];

fn retain_payloads(golden: &mut Score, mut keep: impl FnMut(ElementId, &OpaquePayload) -> bool) {
    for (id, payloads) in &mut golden.extensions.0 {
        payloads.retain(|payload| keep(*id, payload));
    }
    golden
        .extensions
        .0
        .retain(|_, payloads| !payloads.is_empty());
}

fn gp_writes_its_own_defaults(_fixture: &mut Score, golden: &mut Score) {
    let allowed: BTreeSet<&str> = GP_DEFAULT_PAYLOADS.into_iter().collect();
    retain_payloads(golden, |_, payload| {
        !allowed.contains(payload_kind(&payload.data).as_str())
    });
    golden
        .document_extensions
        .retain(|payload| !allowed.contains(payload_kind(&payload.data).as_str()));
}

fn later_staves(score: &Score) -> BTreeMap<ElementId, &Track> {
    score
        .tracks
        .iter()
        .flat_map(|track| {
            track
                .staves
                .iter()
                .skip(1)
                .map(move |staff| (staff.id.0, track))
        })
        .collect()
}

fn element_text<'d>(data: &'d str, name: &str) -> Option<&'d str> {
    let open = format!("<{name}>");
    let start = data.find(&open)? + open.len();
    let rest = &data[start..];
    rest.find(&format!("</{name}>")).map(|end| &rest[..end])
}

fn lowest_first(track: &Track) -> String {
    track
        .tuning
        .strings
        .iter()
        .rev()
        .map(|pitch| pitch.0.to_string())
        .collect::<Vec<String>>()
        .join(" ")
}

fn later_staff_tunings(golden: &Score) -> Vec<(ElementId, String, String)> {
    let staves = later_staves(golden);
    let mut found = Vec::new();
    for (id, payloads) in &golden.extensions.0 {
        for payload in payloads {
            if payload_kind(&payload.data) != LATER_STAFF_TUNING {
                continue;
            }
            let track = staves.get(id).unwrap_or_else(|| {
                panic!("a Tuning payload on {id:?}, which is not a later staff")
            });
            let pitches = element_text(&payload.data, "Pitches").unwrap_or_default();
            found.push((*id, pitches.to_string(), lowest_first(track)));
        }
    }
    found
}

fn gp_writes_the_track_tuning_on_later_staves(_fixture: &mut Score, golden: &mut Score) {
    for (id, pitches, track) in later_staff_tunings(golden) {
        assert_eq!(pitches, track, "staff {id:?} has its own tuning after GP");
    }
    retain_payloads(golden, |_, payload| {
        payload_kind(&payload.data) != LATER_STAFF_TUNING
    });
}

fn notes_with_tracks(score: &Score) -> BTreeMap<ElementId, (&Track, &Note)> {
    let mut found = BTreeMap::new();
    for track in &score.tracks {
        for staff in &track.staves {
            for bar in &staff.bars {
                for voice in &bar.voices {
                    for beat in &voice.beats {
                        for note in &beat.notes {
                            found.insert(note.id.0, (track, note));
                        }
                    }
                }
            }
        }
    }
    found
}

fn spelled_midi(data: &str) -> Option<i32> {
    let step = match element_text(data, "Step")? {
        "C" => 0,
        "D" => 2,
        "E" => 4,
        "F" => 5,
        "G" => 7,
        "A" => 9,
        "B" => 11,
        _ => return None,
    };
    let accidental: i32 = element_text(data, "Accidental")
        .unwrap_or_default()
        .chars()
        .map(|sign| match sign {
            '#' => 1,
            'b' => -1,
            _ => 0,
        })
        .sum();
    let octave: i32 = element_text(data, "Octave")?.parse().ok()?;
    Some(octave * SEMITONES + step + accidental)
}

fn written_midi(track: &Track, note: &Note) -> Option<i32> {
    let sounding = i32::from(track.pitch_of(note)?.0);
    let transpose = track.transpose;
    Some(sounding - i32::from(transpose.chromatic()) - SEMITONES * i32::from(transpose.octave()))
}

fn respelled_pitches(golden: &Score) -> Vec<(ElementId, Option<i32>, Option<i32>)> {
    let notes = notes_with_tracks(golden);
    let mut found = Vec::new();
    for (id, payloads) in &golden.extensions.0 {
        for payload in payloads {
            if payload_kind(&payload.data) != RESPELLED_PITCH {
                continue;
            }
            let written = notes
                .get(id)
                .and_then(|(track, note)| written_midi(track, note));
            found.push((*id, spelled_midi(&payload.data), written));
        }
    }
    found
}

fn gp_spells_transposed_pitch_its_own_way(_fixture: &mut Score, golden: &mut Score) {
    for (id, spelled, written) in respelled_pitches(golden) {
        assert!(
            spelled.is_some() && spelled == written,
            "note {id:?}: GP's TransposedPitch {spelled:?} is not the written pitch {written:?}"
        );
    }
    retain_payloads(golden, |_, payload| {
        payload_kind(&payload.data) != RESPELLED_PITCH
    });
}

fn is_default_tempo(change: &TempoChange) -> bool {
    *change.position.numer() == 0
        && change.unit == TempoUnit::Quarter
        && change.bpm.bpm() == DEFAULT_BPM
}

fn gp_adds_a_default_tempo(fixture: &mut Score, golden: &mut Score) {
    let Some(first) = fixture.master_bars.first() else {
        return;
    };
    if first
        .tempo_changes
        .iter()
        .any(|change| *change.position.numer() == 0)
    {
        return;
    }
    if let Some(bar) = golden.master_bars.first_mut()
        && bar.tempo_changes.first().is_some_and(is_default_tempo)
    {
        bar.tempo_changes.remove(0);
    }
}

fn outside_bmp_replaced(text: &mut String) {
    *text = text
        .chars()
        .map(|character| {
            if u32::from(character) > 0xFFFF {
                REPLACEMENT
            } else {
                character
            }
        })
        .collect();
}

fn gp_replaces_characters_outside_the_bmp(fixture: &mut Score, _golden: &mut Score) {
    let meta = &mut fixture.meta;
    for text in [
        &mut meta.title,
        &mut meta.subtitle,
        &mut meta.artist,
        &mut meta.album,
        &mut meta.words,
        &mut meta.music,
        &mut meta.copyright,
        &mut meta.tab_author,
        &mut meta.instructions,
        &mut meta.notices,
    ] {
        outside_bmp_replaced(text);
    }
    for master_bar in &mut fixture.master_bars {
        if let Some(marker) = &mut master_bar.marker {
            outside_bmp_replaced(&mut marker.text);
            outside_bmp_replaced(&mut marker.letter);
        }
    }
    for track in &mut fixture.tracks {
        outside_bmp_replaced(&mut track.name);
        outside_bmp_replaced(&mut track.short_name);
        for staff in &mut track.staves {
            for bar in &mut staff.bars {
                for voice in &mut bar.voices {
                    for beat in &mut voice.beats {
                        if let Some(text) = &mut beat.effects.text {
                            outside_bmp_replaced(text);
                        }
                        if let Some(name) = &mut beat.effects.chord_name {
                            outside_bmp_replaced(name);
                        }
                    }
                }
            }
        }
    }
}

fn gp_drops_empty_voices(fixture: &mut Score, _golden: &mut Score) {
    for track in &mut fixture.tracks {
        for staff in &mut track.staves {
            for bar in &mut staff.bars {
                bar.voices.retain(|voice| !voice.beats.is_empty());
            }
        }
    }
}

fn gp_drops_let_ring_on_staccato_notes(fixture: &mut Score, _golden: &mut Score) {
    for track in &mut fixture.tracks {
        for staff in &mut track.staves {
            for bar in &mut staff.bars {
                for voice in &mut bar.voices {
                    for beat in &mut voice.beats {
                        for note in &mut beat.notes {
                            if note.effects.staccato {
                                note.effects.let_ring = false;
                            }
                        }
                    }
                }
            }
        }
    }
}

fn notes_mut(score: &mut Score) -> impl Iterator<Item = &mut Note> {
    score
        .tracks
        .iter_mut()
        .flat_map(|track| track.staves.iter_mut())
        .flat_map(|staff| staff.bars.iter_mut())
        .flat_map(|bar| bar.voices.iter_mut())
        .flat_map(|voice| voice.beats.iter_mut())
        .flat_map(|beat| beat.notes.iter_mut())
}

fn notes_of(score: &Score) -> Vec<&Note> {
    score
        .tracks
        .iter()
        .flat_map(|track| track.staves.iter())
        .flat_map(|staff| staff.bars.iter())
        .flat_map(|bar| bar.voices.iter())
        .flat_map(|voice| voice.beats.iter())
        .flat_map(|beat| beat.notes.iter())
        .collect()
}

fn accented_ghost(note: &Note) -> bool {
    note.effects.ghost && (note.effects.accent.is_some() || note.effects.staccato)
}

fn gp_drops_ghost_on_accented_notes(fixture: &mut Score, _golden: &mut Score) {
    for note in notes_mut(fixture) {
        if accented_ghost(note) {
            note.effects.ghost = false;
        }
    }
}

fn renumbered(score: &mut Score) {
    let mut ids = IdAllocator::default();
    let mut next = || ids.next().unwrap_or(ElementId(0));
    for master_bar in &mut score.master_bars {
        master_bar.id = next().into();
    }
    for track in &mut score.tracks {
        track.id = next().into();
        for staff in &mut track.staves {
            staff.id = next().into();
            for bar in &mut staff.bars {
                bar.id = next().into();
                for voice in &mut bar.voices {
                    voice.id = next().into();
                    for beat in &mut voice.beats {
                        beat.id = next().into();
                        for note in &mut beat.notes {
                            note.id = next().into();
                        }
                    }
                }
            }
        }
    }
    score.ids = ids;
}

fn element_ids_follow_reading_order(fixture: &mut Score, golden: &mut Score) {
    renumbered(fixture);
    renumbered(golden);
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn fixture_names() -> Vec<String> {
    let dir = root().join("fixtures").join("gp");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("fixtures/gp is readable")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.ends_with(".gp"))
        .collect();
    names.sort();
    names
}

fn read(path: &Path) -> Score {
    let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    read_gp(&bytes).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn minimal_container(gpif: &[u8]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, data) in [
        ("Content/PartConfiguration", &b""[..]),
        ("Content/score.gpif", gpif),
        ("VERSION", b"7.0"),
    ] {
        writer.start_file(name, options).expect("entry");
        writer.write_all(data).expect("data");
    }
    writer.finish().expect("archive").into_inner()
}

fn read_resave(path: &Path) -> Score {
    let gpif = std::fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let score = read_gp(&minimal_container(&gpif))
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    assert!(score.container.is_empty(), "{}", path.display());
    score
}

fn pairs() -> Vec<Pair> {
    fixture_names()
        .into_iter()
        .map(|name| Pair {
            fixture: read(&root().join("fixtures").join("gp").join(&name)),
            golden: read_resave(&root().join("goldens").join("gp").join(&name).join(RESAVE)),
            name,
        })
        .collect()
}

fn normalised(pair: &Pair, skip: Option<&str>) -> (Score, Score) {
    let mut fixture = pair.fixture.clone();
    let mut golden = pair.golden.clone();
    for normalisation in &NORMALISATIONS {
        if Some(normalisation.name) != skip {
            (normalisation.apply)(&mut fixture, &mut golden);
        }
    }
    (fixture, golden)
}

fn json(score: &Score) -> String {
    to_json(score).expect("json")
}

fn difference(name: &str, fixture: &Score, golden: &Score) -> String {
    let ours = json(fixture);
    let theirs = json(golden);
    let shown: Vec<String> = ours
        .lines()
        .zip(theirs.lines())
        .enumerate()
        .filter(|(_, (left, right))| left != right)
        .take(SHOWN_DIFF_LINES)
        .map(|(line, (left, right))| format!("line {line}: fixture {left} | golden {right}"))
        .collect();
    format!(
        "{name}: fixture and golden differ ({} vs {} JSON lines)\n{}",
        ours.lines().count(),
        theirs.lines().count(),
        shown.join("\n")
    )
}

fn attribute<'d>(head: &'d str, name: &str) -> Option<&'d str> {
    let marker = format!(" {name}=\"");
    let start = head.find(&marker)? + marker.len();
    let rest = &head[start..];
    rest.find('"').map(|end| &rest[..end])
}

fn ordinal_free(parent: &str) -> String {
    parent
        .split('/')
        .map(|step| step.split('#').next().unwrap_or(step))
        .collect::<Vec<&str>>()
        .join("/")
}

fn payload_kind(data: &str) -> String {
    let head_end = data.find('>').unwrap_or(data.len());
    let head = &data[..head_end];
    let parent = ordinal_free(attribute(head, "parent").unwrap_or_default());
    let guard = attribute(head, "guard")
        .and_then(|guard| guard.split(':').next())
        .map(|kind| format!("{kind}:"))
        .unwrap_or_default();
    let body = data.get(head_end + 1..).unwrap_or_default();
    let element = if body.starts_with("<!--") {
        "comment".to_string()
    } else if let Some(tag) = body.strip_prefix('<') {
        let name: String = tag
            .chars()
            .take_while(|character| !matches!(character, ' ' | '/' | '>'))
            .collect();
        let tag_end = tag.find('>').unwrap_or(tag.len());
        let key = match name.as_str() {
            "Property" => attribute(&tag[..tag_end], "name").map(|value| format!("@{value}")),
            "XProperty" => attribute(&tag[..tag_end], "id").map(|value| format!("@{value}")),
            _ => None,
        };
        format!("{name}{}", key.unwrap_or_default())
    } else {
        "text".to_string()
    };
    format!("{guard}{parent}/{element}")
}

fn golden_payload_kinds(golden: &Score) -> BTreeSet<String> {
    golden
        .extensions
        .0
        .values()
        .flatten()
        .chain(golden.document_extensions.iter())
        .map(|payload| payload_kind(&payload.data))
        .collect()
}

#[test]
fn every_fixture_has_a_golden_and_reads_without_payloads() {
    let names = fixture_names();
    assert!(names.len() >= 11, "{names:?}");
    let golden_dirs: BTreeSet<String> = std::fs::read_dir(root().join("goldens").join("gp"))
        .expect("goldens/gp is readable")
        .map(|entry| entry.expect("entry"))
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        golden_dirs,
        names.iter().cloned().collect::<BTreeSet<String>>()
    );
    for pair in pairs() {
        assert!(pair.fixture.container.is_empty(), "{}", pair.name);
        assert!(pair.fixture.extensions.0.is_empty(), "{}", pair.name);
        assert!(pair.fixture.document_extensions.is_empty(), "{}", pair.name);
    }
}

#[test]
fn guitar_pro_keeps_every_fixture_modulo_its_documented_normalisations() {
    let mut failures = Vec::new();
    for pair in pairs() {
        let (fixture, golden) = normalised(&pair, None);
        if fixture != golden {
            failures.push(difference(&pair.name, &fixture, &golden));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn guitar_pro_adds_only_documented_default_payloads() {
    let allowed: BTreeSet<&str> = GP_DEFAULT_PAYLOADS.into_iter().collect();
    let mut seen = BTreeSet::new();
    for pair in pairs() {
        for kind in golden_payload_kinds(&pair.golden) {
            if kind == LATER_STAFF_TUNING || kind == RESPELLED_PITCH {
                continue;
            }
            assert!(
                allowed.contains(kind.as_str()),
                "{}: undocumented payload kind {kind}",
                pair.name
            );
            seen.insert(kind);
        }
    }
    let unused: Vec<&&str> = allowed
        .iter()
        .filter(|kind| !seen.contains(**kind))
        .collect();
    assert!(unused.is_empty(), "documented but never seen: {unused:?}");
}

#[test]
fn later_staves_keep_the_track_tuning_in_guitar_pro() {
    let found: Vec<(ElementId, String, String)> = pairs()
        .iter()
        .flat_map(|pair| later_staff_tunings(&pair.golden))
        .collect();
    assert_eq!(found.len(), LATER_STAFF_TUNINGS, "{found:?}");
    for (id, pitches, track) in found {
        assert_eq!(pitches, track, "{id:?}");
        assert_eq!(pitches, "40 45 50 55 59 64");
    }
}

#[test]
fn guitar_pro_respells_exactly_the_counted_transposed_pitches_enharmonically() {
    let found: Vec<(ElementId, Option<i32>, Option<i32>)> = pairs()
        .iter()
        .flat_map(|pair| respelled_pitches(&pair.golden))
        .collect();
    assert_eq!(found.len(), RESPELLED_PITCHES, "{found:?}");
    for (id, spelled, written) in found {
        assert!(spelled.is_some(), "{id:?}");
        assert_eq!(spelled, written, "{id:?}");
    }
}

#[test]
fn every_normalisation_is_needed() {
    let pairs = pairs();
    for normalisation in &NORMALISATIONS {
        let needed = pairs.iter().any(|pair| {
            let (fixture, golden) = normalised(pair, Some(normalisation.name));
            fixture != golden
        });
        assert!(
            needed,
            "{} changes no fixture comparison",
            normalisation.name
        );
    }
}

#[test]
fn payload_kinds_name_their_parent_guard_and_element() {
    assert_eq!(
        payload_kind("<Keep parent=\"\" index=\"3\"><Color>1 2 3</Color></Keep>"),
        "/Color"
    );
    assert_eq!(
        payload_kind(
            "<Keep parent=\"Properties\" index=\"4\" guard=\"pitch:fretted 4 1\"><Property name=\"TransposedPitch\"><Pitch/></Property></Keep>"
        ),
        "pitch:Properties/Property@TransposedPitch"
    );
    assert_eq!(
        payload_kind("<Keep parent=\"Sounds/Sound#0/MIDI\" index=\"0\"><LSB>0</LSB></Keep>"),
        "Sounds/Sound/MIDI/LSB"
    );
    assert_eq!(
        payload_kind("<Keep parent=\"MasterBars\" index=\"0\"><!-- x --></Keep>"),
        "MasterBars/comment"
    );
    assert_eq!(
        payload_kind("<Keep parent=\"Notes\" index=\"0\">\n</Keep>"),
        "Notes/text"
    );
    assert_eq!(
        spelled_midi("<Pitch><Step>B</Step><Accidental>#</Accidental><Octave>3</Octave></Pitch>"),
        Some(48)
    );
    assert_eq!(
        spelled_midi("<Pitch><Step>E</Step><Accidental>b</Accidental><Octave>2</Octave></Pitch>"),
        Some(27)
    );
    assert_eq!(
        spelled_midi("<Pitch><Step>C</Step><Accidental></Accidental><Octave>4</Octave></Pitch>"),
        Some(48)
    );
    let tempo = Tempo::new(DEFAULT_BPM).expect("tempo");
    assert!(is_default_tempo(&TempoChange {
        position: opentab_model::Fraction::from_integer(0),
        bpm: tempo,
        unit: TempoUnit::Quarter,
    }));
}

#[test]
fn guitar_pro_drops_the_ghost_flag_only_on_accented_notes() {
    let pair = pairs()
        .into_iter()
        .find(|pair| pair.name == GHOST_ACCENT)
        .expect("the ghost-accent fixture");
    let (fixture, golden) = normalised(&pair, Some(GHOST_NORMALISATION));
    let ours = notes_of(&fixture);
    let theirs = notes_of(&golden);
    assert_eq!(ours.len(), theirs.len());
    let changed: Vec<(&&Note, &&Note)> = ours
        .iter()
        .zip(&theirs)
        .filter(|(mine, gp)| mine.effects != gp.effects)
        .collect();
    assert_eq!(changed.len(), GHOSTS_DROPPED_WITH_ACCENTS, "{changed:?}");
    for (mine, gp) in changed {
        assert!(accented_ghost(mine), "{mine:?}");
        assert!(!gp.effects.ghost, "{gp:?}");
        let mut expected = mine.effects.clone();
        expected.ghost = false;
        assert_eq!(expected, gp.effects);
    }
    let plain_ghosts = ours
        .iter()
        .filter(|note| note.effects.ghost && !accented_ghost(note))
        .count();
    assert!(plain_ghosts > 0);
    assert_eq!(
        plain_ghosts,
        theirs.iter().filter(|note| note.effects.ghost).count()
    );
}
