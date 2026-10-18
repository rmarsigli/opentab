use opentab_command::{Command, CommandError, History, MetaField};
use opentab_model::{BeatId, ElementId, NoteValue};

use crate::support::{duration, fixture, four_four, guitar_voice};

fn title(value: &str) -> Command {
    Command::SetMeta {
        field: MetaField::Title,
        value: value.to_owned(),
    }
}

#[test]
fn a_new_history_has_nothing_to_undo_or_redo() {
    let mut fx = fixture();
    let before = fx.score.clone();
    let mut history = History::new();
    assert!(!history.can_undo());
    assert!(!history.can_redo());
    assert_eq!(history.undo(&mut fx.score), Ok(false));
    assert_eq!(history.redo(&mut fx.score), Ok(false));
    assert_eq!(fx.score, before);
}

#[test]
fn execute_undo_redo_walks_the_score_back_and_forth() {
    let mut fx = fixture();
    let original = fx.score.clone();
    let mut history = History::new();
    history.execute(&mut fx.score, title("One")).unwrap();
    history.execute(&mut fx.score, title("Two")).unwrap();
    let edited = fx.score.clone();
    assert!(history.can_undo());
    assert!(!history.can_redo());
    assert_eq!(history.undo(&mut fx.score), Ok(true));
    assert_eq!(fx.score.meta.title, "One");
    assert_eq!(history.undo(&mut fx.score), Ok(true));
    assert_eq!(fx.score, original);
    assert_eq!(history.undo(&mut fx.score), Ok(false));
    assert!(history.can_redo());
    assert_eq!(history.redo(&mut fx.score), Ok(true));
    assert_eq!(history.redo(&mut fx.score), Ok(true));
    assert_eq!(history.redo(&mut fx.score), Ok(false));
    assert_eq!(fx.score, edited);
}

#[test]
fn execute_clears_the_redo_stack() {
    let mut fx = fixture();
    let mut history = History::new();
    history.execute(&mut fx.score, title("One")).unwrap();
    history.undo(&mut fx.score).unwrap();
    assert!(history.can_redo());
    history.execute(&mut fx.score, title("Two")).unwrap();
    assert!(!history.can_redo());
    assert_eq!(history.redo(&mut fx.score), Ok(false));
    assert_eq!(fx.score.meta.title, "Two");
}

#[test]
fn a_failing_execute_keeps_both_stacks() {
    let mut fx = fixture();
    let mut history = History::new();
    history.execute(&mut fx.score, title("One")).unwrap();
    history.execute(&mut fx.score, title("Two")).unwrap();
    history.undo(&mut fx.score).unwrap();
    let before = fx.score.clone();
    let bad = Command::RemoveBeat {
        beat: BeatId(ElementId(9999)),
    };
    assert!(history.execute(&mut fx.score, bad).is_err());
    assert_eq!(fx.score, before);
    assert!(history.can_undo());
    assert!(history.can_redo());
    assert_eq!(history.redo(&mut fx.score), Ok(true));
    assert_eq!(fx.score.meta.title, "Two");
}

#[test]
fn redo_reuses_the_ids_of_the_first_execution() {
    let mut fx = fixture();
    let mut history = History::new();
    let insert = Command::InsertBeat {
        voice: fx.voice,
        index: 1,
        duration: duration(NoteValue::Eighth),
    };
    history.execute(&mut fx.score, insert.clone()).unwrap();
    let first = guitar_voice(&fx.score, 0).beats[1].id;
    history
        .execute(
            &mut fx.score,
            Command::AddNote {
                beat: first,
                string: 2,
                fret: 4,
            },
        )
        .unwrap();
    let after = fx.score.clone();
    history.undo(&mut fx.score).unwrap();
    history.undo(&mut fx.score).unwrap();
    history.redo(&mut fx.score).unwrap();
    history.redo(&mut fx.score).unwrap();
    assert_eq!(fx.score, after);
    assert_eq!(guitar_voice(&fx.score, 0).beats[1].id, first);
}

#[test]
fn undone_ids_are_never_handed_out_again() {
    let mut fx = fixture();
    let mut history = History::new();
    history
        .execute(
            &mut fx.score,
            Command::InsertBeat {
                voice: fx.voice,
                index: 0,
                duration: duration(NoteValue::Eighth),
            },
        )
        .unwrap();
    let undone = guitar_voice(&fx.score, 0).beats[0].id;
    history
        .execute(
            &mut fx.score,
            Command::AddNote {
                beat: undone,
                string: 1,
                fret: 2,
            },
        )
        .unwrap();
    let undone_note = guitar_voice(&fx.score, 0).beats[0].notes[0].id;
    let undone_ids = [ElementId::from(undone), ElementId::from(undone_note)];
    history.undo(&mut fx.score).unwrap();
    history.undo(&mut fx.score).unwrap();
    let existing: Vec<ElementId> = all_ids(&fx.score);
    history
        .execute(
            &mut fx.score,
            Command::InsertMasterBar {
                index: 0,
                time_signature: four_four(),
            },
        )
        .unwrap();
    history
        .execute(
            &mut fx.score,
            Command::InsertBeat {
                voice: fx.voice,
                index: 0,
                duration: duration(NoteValue::Quarter),
            },
        )
        .unwrap();
    let created: Vec<ElementId> = all_ids(&fx.score)
        .into_iter()
        .filter(|id| !existing.contains(id))
        .collect();
    assert_eq!(created.len(), 6);
    for id in &undone_ids {
        assert!(!created.contains(id), "undone id {id:?} was reused");
    }
    assert_eq!(
        history.execute(&mut fx.score, Command::RemoveBeat { beat: undone }),
        Err(CommandError::NotFound {
            what: "beat",
            id: undone.0.0
        })
    );
    assert_eq!(
        history.execute(&mut fx.score, Command::RemoveNote { note: undone_note }),
        Err(CommandError::NotFound {
            what: "note",
            id: undone_note.0.0
        })
    );
}

fn all_ids(score: &opentab_model::Score) -> Vec<ElementId> {
    let mut ids: Vec<ElementId> = score.master_bars.iter().map(|m| m.id.0).collect();
    for track in &score.tracks {
        ids.push(track.id.0);
        for staff in &track.staves {
            ids.push(staff.id.0);
            for bar in &staff.bars {
                ids.push(bar.id.0);
                for voice in &bar.voices {
                    ids.push(voice.id.0);
                    for beat in &voice.beats {
                        ids.push(beat.id.0);
                        ids.extend(beat.notes.iter().map(|note| note.id.0));
                    }
                }
            }
        }
    }
    ids
}
