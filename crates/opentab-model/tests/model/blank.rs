use opentab_model::{InstrumentKind, blank_score, diagnose, from_json, to_json};

#[test]
fn the_blank_score_is_one_bar_of_guitar_rest_at_120() {
    let score = blank_score().expect("blank");
    assert_eq!(diagnose(&score), Vec::new());
    assert_eq!(score.master_bars.len(), 1);
    assert_eq!(score.master_bars[0].tempo_changes[0].bpm.bpm(), 120.0);
    let track = &score.tracks[0];
    assert_eq!(track.instrument.kind, InstrumentKind::Fretted);
    assert_eq!(track.instrument.midi_program, 25);
    assert_eq!(track.transpose.octave(), -1);
    assert_eq!(track.staves[0].bars[0].voices[0].beats.len(), 1);
    assert!(track.staves[0].bars[0].voices[0].beats[0].notes.is_empty());
    assert_eq!(
        from_json(&to_json(&score).expect("json")).expect("parse"),
        score
    );
    assert_eq!(blank_score().expect("again"), score);
}
