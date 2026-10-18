use opentab_command::{Command, History, MetaField};
use opentab_model::{BeatId, ElementId, NoteValue};

use crate::support::{duration, fixture, guitar_voice};

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
fn an_undone_insert_frees_its_ids_for_the_next_edit() {
    let mut fx = fixture();
    let mut history = History::new();
    let insert = Command::InsertBeat {
        voice: fx.voice,
        index: 0,
        duration: duration(NoteValue::Eighth),
    };
    history.execute(&mut fx.score, insert.clone()).unwrap();
    let first = guitar_voice(&fx.score, 0).beats[0].id;
    history.undo(&mut fx.score).unwrap();
    history.execute(&mut fx.score, insert).unwrap();
    assert_eq!(guitar_voice(&fx.score, 0).beats[0].id, first);
}
