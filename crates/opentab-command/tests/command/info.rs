use opentab_command::{Command, IdTree, Info, TimeSignatureCount, apply, element_ids};
use opentab_model::{NoteValue, Pitch, TimeSignature};

use crate::support::{fixture, score_ids};

#[test]
fn info_summarises_meta_tracks_signatures_and_ids() {
    let mut fixture = fixture();
    fixture.score.meta.artist = "Someone".to_owned();
    fixture.score.tracks[1].name = "Drums".to_owned();
    fixture.score.tracks[0].capo = 2;
    fixture.score.master_bars[1].time_signature = TimeSignature::new(6, 8).unwrap();
    let info = Info::of(&fixture.score);
    assert_eq!(info.title, "Original");
    assert_eq!(info.artist, "Someone");
    assert_eq!(info.master_bars, 2);
    assert_eq!(
        info.time_signatures,
        vec![
            TimeSignatureCount("4/4".to_owned(), 1),
            TimeSignatureCount("6/8".to_owned(), 1)
        ]
    );
    assert_eq!(info.tracks.len(), 2);
    assert_eq!(info.tracks[0].id, fixture.guitar.0.0);
    assert_eq!(info.tracks[0].kind, "fretted");
    assert_eq!(info.tracks[0].capo, 2);
    assert_eq!(
        info.tracks[0].strings,
        [64, 59, 55, 50, 45, 40].map(Pitch).to_vec()
    );
    assert_eq!(info.tracks[1].id, fixture.drums.0.0);
    assert_eq!(info.tracks[1].kind, "percussion");
    assert_eq!(info.tracks[1].name, "Drums");
    assert!(info.tracks[1].strings.is_empty());
    assert_eq!(info.ids, IdTree::of(&fixture.score));
}

#[test]
fn repeated_signatures_are_counted_in_order_of_appearance() {
    let mut fixture = fixture();
    fixture.score.master_bars[0].time_signature = TimeSignature::new(3, 4).unwrap();
    let mut third = fixture.score.master_bars[1].clone();
    third.time_signature = TimeSignature::new(3, 4).unwrap();
    fixture.score.master_bars.push(third);
    let info = Info::of(&fixture.score);
    assert_eq!(
        info.time_signatures,
        vec![
            TimeSignatureCount("3/4".to_owned(), 2),
            TimeSignatureCount("4/4".to_owned(), 1)
        ]
    );
}

#[test]
fn the_id_tree_holds_every_element_id_in_document_order() {
    let fixture = fixture();
    let tree = IdTree::of(&fixture.score);
    let mut flat: Vec<u64> = tree.master_bars.clone();
    for track in &tree.tracks {
        flat.push(track.id);
        for staff in &track.staves {
            flat.push(staff.id);
            for bar in &staff.bars {
                flat.push(bar.id);
                for voice in &bar.voices {
                    flat.push(voice.id);
                    for beat in &voice.beats {
                        flat.push(beat.id);
                        flat.extend(beat.notes.iter().copied());
                    }
                }
            }
        }
    }
    let expected: Vec<u64> = score_ids(&fixture.score).iter().map(|id| id.0).collect();
    assert_eq!(flat, expected);
    let mut sorted = expected.clone();
    sorted.sort_unstable();
    assert_eq!(
        element_ids(&fixture.score)
            .into_iter()
            .collect::<Vec<u64>>(),
        sorted
    );
}

#[test]
fn element_ids_grow_by_exactly_the_ids_a_command_allocates() {
    let mut fixture = fixture();
    let before = element_ids(&fixture.score);
    apply(
        &mut fixture.score,
        &Command::InsertBeat {
            voice: Some(fixture.voice),
            bar: None,
            index: 0,
            duration: crate::support::duration(NoteValue::Quarter),
        },
    )
    .unwrap();
    let after = element_ids(&fixture.score);
    let added: Vec<u64> = after.difference(&before).copied().collect();
    assert_eq!(added.len(), 1);
    let first = &fixture.score.tracks[0].staves[0].bars[0].voices[0].beats[0];
    assert_eq!(added, vec![first.id.0.0]);
    assert!(before.is_subset(&after));
}
