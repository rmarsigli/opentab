use std::collections::BTreeMap;

use opentab_command::{Command, CommandError, History, element_ids};
use opentab_model::Score;

use crate::error::{ToolError, echo};

pub const MAX_SESSIONS: usize = 16;

#[derive(Debug, Clone)]
pub struct Session {
    pub path: String,
    pub score: Score,
    history: History,
    undo_batches: Vec<usize>,
    redo_batches: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub done: bool,
    pub commands: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchFailure {
    pub index: usize,
    pub error: CommandError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Undo,
    Redo,
}

impl Session {
    pub fn new(path: String, score: Score) -> Session {
        Session {
            path,
            score,
            history: History::new(),
            undo_batches: Vec::new(),
            redo_batches: Vec::new(),
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_batches.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_batches.is_empty()
    }

    pub fn apply(&mut self, commands: Vec<Command>) -> Result<Vec<u64>, BatchFailure> {
        let before = element_ids(&self.score);
        let mut score = self.score.clone();
        let mut history = self.history.clone();
        let count = commands.len();
        for (index, command) in commands.into_iter().enumerate() {
            history
                .execute(&mut score, command)
                .map_err(|error| BatchFailure { index, error })?;
        }
        let allocated = element_ids(&score).difference(&before).copied().collect();
        self.score = score;
        self.history = history;
        if count > 0 {
            self.undo_batches.push(count);
            self.redo_batches.clear();
        }
        Ok(allocated)
    }

    pub fn undo(&mut self) -> Result<Step, ToolError> {
        self.step(Direction::Undo)
    }

    pub fn redo(&mut self) -> Result<Step, ToolError> {
        self.step(Direction::Redo)
    }

    fn step(&mut self, direction: Direction) -> Result<Step, ToolError> {
        let batches = match direction {
            Direction::Undo => &self.undo_batches,
            Direction::Redo => &self.redo_batches,
        };
        let Some(&commands) = batches.last() else {
            return Ok(Step {
                done: false,
                commands: 0,
            });
        };
        let mut score = self.score.clone();
        let mut history = self.history.clone();
        for _ in 0..commands {
            let moved = match direction {
                Direction::Undo => history.undo(&mut score),
                Direction::Redo => history.redo(&mut score),
            }
            .map_err(|error| ToolError::command_failed(None, &error))?;
            if !moved {
                return Err(ToolError::internal(
                    "the edit history is shorter than the recorded batches",
                ));
            }
        }
        self.score = score;
        self.history = history;
        let (from, to) = match direction {
            Direction::Undo => (&mut self.undo_batches, &mut self.redo_batches),
            Direction::Redo => (&mut self.redo_batches, &mut self.undo_batches),
        };
        from.pop();
        to.push(commands);
        Ok(Step {
            done: true,
            commands,
        })
    }
}

#[derive(Debug, Default)]
pub struct Sessions {
    open: BTreeMap<String, Session>,
    opened: u64,
}

impl Sessions {
    pub fn insert(&mut self, session: Session) -> Result<String, ToolError> {
        if self.open.len() >= MAX_SESSIONS {
            return Err(ToolError::invalid_input(format!(
                "at most {MAX_SESSIONS} sessions may be open at once; close one first"
            )));
        }
        self.opened += 1;
        let id = format!("session-{}", self.opened);
        self.open.insert(id.clone(), session);
        Ok(id)
    }

    pub fn get(&mut self, id: &str) -> Result<&mut Session, ToolError> {
        self.open.get_mut(id).ok_or_else(|| unknown(id))
    }

    pub fn remove(&mut self, id: &str) -> Result<Session, ToolError> {
        self.open.remove(id).ok_or_else(|| unknown(id))
    }
}

fn unknown(id: &str) -> ToolError {
    ToolError::invalid_input(format!(
        "unknown session {}; open a score first and pass the session it returns",
        echo(id)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        Session::new("a.gp".to_owned(), Score::new())
    }

    #[test]
    fn sessions_are_bounded_and_ids_are_never_reused() {
        let mut sessions = Sessions::default();
        let ids: Vec<String> = (0..MAX_SESSIONS)
            .map(|_| sessions.insert(session()).unwrap())
            .collect();
        assert_eq!(ids[0], "session-1");
        assert_eq!(ids[MAX_SESSIONS - 1], format!("session-{MAX_SESSIONS}"));
        let error = sessions.insert(session()).unwrap_err();
        assert_eq!(error.kind, "invalid_input");
        sessions.remove(&ids[3]).unwrap();
        assert_eq!(
            sessions.insert(session()).unwrap(),
            format!("session-{}", MAX_SESSIONS + 1)
        );
        assert_eq!(sessions.get(&ids[3]).unwrap_err().kind, "invalid_input");
        assert_eq!(sessions.remove(&ids[3]).unwrap_err().kind, "invalid_input");
    }

    #[test]
    fn a_failing_batch_leaves_the_score_and_history_untouched() {
        let mut session = session();
        let title = |value: &str| Command::SetMeta {
            field: opentab_command::MetaField::Title,
            value: value.to_owned(),
        };
        session.apply(vec![title("one")]).unwrap();
        let failure = session
            .apply(vec![
                title("two"),
                Command::RemoveMasterBar { index: 7 },
                title("three"),
            ])
            .unwrap_err();
        assert_eq!(failure.index, 1);
        assert_eq!(session.score.meta.title, "one");
        assert_eq!(
            session.undo().unwrap(),
            Step {
                done: true,
                commands: 1
            }
        );
        assert_eq!(session.score.meta.title, "");
        assert!(!session.can_undo());
    }

    #[test]
    fn undo_and_redo_move_whole_batches() {
        let mut session = session();
        let title = |value: &str| Command::SetMeta {
            field: opentab_command::MetaField::Title,
            value: value.to_owned(),
        };
        session.apply(vec![title("a"), title("b")]).unwrap();
        session.apply(vec![title("c")]).unwrap();
        session.apply(Vec::new()).unwrap();
        assert_eq!(session.undo().unwrap().commands, 1);
        assert_eq!(session.score.meta.title, "b");
        assert_eq!(session.undo().unwrap().commands, 2);
        assert_eq!(session.score.meta.title, "");
        assert!(!session.undo().unwrap().done);
        assert_eq!(session.redo().unwrap().commands, 2);
        assert_eq!(session.score.meta.title, "b");
        assert!(session.can_redo());
        session.apply(vec![title("d")]).unwrap();
        assert!(!session.can_redo());
        assert!(!session.redo().unwrap().done);
        assert_eq!(session.undo().unwrap().commands, 1);
        assert_eq!(session.score.meta.title, "b");
    }
}
