use serde::de::IgnoredAny;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::container::check_container;
use crate::{
    ContainerEntry, Extensions, IdAllocator, MasterBar, ModelError, OpaquePayload, Score,
    ScoreMeta, Track,
};

const VERSION: u64 = 2;

#[derive(Serialize)]
struct DocumentOut<'a> {
    opentab_json: u64,
    meta: &'a ScoreMeta,
    anacrusis: bool,
    master_bars: &'a [MasterBar],
    tracks: &'a [Track],
    extensions: &'a Extensions,
    document_extensions: &'a [OpaquePayload],
    container: &'a [ContainerEntry],
    ids: &'a IdAllocator,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DocumentIn {
    #[serde(rename = "opentab_json")]
    _opentab_json: IgnoredAny,
    meta: ScoreMeta,
    anacrusis: bool,
    master_bars: Vec<MasterBar>,
    tracks: Vec<Track>,
    extensions: Extensions,
    document_extensions: Vec<OpaquePayload>,
    container: Vec<ContainerEntry>,
    ids: IdAllocator,
}

#[derive(Deserialize)]
struct VersionProbe {
    opentab_json: Option<Value>,
}

pub fn to_json(score: &Score) -> Result<String, ModelError> {
    check_container(&score.container)?;
    let document = DocumentOut {
        opentab_json: VERSION,
        meta: &score.meta,
        anacrusis: score.anacrusis,
        master_bars: &score.master_bars,
        tracks: &score.tracks,
        extensions: &score.extensions,
        document_extensions: &score.document_extensions,
        container: &score.container,
        ids: &score.ids,
    };
    serde_json::to_string_pretty(&document).map_err(json_error)
}

pub fn from_json(input: &str) -> Result<Score, ModelError> {
    let probe: VersionProbe = serde_json::from_str(input).map_err(json_error)?;
    match probe.opentab_json {
        None => return Err(ModelError::MissingVersion),
        Some(version) if version.as_u64() != Some(VERSION) => {
            return Err(ModelError::UnsupportedVersion {
                found: version.to_string(),
            });
        }
        Some(_) => {}
    }
    let document: DocumentIn = serde_json::from_str(input).map_err(json_error)?;
    check_container(&document.container)?;
    Ok(Score {
        meta: document.meta,
        anacrusis: document.anacrusis,
        master_bars: document.master_bars,
        tracks: document.tracks,
        extensions: document.extensions,
        document_extensions: document.document_extensions,
        container: document.container,
        ids: document.ids,
    })
}

fn json_error(error: serde_json::Error) -> ModelError {
    ModelError::Json {
        message: error.to_string(),
    }
}
