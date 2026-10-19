use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::LazyLock;

use opentab_command::{Command, History, MetaField};
use opentab_gp::{GpError, Refusal, read_gp, write_gp};
use opentab_model::{
    BarId, Beat, BeatId, BendPoint, DIRECTION_JUMPS, DIRECTION_TARGETS, DirectionJump,
    DirectionTarget, Directions, Duration, ElementId, IdAllocator, InstrumentKind, NoteId,
    NoteKind, NoteValue, Score, TimeSignature, TremoloBar, Trill, TrillSpeed, VoiceId, gm_drum_kit,
};
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::select;
use proptest::test_runner::{Config, FileFailurePersistence, TestRng, TestRunner};

use crate::anchors::{blocks, field, gpif_text, offsets};
use crate::corpus::corpus_paths;
use crate::inventory::{
    INSTRUMENT_SET_ORDER, instrument_set_children, master_bar_children, note_children,
    out_of_note_order, out_of_order, out_of_set_order,
};
use crate::rich::{
    ACCENT_MARKER, BRUSH_MARKER, FRAME_OFFSETS, HOPO_DESTINATION, INTRO_MARKER, MASTER_MARKER,
    ORNAMENT_MARKER, PITCH_MARKER, ROOT_MARKER, Rich, SECOND_SOUND, SIMILE_MARKER, SOUND_MARKER,
    UNREFERENCED_MARKER, rich,
};

const CASES: u32 = 256;
const MAX_EDITS: usize = 14;
const MAX_FRET: u8 = 24;
const STRINGS: u8 = 6;
const CORPUS_DRUM_TRACKS: usize = 10;

static RICH: LazyLock<Rich> = LazyLock::new(rich);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    SetMeta,
    SetAnacrusis,
    InsertMasterBar,
    RemoveMasterBar,
    InsertBeatInVoice,
    InsertBeatInBar,
    RemoveBeat,
    RemoveVoice,
    SetBeatDuration,
    AddNote,
    RemoveNote,
    SetFret,
    SetTempo,
    SetDirections,
    SetTempoLinear,
    SetTremoloBar,
    SetTrill,
    PadStaffBars,
    TrimStaffBars,
}

const OPS: [Op; 19] = [
    Op::SetMeta,
    Op::SetAnacrusis,
    Op::InsertMasterBar,
    Op::RemoveMasterBar,
    Op::InsertBeatInVoice,
    Op::InsertBeatInBar,
    Op::RemoveBeat,
    Op::RemoveVoice,
    Op::SetBeatDuration,
    Op::AddNote,
    Op::RemoveNote,
    Op::SetFret,
    Op::SetTempo,
    Op::SetDirections,
    Op::SetTempoLinear,
    Op::SetTremoloBar,
    Op::SetTrill,
    Op::PadStaffBars,
    Op::TrimStaffBars,
];

const FORWARD_OPS: [&str; 18] = [
    "set_meta",
    "set_anacrusis",
    "insert_master_bar",
    "remove_master_bar",
    "insert_beat",
    "remove_beat",
    "remove_voice",
    "set_beat_duration",
    "add_note",
    "remove_note",
    "set_fret",
    "set_tempo",
    "set_directions",
    "set_tempo_linear",
    "set_tremolo_bar",
    "set_trill",
    "pad_staff_bars",
    "trim_staff_bars",
];

#[derive(Debug, Clone, Copy)]
struct Edit {
    op: Op,
    pick: usize,
    value: u8,
}

#[derive(Default)]
struct Catalog {
    bars: Vec<BarId>,
    voices: Vec<(VoiceId, usize)>,
    beats: Vec<BeatId>,
    notes: Vec<NoteId>,
}

fn catalog(score: &Score) -> Catalog {
    let mut found = Catalog::default();
    for track in &score.tracks {
        for staff in &track.staves {
            for bar in &staff.bars {
                found.bars.push(bar.id);
                for voice in &bar.voices {
                    found.voices.push((voice.id, voice.beats.len()));
                    for beat in &voice.beats {
                        found.beats.push(beat.id);
                        found.notes.extend(beat.notes.iter().map(|note| note.id));
                    }
                }
            }
        }
    }
    found
}

fn pick<T: Copy>(items: &[T], at: usize, missing: T) -> T {
    if items.is_empty() {
        missing
    } else {
        items[at % items.len()]
    }
}

fn duration(value: u8) -> Duration {
    let note = match value % 3 {
        0 => NoteValue::Quarter,
        1 => NoteValue::Eighth,
        _ => NoteValue::Half,
    };
    Duration::new(note, value % 2, None).expect("duration")
}

fn directions(value: u8) -> Directions {
    let target = DIRECTION_TARGETS[usize::from(value) % DIRECTION_TARGETS.len()];
    let jump = DIRECTION_JUMPS[usize::from(value) % DIRECTION_JUMPS.len()];
    match value % 4 {
        0 => Directions::default(),
        1 => Directions {
            targets: vec![target],
            jumps: Vec::new(),
        },
        2 => Directions {
            targets: Vec::new(),
            jumps: vec![jump],
        },
        _ => Directions {
            targets: vec![target],
            jumps: vec![jump],
        },
    }
}

fn tremolo_bar(value: u8) -> Option<TremoloBar> {
    if value.is_multiple_of(3) {
        return None;
    }
    let depth = -i16::from(value % 7) * 50;
    let point = |offset, value| BendPoint::new(offset, value).expect("point");
    Some(
        TremoloBar::new(
            point(0, 0),
            point(30, depth),
            point(60, depth),
            point(100, 0),
        )
        .expect("tremolo bar"),
    )
}

fn trill(value: u8) -> Option<Trill> {
    if value.is_multiple_of(3) {
        return None;
    }
    let speeds = [
        NoteValue::ThirtySecond,
        NoteValue::Sixteenth,
        NoteValue::Eighth,
        NoteValue::Quarter,
    ];
    Some(Trill {
        fret: value % MAX_FRET,
        speed: TrillSpeed::new(speeds[usize::from(value % 4)]).expect("speed"),
    })
}

fn check_child_order(text: &str) -> Result<(), TestCaseError> {
    for children in master_bar_children(text) {
        prop_assert_eq!(out_of_order(&children), None);
    }
    for children in note_children(text) {
        prop_assert_eq!(out_of_note_order(&children), None);
    }
    Ok(())
}

fn resolve(edit: Edit, score: &Score) -> Command {
    let found = catalog(score);
    let bars = score.master_bars.len();
    let nobody = ElementId(u64::MAX);
    let Edit {
        op,
        pick: at,
        value,
    } = edit;
    match op {
        Op::SetMeta => Command::SetMeta {
            field: MetaField::Title,
            value: format!("title {value}"),
        },
        Op::SetAnacrusis => Command::SetAnacrusis {
            value: value % 2 == 0,
        },
        Op::InsertMasterBar => Command::InsertMasterBar {
            index: at % (bars + 1),
            time_signature: TimeSignature::new(3 + value % 2, 4).expect("time"),
        },
        Op::RemoveMasterBar => Command::RemoveMasterBar {
            index: at % bars.max(1),
        },
        Op::InsertBeatInVoice => {
            let (voice, len) = pick(&found.voices, at, (VoiceId(nobody), 0));
            Command::InsertBeat {
                voice: Some(voice),
                bar: None,
                index: (at / 7) % (len + 1),
                duration: duration(value),
            }
        }
        Op::InsertBeatInBar => Command::InsertBeat {
            voice: None,
            bar: Some(pick(&found.bars, at, BarId(nobody))),
            index: 0,
            duration: duration(value),
        },
        Op::RemoveBeat => Command::RemoveBeat {
            beat: pick(&found.beats, at, BeatId(nobody)),
        },
        Op::RemoveVoice => Command::RemoveVoice {
            voice: pick(&found.voices, at, (VoiceId(nobody), 0)).0,
        },
        Op::SetBeatDuration => Command::SetBeatDuration {
            beat: pick(&found.beats, at, BeatId(nobody)),
            duration: duration(value),
        },
        Op::AddNote => Command::AddNote {
            beat: pick(&found.beats, at, BeatId(nobody)),
            string: value % STRINGS,
            fret: value % MAX_FRET,
        },
        Op::RemoveNote => Command::RemoveNote {
            note: pick(&found.notes, at, NoteId(nobody)),
        },
        Op::SetFret => Command::SetFret {
            note: pick(&found.notes, at, NoteId(nobody)),
            fret: value % MAX_FRET,
        },
        Op::SetTempo => Command::SetTempo {
            master_bar: at % bars.max(1),
            bpm: 60.0 + f64::from(value),
        },
        Op::SetDirections => Command::SetDirections {
            master_bar: at % bars.max(1),
            directions: directions(value),
        },
        Op::SetTempoLinear => Command::SetTempoLinear {
            master_bar: at % bars.max(1),
            index: usize::from(value % 2),
            linear: value % 3 == 0,
        },
        Op::SetTremoloBar => Command::SetTremoloBar {
            beat: pick(&found.beats, at, BeatId(nobody)),
            tremolo_bar: tremolo_bar(value),
        },
        Op::SetTrill => Command::SetTrill {
            note: pick(&found.notes, at, NoteId(nobody)),
            trill: trill(value),
        },
        Op::PadStaffBars | Op::TrimStaffBars => {
            let track = &score.tracks[0];
            let staff = &track.staves[0];
            if op == Op::PadStaffBars {
                Command::PadStaffBars {
                    track: track.id,
                    staff: staff.id,
                }
            } else {
                Command::TrimStaffBars {
                    track: track.id,
                    staff: staff.id,
                    len: at % (staff.bars.len() + 1),
                }
            }
        }
    }
}

fn op_name(command: &Command) -> String {
    let name = match command {
        Command::SetMeta { .. } => "set_meta",
        Command::SetAnacrusis { .. } => "set_anacrusis",
        Command::InsertMasterBar { .. } => "insert_master_bar",
        Command::RemoveMasterBar { .. } => "remove_master_bar",
        Command::InsertBeat { .. } => "insert_beat",
        Command::RemoveBeat { .. } => "remove_beat",
        Command::RemoveVoice { .. } => "remove_voice",
        Command::SetBeatDuration { .. } => "set_beat_duration",
        Command::AddNote { .. } => "add_note",
        Command::RemoveNote { .. } => "remove_note",
        Command::SetFret { .. } => "set_fret",
        Command::SetTempo { .. } => "set_tempo",
        Command::SetDirections { .. } => "set_directions",
        Command::SetTempoLinear { .. } => "set_tempo_linear",
        Command::SetTremoloBar { .. } => "set_tremolo_bar",
        Command::SetTrill { .. } => "set_trill",
        Command::PadStaffBars { .. } => "pad_staff_bars",
        Command::TrimStaffBars { .. } => "trim_staff_bars",
        Command::SetDrumKit { .. } => "set_drum_kit",
        _ => "restore",
    };
    name.to_string()
}

fn musical(score: &Score) -> Score {
    let mut copy = score.clone();
    let position = |id: ElementId| {
        score
            .master_bars
            .iter()
            .position(|master| master.id.0 == id)
            .map_or(ElementId(u64::MAX), |index| ElementId(index as u64))
    };
    for track in &mut copy.tracks {
        for automation in &mut track.automations {
            automation.master_bar.0 = position(automation.master_bar.0);
        }
    }
    for entry in &mut copy.sound_overrides {
        entry.track.0 = score
            .tracks
            .iter()
            .position(|track| track.id == entry.track)
            .map_or(ElementId(u64::MAX), |index| ElementId(index as u64));
    }
    copy.ids = IdAllocator::default();
    copy.extensions = opentab_model::Extensions::default();
    copy.document_extensions.clear();
    copy.container.clear();
    let zero = ElementId(0);
    for master in &mut copy.master_bars {
        master.id.0 = zero;
    }
    for track in &mut copy.tracks {
        track.id.0 = zero;
        for staff in &mut track.staves {
            staff.id.0 = zero;
            for bar in &mut staff.bars {
                bar.id.0 = zero;
                for voice in &mut bar.voices {
                    voice.id.0 = zero;
                    for beat in &mut voice.beats {
                        beat.id.0 = zero;
                        for note in &mut beat.notes {
                            note.id.0 = zero;
                        }
                    }
                }
            }
        }
    }
    copy
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NotePlace {
    staff: usize,
    bar: usize,
    voice: usize,
    beat: usize,
}

fn place_of(score: &Score, id: NoteId) -> Option<(NotePlace, NoteKind)> {
    let staff = &score.tracks.first()?.staves.first()?;
    for (bar_index, bar) in staff.bars.iter().enumerate() {
        for (voice_index, voice) in bar.voices.iter().enumerate() {
            for (beat_index, beat) in voice.beats.iter().enumerate() {
                if let Some(note) = beat.notes.iter().find(|note| note.id == id) {
                    let place = NotePlace {
                        staff: 0,
                        bar: bar_index,
                        voice: voice_index,
                        beat: beat_index,
                    };
                    return Some((place, note.kind));
                }
            }
        }
    }
    None
}

fn previous_beat(score: &Score, place: NotePlace) -> Option<&Beat> {
    let bars = &score.tracks.first()?.staves.get(place.staff)?.bars;
    let voice = bars.get(place.bar)?.voices.get(place.voice)?;
    if place.beat > 0 {
        return voice.beats.get(place.beat - 1);
    }
    let earlier = bars.get(place.bar.checked_sub(1)?)?;
    earlier
        .voices
        .iter()
        .find(|candidate| candidate.slot == voice.slot)?
        .beats
        .last()
}

fn origin(score: &Score, id: NoteId) -> Option<Option<bool>> {
    let (place, kind) = place_of(score, id)?;
    let NoteKind::Fretted { string, .. } = kind else {
        return Some(None);
    };
    let previous = previous_beat(score, place);
    Some(previous.and_then(|beat| {
        beat.notes
            .iter()
            .find(|note| matches!(note.kind, NoteKind::Fretted { string: other, .. } if other == string))
            .map(|note| note.effects.hammer_on_pull_off)
    }))
}

fn note_exists(score: &Score, id: NoteId) -> bool {
    place_of(score, id).is_some()
}

fn beat_exists(score: &Score, id: ElementId) -> bool {
    catalog(score).beats.iter().any(|beat| beat.0 == id)
}

fn bar_exists(score: &Score, id: BarId) -> bool {
    catalog(score).bars.contains(&id)
}

fn master_index(score: &Score, id: ElementId) -> Option<usize> {
    score
        .master_bars
        .iter()
        .position(|master| master.id.0 == id)
}

fn successor(score: &Score, id: ElementId) -> usize {
    master_index(score, id).unwrap_or_else(|| {
        score
            .master_bars
            .iter()
            .position(|master| master.id.0 > id)
            .unwrap_or(score.master_bars.len().saturating_sub(1))
    })
}

fn sync_point(text: &str, frame: &str) -> Option<(usize, usize)> {
    blocks(text, "Automation")
        .into_iter()
        .find(|block| block.contains(&format!("<FrameOffset>{frame}</FrameOffset>")))
        .map(|block| {
            let number = |name: &str| {
                field(&block, name)
                    .and_then(|text| text.parse().ok())
                    .unwrap_or(usize::MAX)
            };
            (number("Bar"), number("BarIndex"))
        })
}

fn sound_bar(text: &str) -> Option<usize> {
    blocks(text, "Automation")
        .into_iter()
        .find(|block| block.contains(SOUND_MARKER))
        .and_then(|block| field(&block, "Bar"))
        .and_then(|text| text.parse().ok())
}

fn check_markers(text: &str, score: &Score) -> Result<(), TestCaseError> {
    let original = &*RICH;
    let first_master = original.master_ids[0];
    let has_first = master_index(score, first_master).is_some();
    let pitch_kept = place_of(score, original.pitched).map(|(_, kind)| kind)
        == place_of(&original.score, original.pitched).map(|(_, kind)| kind);
    let expected = [
        (ROOT_MARKER, true),
        (SECOND_SOUND, true),
        (UNREFERENCED_MARKER, true),
        (INTRO_MARKER, has_first),
        (MASTER_MARKER, has_first),
        (PITCH_MARKER, pitch_kept),
        (ACCENT_MARKER, note_exists(score, original.accented)),
        (ORNAMENT_MARKER, note_exists(score, original.ornamented)),
        (BRUSH_MARKER, beat_exists(score, original.brushed)),
        (SIMILE_MARKER, bar_exists(score, original.simile)),
    ];
    for (marker, present) in expected {
        prop_assert_eq!(text.contains(marker), present, "{}", marker);
    }
    let valid = original
        .destinations
        .iter()
        .filter(|id| {
            origin(score, **id).is_some() && origin(score, **id) == origin(&original.score, **id)
        })
        .count();
    prop_assert_eq!(text.matches(HOPO_DESTINATION).count(), valid);
    let ids = &original.master_ids;
    prop_assert_eq!(sound_bar(text), master_index(score, ids[2]));
    prop_assert_eq!(
        offsets(text),
        vec![successor(score, ids[1]), successor(score, ids[0])]
    );
    for (frame, id) in [(FRAME_OFFSETS[1], ids[2]), (FRAME_OFFSETS[2], ids[3])] {
        let found = sync_point(text, frame).map(|(_, index)| index);
        prop_assert_eq!(found, master_index(score, id), "{}", frame);
    }
    if let Some((_, index)) = sync_point(text, FRAME_OFFSETS[0]) {
        prop_assert_eq!(Some(index), master_index(score, ids[0]));
    }
    Ok(())
}

fn allowed(reason: &Refusal) -> bool {
    matches!(reason, Refusal::BarCount { .. } | Refusal::NoMasterBars)
}

struct Tally {
    applied: BTreeMap<String, usize>,
    written: usize,
    refused: usize,
}

fn arb_edit() -> impl Strategy<Value = Edit> {
    (select(OPS.to_vec()), any::<usize>(), any::<u8>()).prop_map(|(op, pick, value)| Edit {
        op,
        pick,
        value,
    })
}

fn run_edits(edits: &[Edit], tally: &RefCell<Tally>) -> Result<(), TestCaseError> {
    let original = &*RICH;
    let before =
        write_gp(&original.score).map_err(|error| TestCaseError::fail(error.to_string()))?;
    let mut score = original.score.clone();
    let mut history = History::new();
    let mut executed = 0usize;
    for edit in edits {
        let command = resolve(*edit, &score);
        let name = op_name(&command);
        if history.execute(&mut score, command).is_ok() {
            executed += 1;
            *tally.borrow_mut().applied.entry(name).or_default() += 1;
        }
    }
    match write_gp(&score) {
        Ok(bytes) => {
            tally.borrow_mut().written += 1;
            let again = read_gp(&bytes).map_err(|error| TestCaseError::fail(error.to_string()))?;
            prop_assert_eq!(musical(&again), musical(&score));
            check_markers(&gpif_text(&bytes), &score)?;
            check_child_order(&gpif_text(&bytes))?;
        }
        Err(GpError::Unsupported { reason, path }) => {
            tally.borrow_mut().refused += 1;
            prop_assert!(allowed(&reason), "{reason:?} at {path}");
        }
        Err(other) => return Err(TestCaseError::fail(other.to_string())),
    }
    for _ in 0..executed {
        prop_assert!(history.undo(&mut score).expect("undo never fails"));
    }
    let after = write_gp(&score).map_err(|error| TestCaseError::fail(error.to_string()))?;
    prop_assert!(
        after == before,
        "undoing every edit changes the written bytes"
    );
    Ok(())
}

#[test]
fn edited_rich_documents_write_without_stale_opaque_data() {
    let tally = RefCell::new(Tally {
        applied: BTreeMap::new(),
        written: 0,
        refused: 0,
    });
    let config = Config {
        cases: CASES,
        failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
        ..Config::default()
    };
    let rng = TestRng::deterministic_rng(config.rng_algorithm);
    let mut runner = TestRunner::new_with_rng(config, rng);
    let outcome = runner.run(&vec(arb_edit(), 0..=MAX_EDITS), |edits| {
        run_edits(&edits, &tally)
    });
    if let Err(error) = outcome {
        panic!("{error}");
    }
    let tally = tally.into_inner();
    println!(
        "edits: written {} refused {} applied {:?}",
        tally.written, tally.refused, tally.applied
    );
    for op in FORWARD_OPS {
        assert!(
            tally.applied.get(op).copied().unwrap_or(0) > 0,
            "{op} never applied: {:?}",
            tally.applied
        );
    }
    assert!(tally.written > CASES as usize / 2, "{}", tally.written);
    assert!(tally.refused > 0);
}

#[test]
fn the_rich_document_carries_each_kind_of_guard() {
    let original = &*RICH;
    let all: Vec<String> = original
        .score
        .extensions
        .0
        .values()
        .flatten()
        .map(|payload| payload.data.clone())
        .collect();
    for guard in [
        "guard=\"pitch:",
        "guard=\"hopo:true\"",
        "guard=\"hopo:false\"",
        "guard=\"hopo:none\"",
        "guard=\"accent:",
        "guard=\"stroke:",
        "guard=\"tempo-position:",
        "guard=\"tempo-linear:false;",
    ] {
        assert!(
            all.iter().any(|data| data.contains(guard)),
            "{guard}: {all:#?}"
        );
    }
    let text = gpif_text(&write_gp(&original.score).expect("write"));
    check_markers(&text, &original.score).expect("the unedited document keeps everything");
}

fn first_fretted_note(score: &Score, bar: usize) -> Option<(NoteId, u8)> {
    for track in &score.tracks {
        let Some(staff) = track.staves.first() else {
            continue;
        };
        let Some(found) = staff.bars.get(bar) else {
            continue;
        };
        for voice in &found.voices {
            for beat in &voice.beats {
                for note in &beat.notes {
                    if let NoteKind::Fretted { fret, .. } = note.kind {
                        return Some((note.id, fret));
                    }
                }
            }
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FullPlace {
    track: usize,
    staff: usize,
    bar: usize,
    voice: usize,
    beat: usize,
    note: usize,
}

fn full_place(score: &Score, id: NoteId) -> Option<FullPlace> {
    for (track, owner) in score.tracks.iter().enumerate() {
        for (staff, held) in owner.staves.iter().enumerate() {
            for (bar, found) in held.bars.iter().enumerate() {
                for (voice, line) in found.voices.iter().enumerate() {
                    for (beat, item) in line.beats.iter().enumerate() {
                        if let Some(note) = item.notes.iter().position(|note| note.id == id) {
                            return Some(FullPlace {
                                track,
                                staff,
                                bar,
                                voice,
                                beat,
                                note,
                            });
                        }
                    }
                }
            }
        }
    }
    None
}

fn note_at(score: &Score, place: FullPlace) -> Option<NoteId> {
    let beat = score
        .tracks
        .get(place.track)?
        .staves
        .get(place.staff)?
        .bars
        .get(place.bar)?
        .voices
        .get(place.voice)?
        .beats
        .get(place.beat)?;
    beat.notes.get(place.note).map(|note| note.id)
}

fn hopo_origin(score: &Score, destination: NoteId) -> Option<NoteId> {
    let place = full_place(score, destination)?;
    let bars = &score.tracks.get(place.track)?.staves.get(place.staff)?.bars;
    let voice = bars.get(place.bar)?.voices.get(place.voice)?;
    let previous = if place.beat > 0 {
        voice.beats.get(place.beat - 1)
    } else {
        let earlier = bars.get(place.bar.checked_sub(1)?)?;
        earlier
            .voices
            .iter()
            .find(|candidate| candidate.slot == voice.slot)
            .and_then(|candidate| candidate.beats.last())
    };
    let kind = voice.beats.get(place.beat)?.notes.get(place.note)?.kind;
    let NoteKind::Fretted { string, .. } = kind else {
        return None;
    };
    previous?
        .notes
        .iter()
        .find(|note| {
            matches!(note.kind, NoteKind::Fretted { string: other, .. } if other == string)
                && note.effects.hammer_on_pull_off
        })
        .map(|note| note.id)
}

fn hopo_destination(score: &Score) -> Option<(NoteId, NoteId)> {
    score.extensions.0.iter().find_map(|(id, payloads)| {
        let guarded = payloads
            .iter()
            .any(|payload| payload.data.contains("guard=\"hopo:true\""));
        if !guarded {
            return None;
        }
        let destination = NoteId(*id);
        hopo_origin(score, destination).map(|origin| (destination, origin))
    })
}

fn corpus_edits(score: &Score, history: &mut History, edited: &mut Score) -> Vec<String> {
    let mut applied = Vec::new();
    let mut run = |command: Command, edited: &mut Score, history: &mut History| {
        let name = op_name(&command);
        if history.execute(edited, command).is_ok() {
            applied.push(name);
        }
    };
    *edited = score.clone();
    run(
        Command::InsertMasterBar {
            index: 0,
            time_signature: TimeSignature::new(4, 4).expect("time"),
        },
        edited,
        history,
    );
    run(
        Command::SetTempo {
            master_bar: 1,
            bpm: 133.0,
        },
        edited,
        history,
    );
    if let Some((note, fret)) = first_fretted_note(edited, 2) {
        run(
            Command::SetFret {
                note,
                fret: (fret + 1) % MAX_FRET,
            },
            edited,
            history,
        );
    }
    let first_voice = catalog(edited).voices.first().map(|(voice, _)| *voice);
    if let Some(voice) = first_voice {
        let before = edited.ids.peek();
        run(
            Command::InsertBeat {
                voice: Some(voice),
                bar: None,
                index: 0,
                duration: duration(0),
            },
            edited,
            history,
        );
        run(
            Command::AddNote {
                beat: BeatId(before),
                string: 0,
                fret: 5,
            },
            edited,
            history,
        );
    }
    let voiceless = edited
        .tracks
        .iter()
        .flat_map(|track| &track.staves)
        .flat_map(|staff| &staff.bars)
        .find(|bar| bar.voices.is_empty())
        .map(|bar| bar.id);
    if let Some(bar) = voiceless {
        run(
            Command::InsertBeat {
                voice: None,
                bar: Some(bar),
                index: 0,
                duration: duration(2),
            },
            edited,
            history,
        );
    }
    let found = catalog(edited);
    if let Some(beat) = found.beats.get(found.beats.len() / 2) {
        run(Command::RemoveBeat { beat: *beat }, edited, history);
    }
    if let Some((voice, _)) = found.voices.get(found.voices.len() / 3) {
        run(Command::RemoveVoice { voice: *voice }, edited, history);
    }
    let last = edited.master_bars.len().saturating_sub(1);
    run(Command::RemoveMasterBar { index: last }, edited, history);
    run(
        Command::SetDirections {
            master_bar: edited.master_bars.len() / 2,
            directions: Directions {
                targets: vec![DirectionTarget::Coda],
                jumps: vec![DirectionJump::DaCoda],
            },
        },
        edited,
        history,
    );
    let drums = edited
        .tracks
        .iter()
        .find(|track| track.instrument.kind == InstrumentKind::Percussion)
        .map(|track| track.id);
    if let Some(track) = drums {
        run(
            Command::SetDrumKit {
                track,
                drum_kit: Some(gm_drum_kit()),
            },
            edited,
            history,
        );
    }
    applied
}

#[test]
fn corpus_examples_survive_edits_and_undo() {
    let Some(paths) = corpus_paths() else {
        return;
    };
    let mut applied: BTreeMap<String, usize> = BTreeMap::new();
    let mut dropped = 0usize;
    let mut ordered = 0usize;
    let mut kits = 0usize;
    for path in &paths {
        let name = path.display().to_string();
        let score = read_gp(&std::fs::read(path).expect("corpus file")).expect("reads");
        let before = write_gp(&score).expect("writes");
        let mut history = History::new();
        let mut edited = Score::default();
        for op in corpus_edits(&score, &mut history, &mut edited) {
            *applied.entry(op).or_default() += 1;
        }
        let mut destination = None;
        if let Some((target, origin)) = hopo_destination(&edited) {
            history
                .execute(&mut edited, Command::RemoveNote { note: origin })
                .expect("the origin note is removed");
            *applied.entry("remove_note".to_string()).or_default() += 1;
            destination = full_place(&edited, target);
        }
        let bytes = write_gp(&edited).unwrap_or_else(|error| panic!("{name}: {error}"));
        let bars = master_bar_children(&gpif_text(&bytes));
        for children in &bars {
            assert_eq!(out_of_order(children), None, "{name}");
        }
        for children in note_children(&gpif_text(&bytes)) {
            assert_eq!(out_of_note_order(&children), None, "{name}");
        }
        for (track, children) in edited
            .tracks
            .iter()
            .zip(instrument_set_children(&gpif_text(&bytes)))
        {
            assert_eq!(out_of_set_order(&children), None, "{name}");
            if track.instrument.kind == InstrumentKind::Percussion {
                assert_eq!(children, INSTRUMENT_SET_ORDER.to_vec(), "{name}");
                assert_eq!(track.drum_kit, Some(gm_drum_kit()), "{name}");
                kits += 1;
            }
        }
        let directed = bars.get(edited.master_bars.len() / 2);
        if directed.is_some_and(|children| {
            children.iter().any(|child| child == "Directions")
                && children.iter().any(|child| child == "XProperties")
        }) {
            ordered += 1;
        }
        let again = read_gp(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(musical(&again) == musical(&edited), "{name}: model differs");
        if let Some(place) = destination {
            let reread = note_at(&again, place).expect("destination is still there");
            let stale = again.extensions.0.get(&reread.0).is_some_and(|payloads| {
                payloads
                    .iter()
                    .any(|payload| payload.data.contains("HopoDestination"))
            });
            assert!(!stale, "{name}: a stale HopoDestination survived");
            dropped += 1;
        }
        while history.undo(&mut edited).expect("undo") {}
        assert!(
            write_gp(&edited).expect("writes") == before,
            "{name}: undo changes the bytes"
        );
    }
    println!(
        "corpus edits: {applied:?}, stale destinations checked: {dropped}, directed bars with XProperties: {ordered}"
    );
    assert!(
        ordered >= 10,
        "only {ordered} directed bars carry XProperties"
    );
    for op in [
        "insert_master_bar",
        "set_tempo",
        "set_fret",
        "insert_beat",
        "add_note",
        "remove_beat",
        "remove_voice",
        "remove_note",
        "remove_master_bar",
        "set_directions",
        "set_drum_kit",
    ] {
        assert!(
            applied.get(op).copied().unwrap_or(0) > 0,
            "{op}: {applied:?}"
        );
    }
    assert!(dropped >= 10, "only {dropped} files had a hopo destination");
    assert_eq!(kits, CORPUS_DRUM_TRACKS);
}

#[test]
fn a_voiceless_bar_from_a_gp_file_can_be_removed_trimmed_and_filled() {
    let original = &*RICH;
    let voiceless = &original.score.tracks[0].staves[0].bars[3];
    assert!(voiceless.voices.is_empty());
    let bar = voiceless.id;
    let mut removed = original.score.clone();
    opentab_command::apply(&mut removed, &Command::RemoveMasterBar { index: 3 })
        .expect("remove_master_bar accepts a voiceless bar");
    let bytes = write_gp(&removed).expect("write");
    assert_eq!(read_gp(&bytes).expect("read").master_bars.len(), 3);
    let mut trimmed = original.score.clone();
    let track = trimmed.tracks[0].id;
    let staff = trimmed.tracks[0].staves[0].id;
    let restore = opentab_command::apply(
        &mut trimmed,
        &Command::TrimStaffBars {
            track,
            staff,
            len: 3,
        },
    )
    .expect("trim_staff_bars accepts a voiceless bar");
    assert!(matches!(
        write_gp(&trimmed),
        Err(GpError::Unsupported {
            reason: Refusal::BarCount { .. },
            ..
        })
    ));
    opentab_command::apply(&mut trimmed, &restore).expect("restore");
    assert_eq!(trimmed, original.score);
    let mut filled = original.score.clone();
    opentab_command::apply(
        &mut filled,
        &Command::InsertBeat {
            voice: None,
            bar: Some(bar),
            index: 0,
            duration: duration(2),
        },
    )
    .expect("insert_beat creates voice slot 0");
    let again = read_gp(&write_gp(&filled).expect("write")).expect("read");
    let voices = &again.tracks[0].staves[0].bars[3].voices;
    assert_eq!(voices.len(), 1);
    assert_eq!(voices[0].beats.len(), 1);
}

#[test]
fn a_string_edit_on_a_hopo_destination_drops_it_and_a_fret_edit_keeps_it() {
    let original = &*RICH;
    let destination = original.destinations[0];
    let count = |score: &Score| {
        gpif_text(&write_gp(score).expect("write"))
            .matches(HOPO_DESTINATION)
            .count()
    };
    assert_eq!(count(&original.score), 3);
    let mut fretted = original.score.clone();
    opentab_command::apply(
        &mut fretted,
        &Command::SetFret {
            note: destination,
            fret: 7,
        },
    )
    .expect("set_fret");
    assert_eq!(count(&fretted), 3);
    let mut restrung = original.score.clone();
    let beat = &mut restrung.tracks[0].staves[0].bars[0].voices[0].beats[1];
    let note = beat
        .notes
        .iter_mut()
        .find(|note| note.id == destination)
        .expect("destination");
    let NoteKind::Fretted { string, fret } = note.kind else {
        panic!("fretted destination");
    };
    note.kind = NoteKind::Fretted {
        string: (string + 1) % STRINGS,
        fret,
    };
    assert_eq!(count(&restrung), 2);
}
