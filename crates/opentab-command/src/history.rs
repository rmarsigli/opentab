use opentab_model::Score;

use crate::{Command, CommandError, apply};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct History {
    undo: Vec<Command>,
    redo: Vec<Command>,
}

impl History {
    pub fn new() -> History {
        History::default()
    }

    pub fn execute(&mut self, score: &mut Score, command: Command) -> Result<(), CommandError> {
        let inverse = apply(score, &command)?;
        self.undo.push(inverse);
        self.redo.clear();
        Ok(())
    }

    pub fn undo(&mut self, score: &mut Score) -> Result<bool, CommandError> {
        let Some(inverse) = self.undo.last() else {
            return Ok(false);
        };
        let forward = apply(score, inverse)?;
        self.undo.pop();
        self.redo.push(forward);
        Ok(true)
    }

    pub fn redo(&mut self, score: &mut Score) -> Result<bool, CommandError> {
        let Some(forward) = self.redo.last() else {
            return Ok(false);
        };
        let inverse = apply(score, forward)?;
        self.redo.pop();
        self.undo.push(inverse);
        Ok(true)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
