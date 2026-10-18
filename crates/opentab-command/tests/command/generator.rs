use opentab_command::{Command, MetaField};
use opentab_model::{
    BeatId, Duration, ElementId, InstrumentKind, NoteId, NoteKind, NoteValue, Score, StaffId,
    TimeSignature, TrackId, Tuplet, VoiceId,
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
    RemoveBeat {
        beat: Pick,
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
    PadStaffBars {
        track: Pick,
        staff: Pick,
    },
    TrimStaffBars {
        track: Pick,
        staff: Pick,
        len: Pick,
    },
}

#[derive(Debug, Clone)]
pub struct Case {
    pub score: Score,
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
                voice,
                index: insert_index(*index, voice_len(score, voice)),
                duration: *duration,
            }
        }
        Step::RemoveBeat { beat } => Command::RemoveBeat {
            beat: catalog.id(*beat, &catalog.beats),
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
        Step::SetTempo { master_bar, bpm } => Command::SetTempo {
            master_bar: existing_index(*master_bar, master_bars),
            bpm: *bpm,
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
    }
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
        arb_pick().prop_map(|beat| Step::RemoveBeat { beat }),
        (arb_pick(), arb_duration())
            .prop_map(|(beat, duration)| Step::SetBeatDuration { beat, duration }),
        (arb_pick(), arb_pick(), 0u8..=30).prop_map(|(beat, string, fret)| Step::AddNote {
            beat,
            string,
            fret
        }),
        arb_pick().prop_map(|note| Step::RemoveNote { note }),
        (arb_pick(), 0u8..=30).prop_map(|(note, fret)| Step::SetFret { note, fret }),
        (arb_pick(), arb_bpm()).prop_map(|(master_bar, bpm)| Step::SetTempo { master_bar, bpm }),
        (arb_pick(), arb_pick()).prop_map(|(track, staff)| Step::PadStaffBars { track, staff }),
        (arb_pick(), arb_pick(), arb_pick()).prop_map(|(track, staff, len)| Step::TrimStaffBars {
            track,
            staff,
            len
        }),
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
        if (selector >> ((index + CLEAR_SHIFT) % SELECTOR_BITS)) & 1 == 1
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

fn keep_tempo_changes_inside_their_bars(score: &mut Score) {
    for master_bar in &mut score.master_bars {
        let length = master_bar.time_signature.bar_length();
        for change in &mut master_bar.tempo_changes {
            change.position %= length;
        }
    }
}

pub fn arb_case() -> impl Strategy<Value = Case> {
    (
        arb_score(),
        option::weighted(PERTURB_RATE, any::<usize>()),
        vec(arb_step(), 0..=MAX_STEPS),
    )
        .prop_map(|(mut score, perturbation, steps)| {
            keep_tempo_changes_inside_their_bars(&mut score);
            if let Some(selector) = perturbation {
                perturb(&mut score, selector);
            }
            Case { score, steps }
        })
}
