use opentab_command::{Command, MetaField};
use opentab_model::{
    Automation, AutomationValue, Balance, BarId, Beat, BeatId, BeatPath, DIRECTION_JUMPS,
    DIRECTION_TARGETS, Directions, Duration, ElementId, Fraction, InstrumentKind, Level,
    MasterBarId, MidiBank, MidiChannel, Note, NoteEffects, NoteId, NoteKind, NoteValue, Score,
    Sound, SoundRef, StaffId, Tempo, TempoChange, TempoUnit, TimeSignature, TrackId, TrackMidi,
    Tuplet, VoiceId, VoiceSlot,
};
use opentab_testkit::arb_score;
use proptest::collection::vec;
use proptest::option;
use proptest::prelude::*;
use proptest::sample::select;

use crate::support::score_ids;

const MAX_STEPS: usize = 20;
const VALID_RATE: f64 = 0.8;
const PERTURB_RATE: f64 = 0.75;
const WRONG_TRACK_EVERY: usize = 4;
const ALL_POOL_EVERY: usize = 4;
const PERTURB_SHIFT: usize = 7;
const MAX_DROPPED_BARS: usize = 2;
const SELECTOR_BITS: usize = 32;
const CLEAR_SHIFT: usize = 16;
const CORRUPT_RATE: f64 = 0.25;
const FIELDS: [MetaField; 10] = [
    MetaField::Title,
    MetaField::Subtitle,
    MetaField::Artist,
    MetaField::Album,
    MetaField::Words,
    MetaField::Music,
    MetaField::Copyright,
    MetaField::TabAuthor,
    MetaField::Instructions,
    MetaField::Notices,
];
const NOTE_VALUES: [NoteValue; 8] = [
    NoteValue::Whole,
    NoteValue::Half,
    NoteValue::Quarter,
    NoteValue::Eighth,
    NoteValue::Sixteenth,
    NoteValue::ThirtySecond,
    NoteValue::SixtyFourth,
    NoteValue::OneTwentyEighth,
];

#[derive(Debug, Clone, Copy)]
pub struct Pick {
    pub valid: bool,
    pub selector: usize,
    pub raw: u64,
}

#[derive(Debug, Clone)]
pub enum Step {
    SetMeta {
        field: usize,
        value: String,
    },
    SetAnacrusis {
        value: bool,
    },
    InsertMasterBar {
        index: Pick,
        numerator: u8,
        denominator: u8,
    },
    RemoveMasterBar {
        index: Pick,
    },
    InsertBeat {
        voice: Pick,
        index: Pick,
        duration: Duration,
    },
    InsertBeatInBar {
        bar: Pick,
        index: Pick,
        duration: Duration,
    },
    RemoveBeat {
        beat: Pick,
    },
    RemoveVoice {
        voice: Pick,
    },
    SetBeatDuration {
        beat: Pick,
        duration: Duration,
    },
    AddNote {
        beat: Pick,
        string: Pick,
        fret: u8,
    },
    RemoveNote {
        note: Pick,
    },
    SetFret {
        note: Pick,
        fret: u8,
    },
    SetTempo {
        master_bar: Pick,
        bpm: f64,
    },
    SetDirections {
        master_bar: Pick,
        directions: Directions,
    },
    PadStaffBars {
        track: Pick,
        staff: Pick,
    },
    TrimStaffBars {
        track: Pick,
        staff: Pick,
        len: Pick,
    },
    SetTrackMidi {
        track: Pick,
        midi: TrackMidi,
    },
    InsertAutomation {
        track: Pick,
        index: Pick,
        bar: Pick,
        share: Fraction,
        value: ValueSeed,
        linear: bool,
        visible: bool,
    },
    RemoveAutomation {
        track: Pick,
        index: Pick,
    },
    SetSoundOverride {
        track: Pick,
        bank: Option<String>,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum ValueSeed {
    Volume(u32),
    Pan(u32),
    Sound(Pick),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Corruption {
    TempoOutsideBar,
    DuplicateString,
    PercussionOnFretted,
    VoicelessBar,
    DuplicateId,
    IdAboveAllocator,
    StringOutOfRange,
    FirstSoundBank,
}

pub const CORRUPTIONS: [Corruption; 8] = [
    Corruption::TempoOutsideBar,
    Corruption::DuplicateString,
    Corruption::PercussionOnFretted,
    Corruption::VoicelessBar,
    Corruption::DuplicateId,
    Corruption::IdAboveAllocator,
    Corruption::StringOutOfRange,
    Corruption::FirstSoundBank,
];

#[derive(Debug, Clone)]
pub struct Case {
    pub score: Score,
    pub corruption: Option<Corruption>,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Copy)]
struct StaffInfo {
    track: TrackId,
    staff: StaffId,
    bars: usize,
    empty_tail: usize,
}

struct Catalog {
    all: Vec<ElementId>,
    tracks: Vec<TrackId>,
    staves: Vec<StaffInfo>,
    bars: Vec<BarId>,
    voiceless: Vec<BarId>,
    voices: Vec<VoiceId>,
    beats: Vec<BeatId>,
    open_beats: Vec<BeatId>,
    notes: Vec<NoteId>,
    fretted_notes: Vec<NoteId>,
}

impl Catalog {
    fn of(score: &Score) -> Catalog {
        let mut catalog = Catalog {
            all: score_ids(score),
            tracks: Vec::new(),
            staves: Vec::new(),
            bars: Vec::new(),
            voiceless: Vec::new(),
            voices: Vec::new(),
            beats: Vec::new(),
            open_beats: Vec::new(),
            notes: Vec::new(),
            fretted_notes: Vec::new(),
        };
        for track in &score.tracks {
            catalog.tracks.push(track.id);
            for staff in &track.staves {
                let empty_tail = staff
                    .bars
                    .iter()
                    .rev()
                    .take_while(|bar| bar.voices.iter().all(|voice| voice.beats.is_empty()))
                    .count();
                catalog.staves.push(StaffInfo {
                    track: track.id,
                    staff: staff.id,
                    bars: staff.bars.len(),
                    empty_tail,
                });
                for bar in &staff.bars {
                    catalog.bars.push(bar.id);
                    if bar.voices.is_empty() {
                        catalog.voiceless.push(bar.id);
                    }
                    for voice in &bar.voices {
                        catalog.voices.push(voice.id);
                        for beat in &voice.beats {
                            catalog.beats.push(beat.id);
                            if track.instrument.kind == InstrumentKind::Fretted
                                && beat.notes.len() < track.tuning.strings.len()
                            {
                                catalog.open_beats.push(beat.id);
                            }
                            catalog.notes.extend(beat.notes.iter().map(|note| note.id));
                            if track.instrument.kind == InstrumentKind::Fretted {
                                catalog
                                    .fretted_notes
                                    .extend(beat.notes.iter().map(|note| note.id));
                            }
                        }
                    }
                }
            }
        }
        catalog
    }

    fn staff(&self, track: Pick, staff: Pick, preferred: &[StaffInfo]) -> StaffInfo {
        let pool = if preferred.is_empty() {
            &self.staves
        } else {
            preferred
        };
        let mut info = if staff.valid && !pool.is_empty() {
            pool[staff.selector % pool.len()]
        } else {
            let ids: Vec<StaffId> = self.staves.iter().map(|info| info.staff).collect();
            StaffInfo {
                track: self.id(track, &self.tracks),
                staff: self.id(staff, &ids),
                bars: 0,
                empty_tail: 0,
            }
        };
        if !track.valid && track.selector.is_multiple_of(WRONG_TRACK_EVERY) {
            info.track = self.id(track, &self.tracks);
        }
        info
    }

    fn id<T: Copy + From<ElementId>>(&self, pick: Pick, pool: &[T]) -> T {
        if pick.valid && !pool.is_empty() {
            return pool[pick.selector % pool.len()];
        }
        if pick.selector % 2 == 1 && !self.all.is_empty() {
            return T::from(self.all[pick.selector % self.all.len()]);
        }
        T::from(ElementId(pick.raw))
    }
}

fn insert_index(pick: Pick, len: usize) -> usize {
    if pick.valid {
        pick.selector % (len + 1)
    } else {
        len + 1 + pick.selector % 3
    }
}

fn existing_index(pick: Pick, len: usize) -> usize {
    if pick.valid && len > 0 {
        pick.selector % len
    } else {
        len + pick.selector % 3
    }
}

fn voice_len(score: &Score, voice: VoiceId) -> usize {
    score
        .find_voice(voice)
        .map(|path| {
            score.tracks[path.track].staves[path.staff].bars[path.bar].voices[path.voice]
                .beats
                .len()
        })
        .unwrap_or(0)
}

fn first_slot_len(score: &Score, bar: BarId) -> usize {
    score
        .tracks
        .iter()
        .flat_map(|track| &track.staves)
        .flat_map(|staff| &staff.bars)
        .find(|candidate| candidate.id == bar)
        .and_then(|found| {
            found
                .voices
                .iter()
                .find(|voice| voice.slot == VoiceSlot::default())
        })
        .map_or(0, |voice| voice.beats.len())
}

fn strings_of(score: &Score, beat: BeatId) -> Strings {
    let Some(path) = score.find_beat(beat) else {
        return Strings::default();
    };
    let track = &score.tracks[path.track];
    let used: Vec<u8> = track.staves[path.staff].bars[path.bar].voices[path.voice].beats[path.beat]
        .notes
        .iter()
        .filter_map(|note| match note.kind {
            NoteKind::Fretted { string, .. } => Some(string),
            NoteKind::Percussion { .. } => None,
        })
        .collect();
    let count = track.tuning.strings.len();
    Strings {
        count,
        free: (0..count)
            .filter_map(|string| u8::try_from(string).ok())
            .filter(|string| !used.contains(string))
            .collect(),
        used,
    }
}

#[derive(Debug, Default)]
struct Strings {
    count: usize,
    free: Vec<u8>,
    used: Vec<u8>,
}

fn pick_string(pick: Pick, strings: &Strings) -> u8 {
    if pick.valid && !strings.free.is_empty() {
        return strings.free[pick.selector % strings.free.len()];
    }
    if pick.selector % 2 == 1 && !strings.used.is_empty() {
        return strings.used[pick.selector % strings.used.len()];
    }
    u8::try_from(strings.count + pick.selector % 3).unwrap_or(u8::MAX)
}

fn automations_of(score: &Score, track: TrackId) -> usize {
    score
        .find_track(track)
        .map_or(0, |track| track.automations.len())
}

fn sounds_of(score: &Score, track: TrackId) -> usize {
    score
        .find_track(track)
        .map_or(0, |track| track.sounds.len())
}

fn automation(score: &Score, step: &Step, track: TrackId) -> Automation {
    let Step::InsertAutomation {
        bar,
        share,
        value,
        linear,
        visible,
        ..
    } = step
    else {
        unreachable!("only insert_automation builds an automation");
    };
    let masters: Vec<MasterBarId> = score.master_bars.iter().map(|master| master.id).collect();
    let (master_bar, length) = if bar.valid && !masters.is_empty() {
        let index = bar.selector % masters.len();
        (
            masters[index],
            score.master_bars[index].time_signature.bar_length(),
        )
    } else {
        (MasterBarId(ElementId(bar.raw)), Fraction::from_integer(1))
    };
    let value = match value {
        ValueSeed::Volume(level) => AutomationValue::Volume(Level::new(*level).expect("in range")),
        ValueSeed::Pan(balance) => AutomationValue::Pan(Balance::new(*balance).expect("in range")),
        ValueSeed::Sound(pick) => {
            let sounds = sounds_of(score, track);
            AutomationValue::Sound(SoundRef(existing_index(*pick, sounds)))
        }
    };
    Automation {
        master_bar,
        position: share * length,
        linear: *linear,
        visible: *visible,
        value,
    }
}

pub fn resolve(step: &Step, score: &Score) -> Command {
    let catalog = Catalog::of(score);
    let master_bars = score.master_bars.len();
    match step {
        Step::SetMeta { field, value } => Command::SetMeta {
            field: FIELDS[field % FIELDS.len()],
            value: value.clone(),
        },
        Step::InsertMasterBar {
            index,
            numerator,
            denominator,
        } => Command::InsertMasterBar {
            index: insert_index(*index, master_bars),
            time_signature: TimeSignature::new(*numerator, *denominator)
                .expect("generated time signature is valid"),
        },
        Step::RemoveMasterBar { index } => Command::RemoveMasterBar {
            index: existing_index(*index, master_bars),
        },
        Step::InsertBeat {
            voice,
            index,
            duration,
        } => {
            let voice = catalog.id(*voice, &catalog.voices);
            Command::InsertBeat {
                voice: Some(voice),
                bar: None,
                index: insert_index(*index, voice_len(score, voice)),
                duration: *duration,
            }
        }
        Step::InsertBeatInBar {
            bar,
            index,
            duration,
        } => {
            let pool = if bar.selector.is_multiple_of(2) && !catalog.voiceless.is_empty() {
                &catalog.voiceless
            } else {
                &catalog.bars
            };
            let target = catalog.id(*bar, pool);
            let voice = (!index.valid && index.selector.is_multiple_of(2))
                .then(|| catalog.id(*bar, &catalog.voices));
            Command::InsertBeat {
                voice,
                bar: Some(target),
                index: insert_index(*index, first_slot_len(score, target)),
                duration: *duration,
            }
        }
        Step::RemoveBeat { beat } => Command::RemoveBeat {
            beat: catalog.id(*beat, &catalog.beats),
        },
        Step::RemoveVoice { voice } => Command::RemoveVoice {
            voice: catalog.id(*voice, &catalog.voices),
        },
        Step::SetBeatDuration { beat, duration } => Command::SetBeatDuration {
            beat: catalog.id(*beat, &catalog.beats),
            duration: *duration,
        },
        Step::AddNote { beat, string, fret } => {
            let pool = if beat.selector.is_multiple_of(ALL_POOL_EVERY) {
                &catalog.beats
            } else {
                &catalog.open_beats
            };
            let beat = catalog.id(*beat, pool);
            Command::AddNote {
                beat,
                string: pick_string(*string, &strings_of(score, beat)),
                fret: *fret,
            }
        }
        Step::RemoveNote { note } => Command::RemoveNote {
            note: catalog.id(*note, &catalog.notes),
        },
        Step::SetFret { note, fret } => {
            let pool = if note.selector.is_multiple_of(ALL_POOL_EVERY) {
                &catalog.notes
            } else {
                &catalog.fretted_notes
            };
            Command::SetFret {
                note: catalog.id(*note, pool),
                fret: *fret,
            }
        }
        Step::SetAnacrusis { value } => Command::SetAnacrusis { value: *value },
        Step::SetTempo { master_bar, bpm } => Command::SetTempo {
            master_bar: existing_index(*master_bar, master_bars),
            bpm: *bpm,
        },
        Step::SetDirections {
            master_bar,
            directions,
        } => Command::SetDirections {
            master_bar: existing_index(*master_bar, master_bars),
            directions: directions.clone(),
        },
        Step::PadStaffBars { track, staff } => {
            let short: Vec<StaffInfo> = catalog
                .staves
                .iter()
                .copied()
                .filter(|info| info.bars < master_bars)
                .collect();
            let info = catalog.staff(*track, *staff, &short);
            Command::PadStaffBars {
                track: info.track,
                staff: info.staff,
            }
        }
        Step::TrimStaffBars { track, staff, len } => {
            let trimmable: Vec<StaffInfo> = catalog
                .staves
                .iter()
                .copied()
                .filter(|info| info.empty_tail > 0)
                .collect();
            let info = catalog.staff(*track, *staff, &trimmable);
            let len = if len.valid && info.empty_tail > 0 {
                info.bars - info.empty_tail + len.selector % info.empty_tail
            } else {
                info.bars + len.selector % 3
            };
            Command::TrimStaffBars {
                track: info.track,
                staff: info.staff,
                len,
            }
        }
        Step::SetTrackMidi { track, midi } => Command::SetTrackMidi {
            track: catalog.id(*track, &catalog.tracks),
            midi: *midi,
        },
        Step::InsertAutomation { track, index, .. } => {
            let track = catalog.id(*track, &catalog.tracks);
            Command::InsertAutomation {
                track,
                index: insert_index(*index, automations_of(score, track)),
                automation: automation(score, step, track),
            }
        }
        Step::RemoveAutomation { track, index } => {
            let track = catalog.id(*track, &catalog.tracks);
            Command::RemoveAutomation {
                track,
                index: existing_index(*index, automations_of(score, track)),
            }
        }
        Step::SetSoundOverride { track, bank } => Command::SetSoundOverride {
            track: catalog.id(*track, &catalog.tracks),
            bank: bank.clone(),
        },
    }
}

fn arb_track_midi() -> impl Strategy<Value = TrackMidi> {
    (
        (0u8..=3, 0u8..=15, 0u8..=15, any::<bool>()),
        (0u8..=127, 0u8..=127),
        (0u32..=1_000_000, 0u32..=1_000_000),
    )
        .prop_map(
            |((port, primary, secondary, per_string), (msb, lsb), (volume, pan))| TrackMidi {
                port,
                primary: MidiChannel::new(primary).expect("channel in range"),
                secondary: MidiChannel::new(secondary).expect("channel in range"),
                per_string,
                bank: MidiBank::new(msb, lsb).expect("bank in range"),
                volume: Level::new(volume).expect("volume in range"),
                pan: Balance::new(pan).expect("pan in range"),
            },
        )
}

fn arb_value_seed() -> impl Strategy<Value = ValueSeed> {
    prop_oneof![
        (0u32..=1_000_000).prop_map(ValueSeed::Volume),
        (0u32..=1_000_000).prop_map(ValueSeed::Pan),
        arb_pick().prop_map(ValueSeed::Sound),
    ]
}

fn arb_share() -> impl Strategy<Value = Fraction> {
    prop_oneof![
        4 => (0i64..=16, 1i64..=16).prop_map(|(numer, denom)| Fraction::new(numer % (denom + 1), denom)),
        1 => (17i64..=40, 1i64..=16).prop_map(|(numer, denom)| Fraction::new(numer, denom)),
    ]
}

fn arb_pick() -> impl Strategy<Value = Pick> {
    (
        prop::bool::weighted(VALID_RATE),
        any::<usize>(),
        any::<u64>(),
    )
        .prop_map(|(valid, selector, raw)| Pick {
            valid,
            selector,
            raw,
        })
}

fn arb_duration() -> impl Strategy<Value = Duration> {
    (
        select(NOTE_VALUES.to_vec()),
        0u8..=3,
        option::weighted(0.2, (1u8..=12, 1u8..=12)),
    )
        .prop_map(|(value, dots, tuplet)| {
            let tuplet =
                tuplet.map(|(num, den)| Tuplet::new(num, den).expect("generated tuplet is valid"));
            Duration::new(value, dots, tuplet).expect("generated duration is valid")
        })
}

fn arb_bpm() -> impl Strategy<Value = f64> {
    prop_oneof![
        4 => 1.0f64..=1000.0,
        1 => select(vec![0.0, 0.5, 1000.5, -3.0, 5000.0]),
    ]
}

fn arb_directions() -> impl Strategy<Value = Directions> {
    (
        vec(select(DIRECTION_TARGETS.to_vec()), 0..=2),
        vec(select(DIRECTION_JUMPS.to_vec()), 0..=2),
    )
        .prop_map(|(targets, jumps)| Directions { targets, jumps })
}

fn arb_step() -> impl Strategy<Value = Step> {
    prop_oneof![
        (0usize..10, ".{0,12}").prop_map(|(field, value)| Step::SetMeta { field, value }),
        (arb_pick(), 1u8..=12, select(vec![1u8, 2, 4, 8, 16, 32, 64])).prop_map(
            |(index, numerator, denominator)| Step::InsertMasterBar {
                index,
                numerator,
                denominator,
            }
        ),
        arb_pick().prop_map(|index| Step::RemoveMasterBar { index }),
        (arb_pick(), arb_pick(), arb_duration()).prop_map(|(voice, index, duration)| {
            Step::InsertBeat {
                voice,
                index,
                duration,
            }
        }),
        (arb_pick(), arb_pick(), arb_duration()).prop_map(|(bar, index, duration)| {
            Step::InsertBeatInBar {
                bar,
                index,
                duration,
            }
        }),
        arb_pick().prop_map(|beat| Step::RemoveBeat { beat }),
        arb_pick().prop_map(|voice| Step::RemoveVoice { voice }),
        (arb_pick(), arb_duration())
            .prop_map(|(beat, duration)| Step::SetBeatDuration { beat, duration }),
        (arb_pick(), arb_pick(), 0u8..=30).prop_map(|(beat, string, fret)| Step::AddNote {
            beat,
            string,
            fret
        }),
        arb_pick().prop_map(|note| Step::RemoveNote { note }),
        (arb_pick(), 0u8..=30).prop_map(|(note, fret)| Step::SetFret { note, fret }),
        any::<bool>().prop_map(|value| Step::SetAnacrusis { value }),
        (arb_pick(), arb_bpm()).prop_map(|(master_bar, bpm)| Step::SetTempo { master_bar, bpm }),
        (arb_pick(), arb_directions()).prop_map(|(master_bar, directions)| Step::SetDirections {
            master_bar,
            directions
        }),
        (arb_pick(), arb_pick()).prop_map(|(track, staff)| Step::PadStaffBars { track, staff }),
        (arb_pick(), arb_pick(), arb_pick()).prop_map(|(track, staff, len)| Step::TrimStaffBars {
            track,
            staff,
            len
        }),
        (arb_pick(), arb_track_midi()).prop_map(|(track, midi)| Step::SetTrackMidi { track, midi }),
        (
            (arb_pick(), arb_pick(), arb_pick()),
            arb_share(),
            arb_value_seed(),
            (any::<bool>(), any::<bool>()),
        )
            .prop_map(|((track, index, bar), share, value, (linear, visible))| {
                Step::InsertAutomation {
                    track,
                    index,
                    bar,
                    share,
                    value,
                    linear,
                    visible,
                }
            }),
        (arb_pick(), arb_pick()).prop_map(|(track, index)| Step::RemoveAutomation { track, index }),
        (arb_pick(), option::of(".{0,8}"))
            .prop_map(|(track, bank)| Step::SetSoundOverride { track, bank }),
    ]
}

#[derive(Debug, Clone, Copy)]
struct StaffSlot {
    track: usize,
    staff: usize,
}

fn perturb(score: &mut Score, selector: usize) {
    let staves: Vec<StaffSlot> = score
        .tracks
        .iter()
        .enumerate()
        .flat_map(|(track, t)| (0..t.staves.len()).map(move |staff| StaffSlot { track, staff }))
        .collect();
    for (index, slot) in staves.iter().enumerate() {
        if (selector >> ((index + CLEAR_SHIFT) % SELECTOR_BITS)) & 3 != 0
            && let Some(last) = score.tracks[slot.track].staves[slot.staff].bars.last_mut()
        {
            for voice in &mut last.voices {
                voice.beats.clear();
            }
        }
    }
    let candidates = match staves.split_first() {
        Some((_, rest)) if !rest.is_empty() => rest.to_vec(),
        _ => staves,
    };
    if candidates.is_empty() {
        return;
    }
    let forced = selector % candidates.len();
    for (index, slot) in candidates.iter().enumerate() {
        let chosen = index == forced || (selector >> (index % SELECTOR_BITS)) & 1 == 1;
        if !chosen {
            continue;
        }
        let bars = &mut score.tracks[slot.track].staves[slot.staff].bars;
        let dropped = 1 + (selector / PERTURB_SHIFT + index) % MAX_DROPPED_BARS;
        bars.truncate(bars.len().saturating_sub(dropped));
    }
}

fn beat_paths(score: &Score, kind: InstrumentKind) -> Vec<BeatPath> {
    let mut paths = Vec::new();
    for (track, t) in score.tracks.iter().enumerate() {
        if t.instrument.kind != kind {
            continue;
        }
        for (staff, s) in t.staves.iter().enumerate() {
            for (bar, b) in s.bars.iter().enumerate() {
                for (voice, v) in b.voices.iter().enumerate() {
                    for beat in 0..v.beats.len() {
                        paths.push(BeatPath {
                            track,
                            staff,
                            bar,
                            voice,
                            beat,
                        });
                    }
                }
            }
        }
    }
    paths
}

fn beat_at(score: &mut Score, path: BeatPath) -> &mut Beat {
    &mut score.tracks[path.track].staves[path.staff].bars[path.bar].voices[path.voice].beats
        [path.beat]
}

fn fretted_paths_with_notes(score: &Score) -> Vec<BeatPath> {
    beat_paths(score, InstrumentKind::Fretted)
        .into_iter()
        .filter(|path| {
            !score.tracks[path.track].staves[path.staff].bars[path.bar].voices[path.voice].beats
                [path.beat]
                .notes
                .is_empty()
        })
        .collect()
}

fn corrupt(score: &mut Score, corruption: Corruption, pick: usize) {
    match corruption {
        Corruption::TempoOutsideBar => {
            let len = score.master_bars.len();
            if let Some(master_bar) = score.master_bars.get_mut(pick % len.max(1)) {
                let position = master_bar.time_signature.bar_length();
                master_bar.tempo_changes.push(TempoChange {
                    position,
                    bpm: Tempo::new(90.0).expect("valid bpm"),
                    unit: TempoUnit::Quarter,
                });
            }
        }
        Corruption::DuplicateString => {
            let paths = fretted_paths_with_notes(score);
            if paths.is_empty() {
                return;
            }
            let Some(id) = score.alloc::<NoteId>() else {
                return;
            };
            let beat = beat_at(score, paths[pick % paths.len()]);
            let mut copy = beat.notes[0].clone();
            copy.id = id;
            beat.notes.push(copy);
        }
        Corruption::PercussionOnFretted => {
            let paths = beat_paths(score, InstrumentKind::Fretted);
            if paths.is_empty() {
                return;
            }
            let Some(id) = score.alloc::<NoteId>() else {
                return;
            };
            beat_at(score, paths[pick % paths.len()]).notes.push(Note {
                id,
                kind: NoteKind::Percussion { articulation: 38 },
                effects: NoteEffects::default(),
            });
        }
        Corruption::VoicelessBar => {
            let bars: Vec<StaffSlot> = score
                .tracks
                .iter()
                .enumerate()
                .flat_map(|(track, t)| {
                    (0..t.staves.len()).map(move |staff| StaffSlot { track, staff })
                })
                .filter(|slot| !score.tracks[slot.track].staves[slot.staff].bars.is_empty())
                .collect();
            if bars.is_empty() {
                return;
            }
            let slot = bars[pick % bars.len()];
            let staff = &mut score.tracks[slot.track].staves[slot.staff];
            let index = (pick / bars.len()) % staff.bars.len();
            staff.bars[index].voices.clear();
        }
        Corruption::DuplicateId => {
            let paths = fretted_paths_with_notes(score);
            if paths.len() < 2 {
                return;
            }
            let from = paths[pick % paths.len()];
            let to = paths[(pick + 1) % paths.len()];
            let id = beat_at(score, from).id;
            beat_at(score, to).id = id;
        }
        Corruption::IdAboveAllocator => {
            let paths = fretted_paths_with_notes(score);
            if paths.is_empty() {
                return;
            }
            let above = ElementId(score.ids.peek().0.saturating_add(3));
            beat_at(score, paths[pick % paths.len()]).notes[0].id = NoteId(above);
        }
        Corruption::FirstSoundBank => {
            let len = score.tracks.len();
            let Some(track) = score.tracks.get_mut(pick % len.max(1)) else {
                return;
            };
            let bank = track.midi.bank;
            let other = MidiBank::new((bank.msb() + 1) % 128, bank.lsb()).expect("bank in range");
            match track.sounds.first_mut() {
                Some(first) => first.bank = other,
                None => track.sounds.push(Sound {
                    name: "corrupt".to_owned(),
                    program: track.instrument.midi_program,
                    bank: other,
                }),
            }
        }
        Corruption::StringOutOfRange => {
            let paths = fretted_paths_with_notes(score);
            if paths.is_empty() {
                return;
            }
            let note = &mut beat_at(score, paths[pick % paths.len()]).notes[0];
            if let NoteKind::Fretted { fret, .. } = note.kind {
                note.kind = NoteKind::Fretted { string: 200, fret };
            }
        }
    }
}

fn arb_case_score() -> impl Strategy<Value = Score> {
    prop_oneof![
        1 => arb_score(),
        3 => arb_score().prop_filter("a score with tracks", |score| !score.tracks.is_empty()),
    ]
}

pub fn arb_case() -> impl Strategy<Value = Case> {
    (
        arb_case_score(),
        option::weighted(PERTURB_RATE, any::<usize>()),
        option::weighted(CORRUPT_RATE, (select(CORRUPTIONS.to_vec()), any::<usize>())),
        vec(arb_step(), 0..=MAX_STEPS),
    )
        .prop_map(|(mut score, perturbation, corruption, steps)| {
            if let Some(selector) = perturbation {
                perturb(&mut score, selector);
            }
            if let Some((kind, pick)) = corruption {
                corrupt(&mut score, kind, pick);
            }
            Case {
                score,
                corruption: corruption.map(|(kind, _)| kind),
                steps,
            }
        })
}
