use opentab_model::{Directions, MasterBar, Score, TripletFeel};

use crate::{Command, CommandError};

const WHAT: &str = "master bar";

fn master_bar_at(score: &mut Score, master_bar: usize) -> Result<&mut MasterBar, CommandError> {
    let len = score.master_bars.len();
    score
        .master_bars
        .get_mut(master_bar)
        .ok_or(CommandError::IndexOutOfRange {
            what: WHAT,
            index: master_bar,
            len,
        })
}

pub(crate) fn set_triplet_feel(
    score: &mut Score,
    master_bar: usize,
    triplet_feel: TripletFeel,
) -> Result<Command, CommandError> {
    let target = master_bar_at(score, master_bar)?;
    let old = std::mem::replace(&mut target.triplet_feel, triplet_feel);
    Ok(Command::SetTripletFeel {
        master_bar,
        triplet_feel: old,
    })
}

pub(crate) fn set(
    score: &mut Score,
    master_bar: usize,
    directions: &Directions,
) -> Result<Command, CommandError> {
    let target = master_bar_at(score, master_bar)?;
    let old = std::mem::replace(&mut target.directions, directions.clone());
    Ok(Command::SetDirections {
        master_bar,
        directions: old,
    })
}
