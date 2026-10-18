use opentab_model::{Note, NoteEffects, NoteId, NoteKind, Pitch, Score};

use crate::support::{guitar, quarters};

fn fretted(string: u8, fret: u8) -> Note {
    Note {
        id: NoteId::from(opentab_model::ElementId(1)),
        kind: NoteKind::Fretted { string, fret },
        effects: NoteEffects::default(),
    }
}

#[test]
fn standard_tuning_with_capo_two() {
    let mut score = Score::new();
    let mut track = guitar(&mut score, vec![quarters(4)]);
    track.capo = 2;
    let expected = [66, 61, 57, 52, 47, 42];
    for (string, open) in expected.iter().enumerate() {
        let string = u8::try_from(string).unwrap();
        assert_eq!(track.pitch_of(&fretted(string, 0)), Some(Pitch(*open)));
        assert_eq!(track.pitch_of(&fretted(string, 5)), Some(Pitch(open + 5)));
    }
    assert_eq!(track.pitch_of(&fretted(5, 3)), Some(Pitch(45)));
}

#[test]
fn out_of_range_string_and_percussion_are_none() {
    let mut score = Score::new();
    let track = guitar(&mut score, vec![quarters(4)]);
    assert_eq!(track.pitch_of(&fretted(6, 0)), None);
    assert_eq!(track.pitch_of(&fretted(255, 0)), None);
    let drum = Note {
        id: NoteId::from(opentab_model::ElementId(1)),
        kind: NoteKind::Percussion { articulation: 38 },
        effects: NoteEffects::default(),
    };
    assert_eq!(track.pitch_of(&drum), None);
}

#[test]
fn pitch_above_midi_range_is_none() {
    let mut score = Score::new();
    let mut track = guitar(&mut score, vec![quarters(4)]);
    track.capo = 255;
    assert_eq!(track.pitch_of(&fretted(0, 255)), None);
    track.capo = 0;
    assert_eq!(track.pitch_of(&fretted(0, 63)), Some(Pitch(127)));
    assert_eq!(track.pitch_of(&fretted(0, 64)), None);
}
