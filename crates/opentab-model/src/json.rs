use serde::de::IgnoredAny;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Extensions, IdAllocator, MasterBar, ModelError, Score, ScoreMeta, Track};

const VERSION: u64 = 1;

#[derive(Serialize)]
struct DocumentOut<'a> {
    opentab_json: u64,
    meta: &'a ScoreMeta,
    master_bars: &'a [MasterBar],
    tracks: &'a [Track],
    extensions: &'a Extensions,
    ids: &'a IdAllocator,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DocumentIn {
    #[serde(rename = "opentab_json")]
    _opentab_json: IgnoredAny,
    meta: ScoreMeta,
    master_bars: Vec<MasterBar>,
    tracks: Vec<Track>,
    extensions: Extensions,
    ids: IdAllocator,
}

#[derive(Deserialize)]
struct VersionProbe {
    opentab_json: Option<Value>,
}

pub fn to_json(score: &Score) -> String {
    let document = DocumentOut {
        opentab_json: VERSION,
        meta: &score.meta,
        master_bars: &score.master_bars,
        tracks: &score.tracks,
        extensions: &score.extensions,
        ids: &score.ids,
    };
    serde_json::to_string_pretty(&document).unwrap_or_default()
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
    Ok(Score {
        meta: document.meta,
        master_bars: document.master_bars,
        tracks: document.tracks,
        extensions: document.extensions,
        ids: document.ids,
    })
}

fn json_error(error: serde_json::Error) -> ModelError {
    ModelError::Json {
        message: error.to_string(),
    }
}
