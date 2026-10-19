mod beat;
mod context;
mod document;
mod drums;
mod master;
mod mixer;
mod note;
mod pools;
mod property;
mod track;

pub(crate) use context::Handled;
pub(crate) use mixer::same_connection;
pub(crate) use pools::POOLS;
pub(crate) use track::ChordItem;
pub(crate) use track::chord_items;

use opentab_model::Score;

use crate::archive::{SCORE_ENTRY, minimal_container, read_archive};
use crate::dom;
use crate::error::{GpError, GpifProblem};
use crate::overrides;

const BYTE_ORDER_MARK: &[u8] = b"\xEF\xBB\xBF";

pub fn read_gp(bytes: &[u8]) -> Result<Score, GpError> {
    let mut archive = read_archive(bytes)?;
    let mut score = read_gpif(&archive.score)?;
    score.sound_overrides = overrides::take(&mut archive.entries, &score)?;
    if archive.entries != minimal_container() {
        score.container = archive.entries;
    }
    Ok(score)
}

pub(crate) fn read_gpif(bytes: &[u8]) -> Result<Score, GpError> {
    let bytes = bytes.strip_prefix(BYTE_ORDER_MARK).unwrap_or(bytes);
    let text = std::str::from_utf8(bytes)
        .map_err(|_| gpif_error("/".to_string(), GpifProblem::NotUtf8))?;
    let root = dom::parse(text).map_err(|failure| gpif_error(failure.path, failure.problem))?;
    document::read(&root, bytes.len())
        .map_err(|fault| gpif_error(fault.location.render(&root), fault.problem))
}

fn gpif_error(path: String, problem: GpifProblem) -> GpError {
    GpError::Gpif {
        entry: SCORE_ENTRY.to_string(),
        path,
        problem,
    }
}
