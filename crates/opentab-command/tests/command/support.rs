use opentab_model::{
    Bar, Beat, BeatEffects, BeatId, Duration, ElementId, Instrument, InstrumentKind, KeySignature,
    MasterBar, Note, NoteEffects, NoteId, NoteKind, NoteValue, OpaquePayload, Pitch, Score, Staff,
    Tempo, TempoChange, TimeSignature, Track, TrackId, Tuning, Voice, VoiceId,
};

pub struct Fixture {
    pub score: Score,
    pub guitar: TrackId,
    pub drums: TrackId,
    pub voice: VoiceId,
    pub beats: Vec<BeatId>,
    pub notes: Vec<NoteId>,
    pub drum_beat: BeatId,
    pub drum_note: NoteId,
}

pub enum NoteSpec {
    Fretted(u8, u8),
    Drum(u16),
}

pub fn duration(value: NoteValue) -> Duration {
    Duration::new(value, 0, None).unwrap()
}

pub fn four_four() -> TimeSignature {
    TimeSignature::new(4, 4).unwrap()
}

pub fn payload(data: &str) -> Vec<OpaquePayload> {
    vec![OpaquePayload {
        format: "gp".to_owned(),
        data: data.to_owned(),
    }]
}

pub fn master_bar(score: &mut Score, bpm: Option<f64>) -> MasterBar {
    MasterBar {
        id: score.alloc().unwrap(),
        time_signature: four_four(),
        key_signature: KeySignature::new(2, false).unwrap(),
        repeat_start: false,
        repeat_end: None,
        alternate_endings: 0,
        marker: None,
        tempo_changes: bpm
            .map(|bpm| TempoChange {
                position: opentab_model::Fraction::from_integer(0),
                bpm: Tempo::new(bpm).unwrap(),
            })
            .into_iter()
            .collect(),
    }
}

pub fn beat(score: &mut Score, value: NoteValue, notes: &[NoteSpec]) -> Beat {
    let id = score.alloc().unwrap();
    let notes = notes
        .iter()
        .map(|spec| Note {
            id: score.alloc().unwrap(),
            kind: match spec {
                NoteSpec::Fretted(string, fret) => NoteKind::Fretted {
                    string: *string,
                    fret: *fret,
                },
                NoteSpec::Drum(articulation) => NoteKind::Percussion {
                    articulation: *articulation,
                },
            },
            effects: NoteEffects::default(),
        })
        .collect();
    Beat {
        id,
        duration: duration(value),
        notes,
        effects: BeatEffects::default(),
    }
}

pub fn bar(score: &mut Score, beats: Vec<Beat>) -> Bar {
    let id = score.alloc().unwrap();
    let voice = Voice {
        id: score.alloc().unwrap(),
        beats,
    };
    Bar {
        id,
        voices: vec![voice],
    }
}

pub fn track(score: &mut Score, kind: InstrumentKind, bars: Vec<Bar>) -> Track {
    let strings = match kind {
        InstrumentKind::Fretted => [64, 59, 55, 50, 45, 40].map(Pitch).to_vec(),
        InstrumentKind::Percussion => Vec::new(),
    };
    Track {
        id: score.alloc().unwrap(),
        name: "track".to_owned(),
        short_name: "t".to_owned(),
        instrument: Instrument {
            kind,
            midi_program: 0,
        },
        tuning: Tuning { strings },
        capo: 0,
        staves: vec![Staff {
            id: score.alloc().unwrap(),
            bars,
        }],
    }
}

pub fn fixture() -> Fixture {
    let mut score = Score::new();
    score.meta.title = "Original".to_owned();
    let first = master_bar(&mut score, None);
    let second = master_bar(&mut score, Some(120.0));
    let second_id = second.id;
    score.master_bars = vec![first, second];
    let guitar_beats = vec![
        beat(
            &mut score,
            NoteValue::Quarter,
            &[NoteSpec::Fretted(0, 3), NoteSpec::Fretted(2, 5)],
        ),
        beat(&mut score, NoteValue::Quarter, &[]),
        beat(&mut score, NoteValue::Quarter, &[NoteSpec::Fretted(1, 0)]),
        beat(&mut score, NoteValue::Quarter, &[NoteSpec::Fretted(4, 7)]),
    ];
    let beats: Vec<BeatId> = guitar_beats.iter().map(|beat| beat.id).collect();
    let notes: Vec<NoteId> = guitar_beats[0].notes.iter().map(|note| note.id).collect();
    let guitar_first = bar(&mut score, guitar_beats);
    let voice = guitar_first.voices[0].id;
    let whole = beat(&mut score, NoteValue::Whole, &[NoteSpec::Fretted(5, 12)]);
    let guitar_second = bar(&mut score, vec![whole]);
    let guitar = track(
        &mut score,
        InstrumentKind::Fretted,
        vec![guitar_first, guitar_second],
    );
    let drum_hit = beat(&mut score, NoteValue::Whole, &[NoteSpec::Drum(38)]);
    let drum_beat = drum_hit.id;
    let drum_note = drum_hit.notes[0].id;
    let drums_first = bar(&mut score, vec![drum_hit]);
    let rest = beat(&mut score, NoteValue::Whole, &[]);
    let drums_second = bar(&mut score, vec![rest]);
    let drums = track(
        &mut score,
        InstrumentKind::Percussion,
        vec![drums_first, drums_second],
    );
    let guitar_id = guitar.id;
    let drums_id = drums.id;
    score.tracks = vec![guitar, drums];
    score
        .extensions
        .0
        .insert(ElementId::from(beats[2]), payload("beat"));
    score
        .extensions
        .0
        .insert(ElementId::from(notes[1]), payload("note"));
    score
        .extensions
        .0
        .insert(ElementId::from(second_id), payload("master"));
    Fixture {
        score,
        guitar: guitar_id,
        drums: drums_id,
        voice,
        beats,
        notes,
        drum_beat,
        drum_note,
    }
}

pub fn guitar_voice(score: &Score, bar: usize) -> &Voice {
    &score.tracks[0].staves[0].bars[bar].voices[0]
}
