use opentab_model::{BeatId, ElementId, IdAllocator, NoteId, Score, TrackId};

#[test]
fn allocator_is_monotonic_from_one() {
    let mut ids = IdAllocator::default();
    assert_eq!(ids.peek(), ElementId(1));
    assert_eq!(ids.next(), ElementId(1));
    assert_eq!(ids.next(), ElementId(2));
    assert_eq!(ids.peek(), ElementId(3));
}

#[test]
fn score_alloc_returns_typed_ids() {
    let mut score = Score::new();
    let track: TrackId = score.alloc();
    let beat: BeatId = score.alloc();
    assert_eq!(track, TrackId(ElementId(1)));
    assert_eq!(beat, BeatId(ElementId(2)));
    assert_eq!(ElementId::from(beat), ElementId(2));
}

#[test]
fn typed_ids_serialise_as_plain_integers() {
    assert_eq!(serde_json::to_string(&NoteId(ElementId(42))).unwrap(), "42");
    let parsed: TrackId = serde_json::from_str("7").unwrap();
    assert_eq!(parsed, TrackId(ElementId(7)));
}

#[test]
fn allocator_saturates_instead_of_overflowing() {
    let mut ids: IdAllocator = serde_json::from_str(r#"{"next": 18446744073709551615}"#).unwrap();
    assert_eq!(ids.next(), ElementId(u64::MAX));
    assert_eq!(ids.peek(), ElementId(u64::MAX));
}
