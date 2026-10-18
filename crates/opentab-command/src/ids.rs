use std::collections::{HashMap, HashSet};
use std::iter::once;

use opentab_model::{Bar, Beat, ElementId, Extensions, IdAllocator, Score, Track, Voice};

use crate::{CommandError, ExtensionEntry};

pub(crate) fn score_ids(score: &Score) -> impl Iterator<Item = ElementId> + '_ {
    score
        .master_bars
        .iter()
        .map(|master_bar| master_bar.id.0)
        .chain(score.tracks.iter().flat_map(track_ids))
}

fn track_ids(track: &Track) -> impl Iterator<Item = ElementId> + '_ {
    once(track.id.0).chain(
        track
            .staves
            .iter()
            .flat_map(|staff| once(staff.id.0).chain(staff.bars.iter().flat_map(bar_ids))),
    )
}

pub(crate) fn bar_ids(bar: &Bar) -> impl Iterator<Item = ElementId> + '_ {
    once(bar.id.0).chain(bar.voices.iter().flat_map(voice_ids))
}

fn voice_ids(voice: &Voice) -> impl Iterator<Item = ElementId> + '_ {
    once(voice.id.0).chain(voice.beats.iter().flat_map(beat_ids))
}

pub(crate) fn beat_ids(beat: &Beat) -> impl Iterator<Item = ElementId> + '_ {
    once(beat.id.0).chain(beat.notes.iter().map(|note| note.id.0))
}

fn exhausted(next: ElementId, count: u64) -> CommandError {
    CommandError::invalid(format!(
        "the id space is exhausted: allocator at {} cannot provide {count} more ids",
        next.0
    ))
}

pub(crate) fn next_id<T: From<ElementId>>(ids: &mut IdAllocator) -> Result<T, CommandError> {
    let next = ids.peek();
    ids.next().map(T::from).ok_or_else(|| exhausted(next, 1))
}

pub(crate) fn reserve(score: &Score, count: u64) -> Result<(), CommandError> {
    let next = score.ids.peek();
    if next.0.checked_add(count).is_none() {
        return Err(exhausted(next, count));
    }
    if let Some(highest) = score_ids(score).max()
        && highest >= next
    {
        return Err(CommandError::invalid(format!(
            "element id {} is not below the id allocator ({}); run diagnose",
            highest.0, next.0
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Subtree {
    Restored,
    Removed,
}

impl Subtree {
    fn copies_in_score(self) -> usize {
        match self {
            Subtree::Restored => 0,
            Subtree::Removed => 1,
        }
    }
}

fn check_subtree<I>(score: &Score, ids: I, subtree: Subtree) -> Result<Vec<ElementId>, CommandError>
where
    I: IntoIterator<Item = ElementId>,
{
    let next = score.ids.peek();
    let mut counts: HashMap<ElementId, usize> = HashMap::new();
    let mut order = Vec::new();
    for id in ids {
        if id >= next {
            return Err(CommandError::invalid(format!(
                "element id {} is not below the id allocator ({}); restored ids must have been allocated",
                id.0, next.0
            )));
        }
        if counts.insert(id, 0).is_some() {
            return Err(CommandError::invalid(format!(
                "element id {} appears twice in the subtree",
                id.0
            )));
        }
        order.push(id);
    }
    for id in score_ids(score) {
        if let Some(count) = counts.get_mut(&id) {
            *count = count.saturating_add(1);
        }
    }
    let expected = subtree.copies_in_score();
    if let Some(id) = order
        .iter()
        .find(|id| counts.get(id).is_some_and(|count| *count != expected))
    {
        return Err(CommandError::invalid(format!(
            "element id {} already exists in the score; run diagnose",
            id.0
        )));
    }
    Ok(order)
}

pub(crate) fn check_removable<I>(score: &Score, removed: I) -> Result<(), CommandError>
where
    I: IntoIterator<Item = ElementId>,
{
    check_subtree(score, removed, Subtree::Removed).map(|_| ())
}

pub(crate) fn check_restorable<I>(
    score: &Score,
    restored: I,
    extensions: &[ExtensionEntry],
) -> Result<(), CommandError>
where
    I: IntoIterator<Item = ElementId>,
{
    let order = check_subtree(score, restored, Subtree::Restored)?;
    if let Some(id) = order.iter().find(|id| score.extensions.0.contains_key(id)) {
        return Err(CommandError::invalid(format!(
            "element id {} already has orphan extension payloads in the score",
            id.0
        )));
    }
    let fresh: HashSet<ElementId> = order.into_iter().collect();
    let mut entries = HashSet::new();
    for entry in extensions {
        if !fresh.contains(&entry.id) {
            return Err(CommandError::invalid(format!(
                "extension entry {} does not belong to the restored elements",
                entry.id.0
            )));
        }
        if !entries.insert(entry.id) {
            return Err(CommandError::invalid(format!(
                "extension entry {} is duplicated",
                entry.id.0
            )));
        }
    }
    Ok(())
}

pub(crate) fn take_extensions<I>(extensions: &mut Extensions, ids: I) -> Vec<ExtensionEntry>
where
    I: IntoIterator<Item = ElementId>,
{
    let mut taken = Vec::new();
    if extensions.0.is_empty() {
        return taken;
    }
    for id in ids {
        if let Some(payloads) = extensions.0.remove(&id) {
            taken.push(ExtensionEntry { id, payloads });
        }
    }
    taken.sort_by_key(|entry| entry.id);
    taken
}

pub(crate) fn put_extensions(extensions: &mut Extensions, restored: &[ExtensionEntry]) {
    extensions.0.extend(
        restored
            .iter()
            .map(|entry| (entry.id, entry.payloads.clone())),
    );
}
