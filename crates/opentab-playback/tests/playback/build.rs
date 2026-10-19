use opentab_model::{
    Bar, BarId, Beat, BeatEffects, BeatId, Clef, Duration, ElementId, Instrument, InstrumentKind,
    Note, NoteEffects, NoteId, NoteKind, NoteValue, Pitch, Score, Staff, StaffId, Track, TrackId,
    TrackMidi, Transpose, Tuning, Voice, VoiceId, VoiceSlot,
};

use crate::support::plain_score;

pub const STANDARD: [u8; 6] = [64, 59, 55, 50, 45, 40];

pub struct Builder {
    next: u64,
    pub score: Score,
}

#[derive(Clone)]
pub struct NoteSpec {
    pub kind: NoteKind,
    pub effects: NoteEffects,
}

#[derive(Clone)]
pub struct BeatSpec {
    pub value: NoteValue,
    pub notes: Vec<NoteSpec>,
    pub effects: BeatEffects,
}

pub fn fretted(string: u8, fret: u8) -> NoteSpec {
    NoteSpec {
        kind: NoteKind::Fretted { string, fret },
        effects: NoteEffects::default(),
    }
}

pub fn hit(articulation: u16) -> NoteSpec {
    NoteSpec {
        kind: NoteKind::Percussion { articulation },
        effects: NoteEffects::default(),
    }
}

pub fn with(note: NoteSpec, edit: impl FnOnce(&mut NoteEffects)) -> NoteSpec {
    let mut note = note;
    edit(&mut note.effects);
    note
}

pub fn beat(value: NoteValue, notes: Vec<NoteSpec>) -> BeatSpec {
    BeatSpec {
        value,
        notes,
        effects: BeatEffects::default(),
    }
}

pub fn beat_with(spec: BeatSpec, edit: impl FnOnce(&mut BeatEffects)) -> BeatSpec {
    let mut spec = spec;
    edit(&mut spec.effects);
    spec
}

pub fn rest(value: NoteValue) -> BeatSpec {
    beat(value, Vec::new())
}

impl Builder {
    pub fn new(bars: usize) -> Builder {
        Builder {
            next: 1000,
            score: plain_score(bars),
        }
    }

    fn id(&mut self) -> ElementId {
        self.next += 1;
        ElementId(self.next)
    }

    pub fn track(&mut self, kind: InstrumentKind, tuning: &[u8], capo: u8) -> usize {
        let id = TrackId(self.id());
        let staff = StaffId(self.id());
        let bars: Vec<Bar> = (0..self.score.master_bars.len())
            .map(|_| Bar {
                id: BarId(self.id()),
                clef: Clef::Treble,
                voices: Vec::new(),
            })
            .collect();
        self.score.tracks.push(Track {
            id,
            name: format!("Track {}", self.score.tracks.len()),
            short_name: String::new(),
            instrument: Instrument {
                kind,
                midi_program: 27,
                set: None,
            },
            drum_kit: None,
            midi: TrackMidi::default_for(kind),
            sounds: Vec::new(),
            tuning: Tuning {
                strings: tuning.iter().map(|pitch| Pitch(*pitch)).collect(),
            },
            capo,
            transpose: Transpose::default(),
            staves: vec![Staff { id: staff, bars }],
            automations: Vec::new(),
        });
        self.score.tracks.len() - 1
    }

    pub fn guitar(&mut self) -> usize {
        self.track(InstrumentKind::Fretted, &STANDARD, 0)
    }

    pub fn voice(&mut self, track: usize, bar: usize, slot: u8, beats: Vec<BeatSpec>) {
        let voice_id = VoiceId(self.id());
        let mut built = Vec::new();
        for spec in beats {
            let beat_id = BeatId(self.id());
            let notes = spec
                .notes
                .into_iter()
                .map(|note| Note {
                    id: NoteId(self.id()),
                    kind: note.kind,
                    effects: note.effects,
                })
                .collect();
            built.push(Beat {
                id: beat_id,
                duration: Duration::new(spec.value, 0, None).expect("duration"),
                notes,
                effects: spec.effects,
            });
        }
        let voice = Voice {
            id: voice_id,
            slot: VoiceSlot::new(slot).expect("slot"),
            beats: built,
        };
        self.score.tracks[track].staves[0].bars[bar]
            .voices
            .push(voice);
    }
}
