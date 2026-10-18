use std::collections::HashSet;

use opentab_model::{
    Bar, Beat, ElementId, Extensions, Instrument, InstrumentKind, MasterBar, Note, NoteKind, Score,
    Staff, TempoChange, TimeSignature, Track, Tuning, Voice,
};

use crate::fill::FillTable;
use crate::seed::{
    BarSeed, BeatSeed, MasterBarSeed, ScoreSeed, StaffSeed, TempoChangeSeed, TrackSeed, VoiceSeed,
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
            marker: seed.marker.map(|text| opentab_model::Marker { text }),
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
            .map(|(bar, master_bar)| self.bar(bar, master_bar, kind, strings))
            .collect();
        Staff { id, bars }
    }

    fn bar(
        &mut self,
        seed: BarSeed,
        master_bar: &MasterBar,
        kind: InstrumentKind,
        strings: usize,
    ) -> Bar {
        let id = self.id();
        let voices = seed
            .voices
            .into_iter()
            .map(|voice| self.voice(voice, master_bar, kind, strings))
            .collect();
        Bar { id, voices }
    }

    fn voice(
        &mut self,
        seed: VoiceSeed,
        master_bar: &MasterBar,
        kind: InstrumentKind,
        strings: usize,
    ) -> Voice {
        let id = self.id();
        let picks: Vec<usize> = seed.beats.iter().map(|beat| beat.pick).collect();
        let durations = self
            .table
            .fill_bar(master_bar.time_signature, self.beat_cap, &picks);
        let beats = durations
            .into_iter()
            .enumerate()
            .map(|(index, duration)| {
                let beat_seed = seed.beats.get(index);
                let beat_id = self.id();
                Beat {
                    id: beat_id,
                    duration,
                    notes: self.notes(beat_seed, kind, strings),
                    effects: beat_seed
                        .map(|beat| beat.effects.clone())
                        .unwrap_or_default(),
                }
            })
            .collect();
        Voice { id, beats }
    }

    fn notes(
        &mut self,
        seed: Option<&BeatSeed>,
        kind: InstrumentKind,
        strings: usize,
    ) -> Vec<Note> {
        let Some(seed) = seed else {
            return Vec::new();
        };
        let mut used: HashSet<usize> = HashSet::new();
        let mut notes = Vec::new();
        for note_seed in &seed.notes {
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

fn tempo_change(seed: TempoChangeSeed, time_signature: TimeSignature) -> TempoChange {
    TempoChange {
        position: seed.share * time_signature.bar_length(),
        bpm: seed.bpm,
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
    builder.score
}
