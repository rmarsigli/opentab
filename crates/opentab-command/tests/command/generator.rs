use opentab_command::{Command, MetaField};
use opentab_model::{
    BeatId, Duration, ElementId, NoteId, NoteValue, Score, TimeSignature, Tuplet, VoiceId,
};
use opentab_testkit::arb_score;
use proptest::collection::vec;
use proptest::option;
use proptest::prelude::*;
use proptest::sample::select;

const MAX_STEPS: usize = 20;
const VALID_RATE: f64 = 0.8;
const PERTURB_RATE: f64 = 0.25;
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
}

#[derive(Debug, Clone)]
pub struct Case {
    pub score: Score,
    pub steps: Vec<Step>,
}

struct Catalog {
    all: Vec<ElementId>,
    voices: Vec<VoiceId>,
    beats: Vec<BeatId>,
    notes: Vec<NoteId>,
}

impl Catalog {
    fn of(score: &Score) -> Catalog {
        let mut catalog = Catalog {
            all: score.master_bars.iter().map(|m| m.id.0).collect(),
            voices: Vec::new(),
            beats: Vec::new(),
            notes: Vec::new(),
        };
        for track in &score.tracks {
            catalog.all.push(track.id.0);
            for staff in &track.staves {
                catalog.all.push(staff.id.0);
                for bar in &staff.bars {
                    catalog.all.push(bar.id.0);
                    for voice in &bar.voices {
                        catalog.all.push(voice.id.0);
                        catalog.voices.push(voice.id);
                        for beat in &voice.beats {
                            catalog.all.push(beat.id.0);
                            catalog.beats.push(beat.id);
                            for note in &beat.notes {
                                catalog.all.push(note.id.0);
                                catalog.notes.push(note.id);
                            }
                        }
                    }
                }
            }
        }
        catalog
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

fn string_count(score: &Score, beat: BeatId) -> usize {
    score
        .find_beat(beat)
        .map(|path| score.tracks[path.track].tuning.strings.len())
        .unwrap_or(0)
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
            time_signature: TimeSignature {
                numerator: *numerator,
                denominator: *denominator,
            },
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
            let beat = catalog.id(*beat, &catalog.beats);
            let strings = string_count(score, beat);
            let string = existing_index(*string, strings);
            Command::AddNote {
                beat,
                string: u8::try_from(string).unwrap_or(u8::MAX),
                fret: *fret,
            }
        }
        Step::RemoveNote { note } => Command::RemoveNote {
            note: catalog.id(*note, &catalog.notes),
        },
        Step::SetFret { note, fret } => Command::SetFret {
            note: catalog.id(*note, &catalog.notes),
            fret: *fret,
        },
        Step::SetTempo { master_bar, bpm } => Command::SetTempo {
            master_bar: existing_index(*master_bar, master_bars),
            bpm: *bpm,
        },
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
            let tuplet = tuplet.map(|(num, den)| Tuplet { num, den });
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
        (arb_pick(), 0u8..=12, select(vec![0u8, 1, 2, 4, 8, 16])).prop_map(
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
    ]
}

fn perturb(score: &mut Score, selector: usize) {
    let staves: Vec<(usize, usize)> = score
        .tracks
        .iter()
        .enumerate()
        .flat_map(|(track, t)| (0..t.staves.len()).map(move |staff| (track, staff)))
        .collect();
    if staves.len() < 2 {
        return;
    }
    let (track, staff) = staves[1 + selector % (staves.len() - 1)];
    score.tracks[track].staves[staff].bars.pop();
}

pub fn arb_case() -> impl Strategy<Value = Case> {
    (
        arb_score(),
        option::weighted(PERTURB_RATE, any::<usize>()),
        vec(arb_step(), 0..=MAX_STEPS),
    )
        .prop_map(|(mut score, perturbation, steps)| {
            if let Some(selector) = perturbation {
                perturb(&mut score, selector);
            }
            Case { score, steps }
        })
}
