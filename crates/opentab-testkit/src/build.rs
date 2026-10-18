use std::collections::HashSet;

use opentab_model::{
    Bar, Beat, BeatEffects, ContainerEntry, Duration, ElementId, Extensions, Instrument,
    InstrumentKind, Marker, MasterBar, Note, NoteKind, Score, Staff, TempoChange, TimeSignature,
    Track, Tuning, Voice, VoiceSlot,
};

use crate::fill::FillTable;
use crate::seed::{
    BarSeed, GraceSeed, MasterBarSeed, NoteSeed, ScoreSeed, StaffSeed, TempoChangeSeed, TrackSeed,
    VoiceSeed,
};

struct Builder {
    score: Score,
    allocated: Vec<ElementId>,
    table: &'static FillTable,
    beat_cap: usize,
}

impl Builder {
    fn id<T: From<ElementId>>(&mut self) -> T {
        let raw = self
            .score
            .ids
            .next()
            .expect("generated scores stay far below the id space");
        self.allocated.push(raw);
        T::from(raw)
    }

    fn master_bar(&mut self, seed: MasterBarSeed) -> MasterBar {
        MasterBar {
            id: self.id(),
            time_signature: seed.time_signature,
            key_signature: seed.key_signature,
            repeat_start: seed.repeat_start,
            repeat_end: seed.repeat_end,
            alternate_endings: seed.alternate_endings,
            double_bar: seed.double_bar,
            marker: seed.marker.map(|marker| Marker {
                text: marker.text,
                letter: marker.letter,
            }),
            tempo_changes: seed
                .tempo_changes
                .into_iter()
                .map(|change| tempo_change(change, seed.time_signature))
                .collect(),
        }
    }

    fn track(&mut self, seed: TrackSeed) -> Track {
        let id = self.id();
        let strings = match seed.kind {
            InstrumentKind::Fretted => seed.tuning.len(),
            InstrumentKind::Percussion => 0,
        };
        let tuning = Tuning {
            strings: seed.tuning.into_iter().take(strings).collect(),
        };
        let kind = seed.kind;
        let master_bars = self.score.master_bars.clone();
        let staves = seed
            .staves
            .into_iter()
            .map(|staff| self.staff(staff, kind, strings, &master_bars))
            .collect();
        Track {
            id,
            name: seed.name,
            short_name: seed.short_name,
            instrument: Instrument {
                kind,
                midi_program: seed.midi_program,
            },
            tuning,
            capo: seed.capo,
            transpose: seed.transpose,
            staves,
        }
    }

    fn staff(
        &mut self,
        seed: StaffSeed,
        kind: InstrumentKind,
        strings: usize,
        master_bars: &[MasterBar],
    ) -> Staff {
        let id = self.id();
        let bars = seed
            .bars
            .into_iter()
            .zip(master_bars)
            .enumerate()
            .map(|(index, (bar, master_bar))| {
                let pickup = index == 0 && self.score.anacrusis;
                self.bar(
                    bar,
                    master_bar,
                    Shape {
                        kind,
                        strings,
                        pickup,
                    },
                )
            })
            .collect();
        Staff { id, bars }
    }

    fn bar(&mut self, seed: BarSeed, master_bar: &MasterBar, shape: Shape) -> Bar {
        let id = self.id();
        let mut next_slot = 0u8;
        let mut voices = Vec::new();
        for voice in seed.voices {
            let slot = next_slot.saturating_add(voice.skip);
            let Ok(slot) = VoiceSlot::new(slot) else {
                continue;
            };
            next_slot = slot.get().saturating_add(1);
            voices.push(self.voice(voice, slot, master_bar, shape));
        }
        Bar {
            id,
            clef: seed.clef,
            voices,
        }
    }

    fn voice(
        &mut self,
        seed: VoiceSeed,
        slot: VoiceSlot,
        master_bar: &MasterBar,
        shape: Shape,
    ) -> Voice {
        let id = self.id();
        let picks: Vec<usize> = seed.beats.iter().map(|beat| beat.pick).collect();
        let mut durations = self
            .table
            .fill_bar(master_bar.time_signature, self.beat_cap, &picks);
        if shape.pickup && seed.pickup_cut && durations.len() >= 2 {
            durations.pop();
        }
        let mut beats = Vec::new();
        for (index, duration) in durations.into_iter().enumerate() {
            let beat_seed = seed.beats.get(index);
            if let Some(grace) = beat_seed.and_then(|beat| beat.grace.as_ref()) {
                let grace_beat = self.grace_beat(grace, shape);
                beats.push(grace_beat);
            }
            let beat_id = self.id();
            let notes = self.notes(beat_seed.map_or(&[], |beat| beat.notes.as_slice()), shape);
            beats.push(Beat {
                id: beat_id,
                duration,
                notes,
                effects: beat_seed
                    .map(|beat| beat.effects.clone())
                    .unwrap_or_default(),
            });
        }
        Voice { id, slot, beats }
    }

    fn grace_beat(&mut self, seed: &GraceSeed, shape: Shape) -> Beat {
        let id = self.id();
        let notes = self.notes(&seed.notes, shape);
        Beat {
            id,
            duration: Duration::new(seed.value, 0, None).expect("a plain duration is valid"),
            notes,
            effects: BeatEffects {
                grace: Some(seed.kind),
                ..BeatEffects::default()
            },
        }
    }

    fn notes(&mut self, seeds: &[NoteSeed], shape: Shape) -> Vec<Note> {
        let Shape { kind, strings, .. } = shape;
        let mut used: HashSet<usize> = HashSet::new();
        let mut notes = Vec::new();
        for note_seed in seeds {
            let note_kind = match kind {
                InstrumentKind::Fretted => {
                    let Some(string) = usize::from(note_seed.string).checked_rem(strings) else {
                        continue;
                    };
                    if !used.insert(string) {
                        continue;
                    }
                    let Ok(string) = u8::try_from(string) else {
                        continue;
                    };
                    NoteKind::Fretted {
                        string,
                        fret: note_seed.fret,
                    }
                }
                InstrumentKind::Percussion => NoteKind::Percussion {
                    articulation: note_seed.articulation,
                },
            };
            notes.push(Note {
                id: self.id(),
                kind: note_kind,
                effects: note_seed.effects.clone(),
            });
        }
        notes
    }
}

#[derive(Clone, Copy)]
struct Shape {
    kind: InstrumentKind,
    strings: usize,
    pickup: bool,
}

fn tempo_change(seed: TempoChangeSeed, time_signature: TimeSignature) -> TempoChange {
    TempoChange {
        position: seed.share * time_signature.bar_length(),
        bpm: seed.bpm,
        unit: seed.unit,
    }
}

pub fn build_score(seed: ScoreSeed, beat_cap: usize) -> Score {
    let mut builder = Builder {
        score: Score::new(),
        allocated: Vec::new(),
        table: FillTable::shared(),
        beat_cap,
    };
    builder.score.meta = seed.meta;
    builder.score.anacrusis = seed.anacrusis;
    for master_bar in seed.master_bars {
        let built = builder.master_bar(master_bar);
        builder.score.master_bars.push(built);
    }
    for track in seed.tracks {
        let built = builder.track(track);
        builder.score.tracks.push(built);
    }
    let mut extensions = Extensions::default();
    for extension in seed.extensions {
        let Some(index) = extension.target.checked_rem(builder.allocated.len()) else {
            continue;
        };
        let Some(target) = builder.allocated.get(index) else {
            continue;
        };
        extensions
            .0
            .entry(*target)
            .or_default()
            .extend(extension.payloads);
    }
    builder.score.extensions = extensions;
    builder.score.document_extensions = seed.document_extensions;
    builder.score.container = seed.container;
    if let Some(entry) = seed.score_entry {
        let len = builder.score.container.len();
        let position = entry.position.checked_rem(len + 1).unwrap_or(0);
        builder
            .score
            .container
            .insert(position, ContainerEntry::Score { name: entry.name });
    }
    builder.score
}
