use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use opentab_gp::read_gp;
use opentab_model::{
    Automation, AutomationValue, ElementId, Fraction, IdAllocator, Note, OpaquePayload, Score,
    Sound, SoundRef, Tempo, TempoChange, TempoUnit, Track, to_json,
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
const SOUND_CHANGE: &str = "sound-change.gp";
const SOUND_VARIATION: &str = "/Variation";
const SOUND_MARKED_BEATS: usize = 8;
const SOUND_NORMALISATION: &str = "gp_marks_beats_with_their_sound";
const DIRECTIONS_PREFIX: &str = "directions-";
const EXPORT_MID: &str = "export.mid";
const NOTE_ON: u8 = 0x90;
const STATUS_KIND: u8 = 0xF0;
const STATUS_BIT: u8 = 0x80;
const META: u8 = 0xFF;
const SYSEX: [u8; 2] = [0xF0, 0xF7];
const ONE_DATA_BYTE: [u8; 2] = [0xC0, 0xD0];
const TAMPERED_JUMP: &str = "directions-ds-coda.gp";
const TEMPO: &str = "tempo.gp";
const SET_TEMPO: u8 = 0x51;
const TICKS_PER_WHOLE: i64 = 1920;
const TEMPO_TICK_TOLERANCE: i64 = 1;
const AWAITING_HARVEST: [&str; 4] = [SOUND_CHANGE, "tempo-ramp.gp", "tremolo-bar.gp", "trill.gp"];
const STALE_GOLDENS: [&str; 1] = [SOUND_CHANGE];
const NEEDED_BY_AWAITING: [(&str, &str); 1] = [(SOUND_NORMALISATION, SOUND_CHANGE)];

const GP_DEFAULT_PAYLOADS: [&str; 52] = [
    "/AudioEngineState",
    "/Color",
    "/ConcertPitchStemOrientation",
    "/Encoding",
    "/ForcedSound",
    "/IconId",
    "/Lyrics",
    "/PalmMute",
    "/PlaybackState",
    "/PlayingStyle",
    "/ScoreViews",
    "/SystemsDefautLayout",
    "/SystemsLayout",
    "/TransposedPitchStemOrientation",
    "drum-kit:InstrumentSet/Elements",
    "midi-connection:/MidiConnection",
    "note-kind:/InstrumentArticulation",
    "tempo-linear:*Tempo/Linear",
    "tempo-position:*Tempo/Visible",
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
    "RSE/ChannelStrip/@version",
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
    "Sounds/Sound/Path",
    "Sounds/Sound/RSE",
    "Sounds/Sound/Role",
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

const NORMALISATIONS: [Normalisation; 15] = [
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
        name: "gp_spells_out_the_implicit_sound",
        apply: gp_spells_out_the_implicit_sound,
    },
    Normalisation {
        name: "gp_starts_every_track_with_its_first_sound",
        apply: gp_starts_every_track_with_its_first_sound,
    },
    Normalisation {
        name: "gp_keeps_only_the_last_copy_of_each_direction",
        apply: gp_keeps_only_the_last_copy_of_each_direction,
    },
    Normalisation {
        name: "gp_writes_the_pan_lane_before_the_volume_lane",
        apply: gp_writes_the_pan_lane_before_the_volume_lane,
    },
    Normalisation {
        name: "gp_drops_our_sound_overrides",
        apply: gp_drops_our_sound_overrides,
    },
    Normalisation {
        name: SOUND_NORMALISATION,
        apply: gp_marks_beats_with_their_sound,
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

fn gp_spells_out_the_implicit_sound(fixture: &mut Score, golden: &mut Score) {
    for (ours, theirs) in fixture.tracks.iter().zip(golden.tracks.iter_mut()) {
        let implicit = Sound {
            name: String::new(),
            program: ours.instrument.midi_program,
            bank: ours.midi.bank,
        };
        if ours.sounds.is_empty() && theirs.sounds == [implicit] {
            theirs.sounds.clear();
        }
    }
}

fn gp_starts_every_track_with_its_first_sound(fixture: &mut Score, golden: &mut Score) {
    let Some(first) = golden.master_bars.first().map(|master| master.id) else {
        return;
    };
    let opening = Automation {
        master_bar: first,
        position: Fraction::from_integer(0),
        linear: false,
        visible: true,
        value: AutomationValue::Sound(SoundRef(0)),
    };
    for (ours, theirs) in fixture.tracks.iter().zip(golden.tracks.iter_mut()) {
        let named = ours.automations.iter().any(is_sound);
        if named {
            continue;
        }
        if let Some(index) = theirs.automations.iter().position(is_sound)
            && theirs.automations[index] == opening
        {
            theirs.automations.remove(index);
        }
    }
}

fn is_sound(automation: &Automation) -> bool {
    matches!(automation.value, AutomationValue::Sound(_))
}

fn gp_keeps_only_the_last_copy_of_each_direction(fixture: &mut Score, _golden: &mut Score) {
    let mut targets = BTreeSet::new();
    let mut jumps = BTreeSet::new();
    for master_bar in fixture.master_bars.iter_mut().rev() {
        let directions = &mut master_bar.directions;
        directions.targets.retain(|target| targets.insert(*target));
        directions.jumps.retain(|jump| jumps.insert(*jump));
    }
}

fn lane(automation: &Automation) -> u8 {
    match automation.value {
        AutomationValue::Pan(_) => 0,
        AutomationValue::Volume(_) => 1,
        AutomationValue::Sound(_) => 2,
    }
}

fn gp_writes_the_pan_lane_before_the_volume_lane(fixture: &mut Score, golden: &mut Score) {
    for track in &golden.tracks {
        assert!(
            track.automations.is_sorted_by_key(lane),
            "{}: GP's lanes are not pan, volume, sound",
            track.name
        );
    }
    for track in &mut fixture.tracks {
        track.automations.sort_by_key(lane);
    }
}

fn gp_drops_our_sound_overrides(fixture: &mut Score, golden: &mut Score) {
    assert!(golden.sound_overrides.is_empty());
    fixture.sound_overrides.clear();
}

struct SoundMark {
    beat: ElementId,
    sound: usize,
    sounds: usize,
}

fn beats_with_tracks(score: &Score) -> BTreeMap<ElementId, &Track> {
    let mut found = BTreeMap::new();
    for track in &score.tracks {
        for staff in &track.staves {
            for bar in &staff.bars {
                for voice in &bar.voices {
                    for beat in &voice.beats {
                        found.insert(beat.id.0, track);
                    }
                }
            }
        }
    }
    found
}

fn sound_marks(golden: &Score) -> Vec<SoundMark> {
    let beats = beats_with_tracks(golden);
    let mut found = Vec::new();
    for (id, payloads) in &golden.extensions.0 {
        for payload in payloads {
            if payload_kind(&payload.data) != SOUND_VARIATION {
                continue;
            }
            let track = beats
                .get(id)
                .unwrap_or_else(|| panic!("a Variation payload on {id:?}, which is not a beat"));
            let sound = element_text(&payload.data, "Variation")
                .and_then(|text| text.parse().ok())
                .unwrap_or_else(|| panic!("{id:?}: unreadable Variation {}", payload.data));
            found.push(SoundMark {
                beat: *id,
                sound,
                sounds: track.sounds.len(),
            });
        }
    }
    found
}

fn gp_marks_beats_with_their_sound(_fixture: &mut Score, golden: &mut Score) {
    for mark in sound_marks(golden) {
        assert!(
            (1..mark.sounds).contains(&mark.sound),
            "beat {:?}: Variation {} names no later sound of its track",
            mark.beat,
            mark.sound
        );
    }
    retain_payloads(golden, |_, payload| {
        payload_kind(&payload.data) != SOUND_VARIATION
    });
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

fn fixture_path(name: &str) -> PathBuf {
    root().join("fixtures").join("gp").join(name)
}

fn golden_path(name: &str, output: &str) -> PathBuf {
    root().join("goldens").join("gp").join(name).join(output)
}

fn pairs() -> Vec<Pair> {
    fixture_names()
        .into_iter()
        .filter(|name| !AWAITING_HARVEST.contains(&name.as_str()))
        .map(|name| Pair {
            fixture: read(&fixture_path(&name)),
            golden: read_resave(&golden_path(&name, RESAVE)),
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
    if data.starts_with("<KeepAttribute ") {
        return format!("{parent}/@{}", attribute(head, "name").unwrap_or_default());
    }
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
    let harvested: BTreeSet<String> = names
        .iter()
        .filter(|name| {
            !AWAITING_HARVEST.contains(&name.as_str()) || STALE_GOLDENS.contains(&name.as_str())
        })
        .cloned()
        .collect();
    assert_eq!(golden_dirs, harvested);
    for name in AWAITING_HARVEST {
        assert!(names.iter().any(|fixture| fixture == name), "{name}");
    }
    for name in STALE_GOLDENS {
        assert!(AWAITING_HARVEST.contains(&name), "{name}");
    }
    for name in &names {
        let fixture = read(&root().join("fixtures").join("gp").join(name));
        assert!(fixture.container.is_empty(), "{name}");
        assert!(fixture.extensions.0.is_empty(), "{name}");
        assert!(fixture.document_extensions.is_empty(), "{name}");
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
            if kind == LATER_STAFF_TUNING || kind == RESPELLED_PITCH || kind == SOUND_VARIATION {
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
        let awaiting = NEEDED_BY_AWAITING.iter().any(|(name, fixture)| {
            *name == normalisation.name && AWAITING_HARVEST.contains(fixture)
        });
        let needed = awaiting
            || pairs.iter().any(|pair| {
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
        payload_kind(
            "<KeepAttribute parent=\"RSE/ChannelStrip\" index=\"0\" name=\"version\" value=\"E56\"/>"
        ),
        "RSE/ChannelStrip/@version"
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
        linear: false,
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

#[test]
fn guitar_pro_marks_the_beats_after_each_sound_change_with_the_sound_index() {
    let golden = read_resave(&golden_path(SOUND_CHANGE, RESAVE));
    let marks = sound_marks(&golden);
    assert_eq!(marks.len(), SOUND_MARKED_BEATS);
    let overdrive = marks.iter().filter(|mark| mark.sound == 1).count();
    let nylon = marks.iter().filter(|mark| mark.sound == 2).count();
    assert_eq!((overdrive, nylon), (5, 3));
    let elsewhere: usize = pairs()
        .iter()
        .filter(|pair| pair.name != SOUND_CHANGE)
        .map(|pair| sound_marks(&pair.golden).len())
        .sum();
    assert_eq!(elsewhere, 0);
}

struct NoteOn {
    tick: usize,
    key: u8,
    offset: usize,
}

struct SmfReader<'b> {
    bytes: &'b [u8],
    at: usize,
}

impl<'b> SmfReader<'b> {
    fn byte(&mut self) -> u8 {
        let value = self.bytes[self.at];
        self.at += 1;
        value
    }

    fn take(&mut self, len: usize) -> &'b [u8] {
        let slice = &self.bytes[self.at..self.at + len];
        self.at += len;
        slice
    }

    fn quantity(&mut self) -> usize {
        let mut value = 0;
        loop {
            let byte = self.byte();
            value = (value << 7) | usize::from(byte & !STATUS_BIT);
            if byte & STATUS_BIT == 0 {
                return value;
            }
        }
    }

    fn length(&mut self) -> usize {
        self.take(4)
            .iter()
            .fold(0, |value, byte| (value << 8) | usize::from(*byte))
    }
}

#[derive(Default)]
struct Events {
    notes: Vec<NoteOn>,
    tempos: Vec<usize>,
}

fn track_events(reader: &mut SmfReader<'_>, end: usize, found: &mut Events) {
    let mut tick = 0;
    let mut status = 0;
    while reader.at < end {
        tick += reader.quantity();
        let first = reader.byte();
        if first == META {
            let kind = reader.byte();
            let len = reader.quantity();
            reader.take(len);
            if kind == SET_TEMPO {
                found.tempos.push(tick);
            }
            continue;
        }
        if SYSEX.contains(&first) {
            let len = reader.quantity();
            reader.take(len);
            continue;
        }
        let offset = if first & STATUS_BIT == 0 {
            reader.at - 1
        } else {
            status = first;
            reader.byte();
            reader.at - 1
        };
        let key = reader.bytes[offset];
        if ONE_DATA_BYTE.contains(&(status & STATUS_KIND)) {
            continue;
        }
        let velocity = reader.byte();
        if status & STATUS_KIND == NOTE_ON && velocity > 0 {
            found.notes.push(NoteOn { tick, key, offset });
        }
    }
}

fn events(smf: &[u8]) -> Events {
    let mut reader = SmfReader { bytes: smf, at: 0 };
    assert_eq!(reader.take(4), b"MThd");
    let header = reader.length();
    reader.take(header);
    let mut found = Events::default();
    while reader.at < smf.len() {
        let id = reader.take(4);
        let len = reader.length();
        let end = reader.at + len;
        if id == b"MTrk" {
            track_events(&mut reader, end, &mut found);
        }
        reader.at = end;
    }
    found.notes.sort_by_key(|note| note.tick);
    found.tempos.sort_unstable();
    found
}

fn note_ons(smf: &[u8]) -> Vec<NoteOn> {
    events(smf).notes
}

fn bar_of_each_key(fixture: &Score) -> BTreeMap<u8, usize> {
    let track = &fixture.tracks[0];
    let mut keys = BTreeMap::new();
    for (index, bar) in track.staves[0].bars.iter().enumerate() {
        let note = &bar.voices[0].beats[0].notes[0];
        let pitch = track.pitch_of(note).expect("a fretted note");
        assert_eq!(keys.insert(pitch.0, index), None, "two bars share a key");
    }
    keys
}

fn played_bars(fixture: &Score, smf: &[u8]) -> Vec<usize> {
    let keys = bar_of_each_key(fixture);
    note_ons(smf)
        .iter()
        .map(|note| {
            *keys
                .get(&note.key)
                .unwrap_or_else(|| panic!("key {} belongs to no bar", note.key))
        })
        .collect()
}

fn predicted_orders() -> BTreeMap<String, Vec<Vec<usize>>> {
    let path = root()
        .join("xtask")
        .join("src")
        .join("fixtures")
        .join("play-orders.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    serde_json::from_str(&text).expect("play orders parse")
}

fn directions_fixtures() -> Vec<String> {
    fixture_names()
        .into_iter()
        .filter(|name| name.starts_with(DIRECTIONS_PREFIX))
        .collect()
}

#[test]
fn guitar_pros_export_plays_every_directions_fixture_in_its_predicted_order() {
    let predicted = predicted_orders();
    let names = directions_fixtures();
    let listed: Vec<String> = predicted.keys().cloned().collect();
    assert_eq!(names, listed);
    for name in names {
        let fixture = read(&fixture_path(&name));
        let smf = std::fs::read(golden_path(&name, EXPORT_MID)).expect("export.mid");
        let played = played_bars(&fixture, &smf);
        assert_eq!(predicted[&name], vec![played], "{name}");
    }
}

struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("opentab-fidelity-{label}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch directory");
        Self { dir }
    }

    fn copy(&self, bytes: &[u8], file: &str) -> PathBuf {
        let path = self.dir.join(file);
        std::fs::write(&path, bytes).expect("scratch copy");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn compares_equal(name: &str, golden: &Path) -> bool {
    let pair = Pair {
        name: name.to_string(),
        fixture: read(&fixture_path(name)),
        golden: read_resave(golden),
    };
    let (fixture, golden) = normalised(&pair, None);
    fixture == golden
}

#[test]
fn an_altered_copy_of_a_published_golden_fails_the_fidelity_checks() {
    let scratch = Scratch::new("tamper");
    let resave = std::fs::read_to_string(golden_path(TAMPERED_JUMP, RESAVE)).expect("resave");
    let intact = scratch.copy(resave.as_bytes(), "intact.gpif");
    assert!(compares_equal(TAMPERED_JUMP, &intact));
    let altered_text = resave.replacen("<Jump>DaSegnoAlCoda</Jump>", "<Jump>DaSegno</Jump>", 1);
    assert_ne!(altered_text, resave);
    let altered = scratch.copy(altered_text.as_bytes(), "altered.gpif");
    assert!(!compares_equal(TAMPERED_JUMP, &altered));

    let fixture = read(&fixture_path(TAMPERED_JUMP));
    let predicted = predicted_orders()[TAMPERED_JUMP][0].clone();
    let mut smf = std::fs::read(golden_path(TAMPERED_JUMP, EXPORT_MID)).expect("export.mid");
    let intact_smf = std::fs::read(scratch.copy(&smf, "intact.mid")).expect("intact copy");
    assert_eq!(played_bars(&fixture, &intact_smf), predicted);
    let notes = note_ons(&smf);
    smf[notes[0].offset] = notes[1].key;
    let altered_smf = std::fs::read(scratch.copy(&smf, "altered.mid")).expect("altered copy");
    assert_ne!(played_bars(&fixture, &altered_smf), predicted);
}

fn bar_starts(score: &Score) -> Vec<Fraction> {
    let mut start = Fraction::from_integer(0);
    let mut starts = Vec::with_capacity(score.master_bars.len());
    for master in &score.master_bars {
        starts.push(start);
        start += master.time_signature.bar_length();
    }
    starts
}

#[test]
fn guitar_pro_places_each_tempo_change_at_its_fraction_of_the_bar() {
    let golden = read_resave(&golden_path(TEMPO, RESAVE));
    let starts = bar_starts(&golden);
    let mut expected = Vec::new();
    for (master, start) in golden.master_bars.iter().zip(&starts) {
        for change in &master.tempo_changes {
            let at = (*start + change.position) * TICKS_PER_WHOLE;
            expected.push(at.to_integer());
        }
    }
    assert!(
        golden
            .master_bars
            .iter()
            .flat_map(|master| &master.tempo_changes)
            .any(|change| change.position > Fraction::from_integer(0)),
        "the tempo golden holds tempo changes inside a bar"
    );
    let smf = std::fs::read(golden_path(TEMPO, EXPORT_MID)).expect("export.mid");
    let played = events(&smf).tempos;
    assert_eq!(played.len(), expected.len());
    for (ours, theirs) in expected.iter().zip(&played) {
        let theirs = i64::try_from(*theirs).expect("tick");
        assert!(
            (ours - theirs).abs() <= TEMPO_TICK_TOLERANCE,
            "ours {expected:?}, Guitar Pro {played:?}"
        );
    }
}

fn active_sound(
    track: &Track,
    starts: &[Fraction],
    at: Fraction,
    master_ids: &[ElementId],
) -> usize {
    let mut active = 0;
    let mut latest = None;
    for automation in &track.automations {
        let AutomationValue::Sound(SoundRef(sound)) = automation.value else {
            continue;
        };
        let Some(index) = master_ids
            .iter()
            .position(|id| *id == automation.master_bar.0)
        else {
            continue;
        };
        let when = starts[index] + automation.position;
        if when <= at && latest.is_none_or(|previous| when >= previous) {
            latest = Some(when);
            active = sound;
        }
    }
    active
}

#[test]
fn guitar_pro_switches_sound_at_the_first_beat_on_or_after_the_position_in_quarter_notes() {
    let golden = read_resave(&golden_path(SOUND_CHANGE, RESAVE));
    let marks: BTreeMap<ElementId, usize> = sound_marks(&golden)
        .iter()
        .map(|mark| (mark.beat, mark.sound))
        .collect();
    let starts = bar_starts(&golden);
    let master_ids: Vec<ElementId> = golden
        .master_bars
        .iter()
        .map(|master| master.id.0)
        .collect();
    let track = &golden.tracks[0];
    assert!(track.sounds.len() > 1);
    let between_beats = track.automations.iter().any(|automation| {
        let onsets = [0, 1, 2, 3].map(|quarter| Fraction::new(quarter, 4));
        !onsets.contains(&automation.position)
    });
    assert!(between_beats, "a sound change between two beats");
    let mut checked = 0;
    for (bar, start) in track.staves[0].bars.iter().zip(&starts) {
        for voice in &bar.voices {
            let mut onset = *start;
            for beat in &voice.beats {
                let theirs = marks.get(&beat.id.0).copied().unwrap_or(0);
                let ours = active_sound(track, &starts, onset, &master_ids);
                assert_eq!(ours, theirs, "beat at {onset}");
                checked += 1;
                if beat.effects.grace.is_none() {
                    onset += beat.duration.length();
                }
            }
        }
    }
    assert!(checked >= SOUND_MARKED_BEATS);
}

#[test]
fn a_golden_awaiting_harvest_no_longer_matches_its_fixture() {
    for name in STALE_GOLDENS {
        let pair = Pair {
            name: name.to_string(),
            fixture: read(&fixture_path(name)),
            golden: read_resave(&golden_path(name, RESAVE)),
        };
        let (fixture, golden) = normalised(&pair, None);
        assert_ne!(
            fixture, golden,
            "{name} matches its golden; drop it from STALE_GOLDENS"
        );
    }
}
