use std::collections::HashSet;

use opentab_model::{Diagnostic, diagnose};

use crate::support::{Doc, payloads, read_gpif};

fn shared_document() -> String {
    Doc::default()
        .render()
        .replace("<NoteValue>Whole</NoteValue>", "<NoteValue>Quarter</NoteValue>")
        .replace(
            "<Voice id=\"0\"><Beats>0</Beats></Voice>",
            "<Voice id=\"0\"><Beats>0 0 1 1</Beats></Voice>",
        )
        .replace(
            "<Beat id=\"0\"><Rhythm ref=\"0\"/><Notes>0</Notes></Beat>",
            "<Beat id=\"0\"><Rhythm ref=\"0\"/><Wah>Open</Wah><Notes>0 1</Notes></Beat><Beat id=\"1\"><Rhythm ref=\"0\"/><Notes>1</Notes></Beat>",
        )
        .replace(
            "</Note></Notes>",
            "</Note><Note id=\"1\"><Properties><Property name=\"String\"><String>1</String></Property><Property name=\"Fret\"><Fret>5</Fret></Property><Property name=\"HopoDestination\"><Enable/></Property></Properties></Note></Notes>",
        )
}

#[test]
fn shared_beats_and_notes_become_separate_elements_with_their_payloads() {
    let score = read_gpif(&shared_document());
    let voice = &score.tracks[0].staves[0].bars[0].voices[0];
    assert_eq!(voice.beats.len(), 4);
    let notes: Vec<_> = voice.beats.iter().flat_map(|beat| &beat.notes).collect();
    assert_eq!(notes.len(), 6);
    let mut ids = HashSet::new();
    for beat in &voice.beats {
        assert!(ids.insert(beat.id.0));
        for note in &beat.notes {
            assert!(ids.insert(note.id.0));
        }
    }
    for beat in &voice.beats[..2] {
        assert_eq!(
            payloads(&score, beat.id.0),
            vec!["<Keep parent=\"\" index=\"1\"><Wah>Open</Wah></Keep>".to_string()]
        );
    }
    assert!(payloads(&score, voice.beats[2].id.0).is_empty());
    let copies = notes
        .iter()
        .filter(|note| {
            payloads(&score, note.id.0)
                .iter()
                .any(|payload| payload.contains("HopoDestination"))
        })
        .count();
    assert_eq!(copies, 4);
    let diagnostics = diagnose(&score);
    assert!(
        diagnostics.iter().all(|diagnostic| !matches!(
            diagnostic,
            Diagnostic::DuplicateId { .. } | Diagnostic::IdAboveAllocator { .. }
        )),
        "{diagnostics:?}"
    );
}
