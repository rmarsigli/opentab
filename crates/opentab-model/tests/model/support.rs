use opentab_model::{
    Bar, Beat, BeatEffects, Clef, Duration, Instrument, InstrumentKind, KeySignature, MasterBar,
    Note, NoteEffects, NoteKind, NoteValue, Pitch, Score, Staff, TimeSignature, Track, TrackMidi,
    Transpose, Tuning, Voice, VoiceSlot,
};

pub fn standard_tuning() -> Tuning {
    Tuning {
        strings: [64, 59, 55, 50, 45, 40].map(Pitch).to_vec(),
    }
}

pub fn duration(value: NoteValue) -> Duration {
    Duration::new(value, 0, None).unwrap()
}

pub fn master_bar(score: &mut Score, numerator: u8, denominator: u8) -> MasterBar {
    MasterBar {
        id: score.alloc().unwrap(),
        time_signature: TimeSignature::new(numerator, denominator).unwrap(),
        key_signature: KeySignature::new(0, false).unwrap(),
        repeat_start: false,
        repeat_end: None,
        alternate_endings: 0,
        double_bar: false,
        marker: None,
        tempo_changes: Vec::new(),
    }
}

pub fn note(score: &mut Score, string: u8, fret: u8) -> Note {
    Note {
        id: score.alloc().unwrap(),
        kind: NoteKind::Fretted { string, fret },
        effects: NoteEffects::default(),
    }
}

pub fn beat(score: &mut Score, duration: Duration, notes: Vec<Note>) -> Beat {
    Beat {
        id: score.alloc().unwrap(),
        duration,
        notes,
        effects: BeatEffects::default(),
    }
}

pub fn voice(score: &mut Score, durations: &[Duration]) -> Voice {
    let id = score.alloc().unwrap();
    let mut beats = Vec::new();
    for duration in durations {
        let notes = vec![note(score, 0, 0)];
        beats.push(beat(score, *duration, notes));
    }
    Voice {
        id,
        slot: VoiceSlot::default(),
        beats,
    }
}

pub fn bar(score: &mut Score, durations: &[Duration]) -> Bar {
    let id = score.alloc().unwrap();
    let voices = vec![voice(score, durations)];
    Bar {
        id,
        clef: Clef::Treble,
        voices,
    }
}

pub fn guitar(score: &mut Score, bars: Vec<Vec<Duration>>) -> Track {
    let id = score.alloc().unwrap();
    let staff_id = score.alloc().unwrap();
    let mut staff_bars = Vec::new();
    for durations in bars {
        staff_bars.push(bar(score, &durations));
    }
    Track {
        id,
        name: "Guitar".to_string(),
        short_name: "Gtr".to_string(),
        instrument: Instrument {
            kind: InstrumentKind::Fretted,
            midi_program: 25,
            set: None,
        },
        midi: TrackMidi::default_for(InstrumentKind::Fretted),
        sounds: Vec::new(),
        automations: Vec::new(),
        tuning: standard_tuning(),
        capo: 0,
        transpose: Transpose::default(),
        staves: vec![Staff {
            id: staff_id,
            bars: staff_bars,
        }],
    }
}

pub fn four_four(durations: &[Duration]) -> Score {
    let mut score = Score::new();
    let master = master_bar(&mut score, 4, 4);
    score.master_bars.push(master);
    let track = guitar(&mut score, vec![durations.to_vec()]);
    score.tracks.push(track);
    score
}

pub fn quarters(count: usize) -> Vec<Duration> {
    vec![duration(NoteValue::Quarter); count]
}
