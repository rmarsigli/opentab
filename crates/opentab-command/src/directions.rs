use opentab_model::{Directions, Score};

use crate::{Command, CommandError};

const WHAT: &str = "master bar";

pub(crate) fn set(
    score: &mut Score,
    master_bar: usize,
    directions: &Directions,
) -> Result<Command, CommandError> {
    let len = score.master_bars.len();
    let target = score
        .master_bars
        .get_mut(master_bar)
        .ok_or(CommandError::IndexOutOfRange {
            what: WHAT,
            index: master_bar,
            len,
        })?;
    let old = std::mem::replace(&mut target.directions, directions.clone());
    Ok(Command::SetDirections {
        master_bar,
        directions: old,
    })
}
