use opentab_model::{Fraction, Score, Tempo, TempoChange};

use crate::{Command, CommandError};

const WHAT: &str = "master bar";

fn out_of_range(score: &Score, index: usize) -> CommandError {
    CommandError::IndexOutOfRange {
        what: WHAT,
        index,
        len: score.master_bars.len(),
    }
}

pub(crate) fn set(score: &mut Score, master_bar: usize, bpm: f64) -> Result<Command, CommandError> {
    if master_bar >= score.master_bars.len() {
        return Err(out_of_range(score, master_bar));
    }
    let tempo = Tempo::new(bpm).map_err(CommandError::model)?;
    let error = out_of_range(score, master_bar);
    let target = score.master_bars.get_mut(master_bar).ok_or(error)?;
    let old = target.tempo_changes.clone();
    let start = Fraction::from_integer(0);
    match target
        .tempo_changes
        .iter_mut()
        .find(|change| change.position == start)
    {
        Some(change) => change.bpm = tempo,
        None => target.tempo_changes.insert(
            0,
            TempoChange {
                position: start,
                bpm: tempo,
            },
        ),
    }
    Ok(Command::RestoreTempoChanges {
        master_bar,
        tempo_changes: old,
    })
}

pub(crate) fn restore(
    score: &mut Score,
    master_bar: usize,
    tempo_changes: &[TempoChange],
) -> Result<Command, CommandError> {
    if master_bar >= score.master_bars.len() {
        return Err(out_of_range(score, master_bar));
    }
    if let Some(change) = tempo_changes
        .iter()
        .find(|change| *change.position.numer() < 0 || *change.position.denom() <= 0)
    {
        return Err(CommandError::invalid(format!(
            "tempo change position {}/{} is negative",
            change.position.numer(),
            change.position.denom()
        )));
    }
    let error = out_of_range(score, master_bar);
    let target = score.master_bars.get_mut(master_bar).ok_or(error)?;
    let old = std::mem::replace(&mut target.tempo_changes, tempo_changes.to_vec());
    Ok(Command::RestoreTempoChanges {
        master_bar,
        tempo_changes: old,
    })
}
