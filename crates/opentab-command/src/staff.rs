use opentab_model::{Bar, Score, Staff, StaffId, Track, TrackId};

use crate::ids::{
    bar_ids, check_removable, check_restorable, put_extensions, reserve, take_extensions,
};
use crate::locate::{StaffPath, find_staff, not_found};
use crate::rules::{already_broken, check_bar, check_empty_bar, empty_bar};
use crate::{Command, CommandError, ExtensionEntry};

const STAFF: &str = "staff";

pub(crate) struct Appended<'a> {
    pub(crate) track: TrackId,
    pub(crate) staff: StaffId,
    pub(crate) bars: &'a [Bar],
    pub(crate) extensions: &'a [ExtensionEntry],
}

fn staff_ref(score: &Score, path: StaffPath, staff: StaffId) -> Result<&Staff, CommandError> {
    score
        .tracks
        .get(path.track)
        .and_then(|track| track.staves.get(path.staff))
        .ok_or(not_found(STAFF, staff))
}

fn track_ref(score: &Score, path: StaffPath, staff: StaffId) -> Result<&Track, CommandError> {
    score.tracks.get(path.track).ok_or(not_found(STAFF, staff))
}

fn staff_in(
    score: &mut Score,
    path: StaffPath,
    staff: StaffId,
) -> Result<&mut Staff, CommandError> {
    score
        .tracks
        .get_mut(path.track)
        .and_then(|track| track.staves.get_mut(path.staff))
        .ok_or(not_found(STAFF, staff))
}

pub(crate) fn pad(
    score: &mut Score,
    track: TrackId,
    staff: StaffId,
) -> Result<Command, CommandError> {
    let path = find_staff(score, track, staff)?;
    let have = staff_ref(score, path, staff)?.bars.len();
    let want = score.master_bars.len();
    let Some(missing) = want.checked_sub(have).filter(|missing| *missing > 0) else {
        return Err(CommandError::invalid(format!(
            "staff {} already has {have} bars for {want} master bars; nothing to pad",
            staff.0.0
        )));
    };
    let needed = u64::try_from(missing)
        .ok()
        .and_then(|bars| bars.checked_mul(2))
        .ok_or_else(|| CommandError::invalid("too many bars to allocate ids for"))?;
    reserve(score, needed)?;
    let mut bars = Vec::with_capacity(missing);
    for _ in 0..missing {
        bars.push(empty_bar(&mut score.ids)?);
    }
    staff_in(score, path, staff)?.bars.extend(bars);
    Ok(Command::TrimStaffBars {
        track,
        staff,
        len: have,
    })
}

pub(crate) fn trim(
    score: &mut Score,
    track: TrackId,
    staff: StaffId,
    len: usize,
) -> Result<Command, CommandError> {
    let path = find_staff(score, track, staff)?;
    let current = staff_ref(score, path, staff)?;
    let want = score.master_bars.len();
    if current.bars.len() > want {
        return Err(CommandError::invalid(format!(
            "staff {} has {} bars for {want} master bars; trim only shortens a staff that is not overfull; run diagnose",
            staff.0.0,
            current.bars.len()
        )));
    }
    let Some(removed) = current.bars.get(len..).filter(|tail| !tail.is_empty()) else {
        return Err(CommandError::invalid(format!(
            "staff {} has {} bars; nothing to trim down to {len}",
            staff.0.0,
            current.bars.len()
        )));
    };
    let owner = track_ref(score, path, staff)?;
    for bar in removed {
        check_bar(owner, bar).map_err(already_broken)?;
        check_empty_bar(bar)?;
    }
    check_removable(score, removed.iter().flat_map(bar_ids))?;
    let bars = staff_in(score, path, staff)?.bars.split_off(len);
    let extensions = take_extensions(&mut score.extensions, bars.iter().flat_map(bar_ids));
    Ok(Command::RestoreStaffBars {
        track,
        staff,
        bars,
        extensions,
    })
}

pub(crate) fn restore(score: &mut Score, appended: Appended<'_>) -> Result<Command, CommandError> {
    let Appended {
        track,
        staff,
        bars,
        extensions,
    } = appended;
    let path = find_staff(score, track, staff)?;
    let have = staff_ref(score, path, staff)?.bars.len();
    let want = score.master_bars.len();
    let fits = have
        .checked_add(bars.len())
        .is_some_and(|total| total <= want);
    if bars.is_empty() || !fits {
        return Err(CommandError::invalid(format!(
            "restore_staff_bars carries {} bars for staff {} with {have} of {want} bars",
            bars.len(),
            staff.0.0
        )));
    }
    let owner = track_ref(score, path, staff)?;
    for bar in bars {
        check_bar(owner, bar)?;
        check_empty_bar(bar)?;
    }
    check_restorable(score, bars.iter().flat_map(bar_ids), extensions)?;
    staff_in(score, path, staff)?
        .bars
        .extend(bars.iter().cloned());
    put_extensions(&mut score.extensions, extensions);
    Ok(Command::TrimStaffBars {
        track,
        staff,
        len: have,
    })
}
