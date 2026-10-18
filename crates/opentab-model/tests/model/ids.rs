use opentab_model::{BeatId, ElementId, IdAllocator, NoteId, Score, TrackId};

#[test]
fn allocator_is_monotonic_from_one() {
    let mut ids = IdAllocator::default();
    assert_eq!(ids.peek(), ElementId(1));
    assert_eq!(ids.next(), Some(ElementId(1)));
    assert_eq!(ids.next(), Some(ElementId(2)));
    assert_eq!(ids.peek(), ElementId(3));
}

#[test]
fn score_alloc_returns_typed_ids() {
    let mut score = Score::new();
    let track: TrackId = score.alloc().unwrap();
    let beat: BeatId = score.alloc().unwrap();
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
fn exhausted_allocator_returns_none_and_stays_put() {
    let mut ids: IdAllocator = serde_json::from_str(r#"{"next": 18446744073709551615}"#).unwrap();
    assert_eq!(ids.next(), None);
    assert_eq!(ids.next(), None);
    assert_eq!(ids.peek(), ElementId(u64::MAX));
}

#[test]
fn last_id_before_exhaustion_is_handed_out_once() {
    let mut ids: IdAllocator = serde_json::from_str(r#"{"next": 18446744073709551614}"#).unwrap();
    assert_eq!(ids.next(), Some(ElementId(u64::MAX - 1)));
    assert_eq!(ids.next(), None);
}

#[test]
fn score_alloc_returns_none_when_exhausted() {
    let mut score = Score::new();
    score.ids = serde_json::from_str(r#"{"next": 18446744073709551615}"#).unwrap();
    assert_eq!(score.alloc::<TrackId>(), None);
    assert_eq!(score.alloc::<ElementId>(), None);
    assert_eq!(score.ids.peek(), ElementId(u64::MAX));
}
