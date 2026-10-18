use std::iter::once;

use opentab_model::{Bar, KeySignature, MasterBar, Score, TimeSignature, Voice};

use crate::ids::{bar_ids, check_restorable, next_id, put_extensions, reserve, take_extensions};
use crate::{Command, CommandError, ExtensionEntry};

const WHAT: &str = "master bar";

pub(crate) struct Snapshot<'a> {
    pub(crate) master_bar: &'a MasterBar,
    pub(crate) bars: &'a [Bar],
    pub(crate) extensions: &'a [ExtensionEntry],
}

fn check_index(score: &Score, index: usize, inserting: bool) -> Result<(), CommandError> {
    let len = score.master_bars.len();
    let fits = if inserting { index <= len } else { index < len };
    if fits {
        Ok(())
    } else {
        Err(CommandError::IndexOutOfRange {
            what: WHAT,
            index,
            len,
        })
    }
}

fn check_staves(score: &Score) -> Result<usize, CommandError> {
    let expected = score.master_bars.len();
    let mut count = 0usize;
    for staff in score.tracks.iter().flat_map(|track| &track.staves) {
        if staff.bars.len() != expected {
            return Err(CommandError::invalid(format!(
                "staff {} has {} bars but the score has {expected} master bars",
                staff.id.0.0,
                staff.bars.len()
            )));
        }
        count = count.saturating_add(1);
    }
    Ok(count)
}

pub(crate) fn insert(
    score: &mut Score,
    index: usize,
    time_signature: TimeSignature,
) -> Result<Command, CommandError> {
    check_index(score, index, true)?;
    let staves = check_staves(score)?;
    let needed = u64::try_from(staves)
        .ok()
        .and_then(|staves| staves.checked_mul(2))
        .and_then(|ids| ids.checked_add(1))
        .ok_or_else(|| CommandError::invalid("too many staves to allocate ids for"))?;
    reserve(score, needed)?;
    let key_signature = index
        .checked_sub(1)
        .and_then(|previous| score.master_bars.get(previous))
        .map_or_else(KeySignature::default, |previous| previous.key_signature);
    let master_bar = MasterBar {
        id: next_id(&mut score.ids)?,
        time_signature,
        key_signature,
        repeat_start: false,
        repeat_end: None,
        alternate_endings: 0,
        marker: None,
        tempo_changes: Vec::new(),
    };
    score.master_bars.insert(index, master_bar);
    let ids = &mut score.ids;
    for staff in score.tracks.iter_mut().flat_map(|track| &mut track.staves) {
        let bar = Bar {
            id: next_id(ids)?,
            voices: vec![Voice {
                id: next_id(ids)?,
                beats: Vec::new(),
            }],
        };
        staff.bars.insert(index, bar);
    }
    Ok(Command::RemoveMasterBar { index })
}

pub(crate) fn remove(score: &mut Score, index: usize) -> Result<Command, CommandError> {
    check_index(score, index, false)?;
    check_staves(score)?;
    let master_bar = score.master_bars.remove(index);
    let bars: Vec<Bar> = score
        .tracks
        .iter_mut()
        .flat_map(|track| &mut track.staves)
        .map(|staff| staff.bars.remove(index))
        .collect();
    let removed = once(master_bar.id.0).chain(bars.iter().flat_map(bar_ids));
    let extensions = take_extensions(&mut score.extensions, removed);
    Ok(Command::RestoreMasterBar {
        index,
        master_bar,
        bars,
        extensions,
    })
}

pub(crate) fn restore(
    score: &mut Score,
    index: usize,
    snapshot: Snapshot<'_>,
) -> Result<Command, CommandError> {
    check_index(score, index, true)?;
    let staves = check_staves(score)?;
    if snapshot.bars.len() != staves {
        return Err(CommandError::invalid(format!(
            "restore_master_bar carries {} bars for {staves} staves",
            snapshot.bars.len()
        )));
    }
    let restored = once(snapshot.master_bar.id.0).chain(snapshot.bars.iter().flat_map(bar_ids));
    check_restorable(score, restored, snapshot.extensions)?;
    score.master_bars.insert(index, snapshot.master_bar.clone());
    let staves = score.tracks.iter_mut().flat_map(|track| &mut track.staves);
    for (staff, bar) in staves.zip(snapshot.bars) {
        staff.bars.insert(index, bar.clone());
    }
    put_extensions(&mut score.extensions, snapshot.extensions);
    Ok(Command::RemoveMasterBar { index })
}
