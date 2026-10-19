use opentab_model::{
    Base64Bytes, ContainerEntry, ModelError, Score, SoundOverride, TrackId, check_sound_overrides,
};
use serde::{Deserialize, Serialize};

use crate::error::{GpError, Refusal};

pub(crate) const OVERRIDES_ENTRY: &str = "Content/OpenTab/sound-overrides.json";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OverrideFile {
    sound_overrides: Vec<OverrideEntry>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OverrideEntry {
    track: usize,
    bank: String,
}

fn entry_error(reason: String) -> GpError {
    GpError::Entry {
        entry: OVERRIDES_ENTRY.to_string(),
        reason,
    }
}

pub(crate) fn take(
    entries: &mut Vec<ContainerEntry>,
    score: &Score,
) -> Result<Vec<SoundOverride>, GpError> {
    let Some(position) = entries
        .iter()
        .position(|entry| entry.name() == OVERRIDES_ENTRY)
    else {
        return Ok(Vec::new());
    };
    let entry = entries.remove(position);
    let ContainerEntry::File { data, .. } = entry else {
        return Ok(Vec::new());
    };
    let file: OverrideFile =
        serde_json::from_slice(&data.0).map_err(|error| entry_error(error.to_string()))?;
    let mut overrides = Vec::with_capacity(file.sound_overrides.len());
    for item in file.sound_overrides {
        let track: TrackId = score
            .tracks
            .get(item.track)
            .map(|track| track.id)
            .ok_or_else(|| entry_error(format!("track {} does not exist", item.track)))?;
        overrides.push(SoundOverride {
            track,
            bank: item.bank,
        });
    }
    check_sound_overrides(&overrides).map_err(|error| entry_error(error.to_string()))?;
    Ok(overrides)
}

pub(crate) fn entry(score: &Score) -> Result<Option<ContainerEntry>, GpError> {
    if score.sound_overrides.is_empty() {
        return Ok(None);
    }
    if let Err(ModelError::DuplicateSoundOverride { track }) =
        check_sound_overrides(&score.sound_overrides)
    {
        return Err(GpError::Unsupported {
            path: "sound_overrides".to_string(),
            reason: Refusal::DuplicateSoundOverride { track },
        });
    }
    let mut items = Vec::with_capacity(score.sound_overrides.len());
    for (index, item) in score.sound_overrides.iter().enumerate() {
        let track = score
            .tracks
            .iter()
            .position(|track| track.id == item.track)
            .ok_or_else(|| GpError::Unsupported {
                path: format!("sound_overrides[{index}]"),
                reason: Refusal::SoundOverrideTrack {
                    track: item.track.0.0,
                },
            })?;
        items.push(OverrideEntry {
            track,
            bank: item.bank.clone(),
        });
    }
    let file = OverrideFile {
        sound_overrides: items,
    };
    let data = serde_json::to_vec_pretty(&file).map_err(|error| GpError::Write {
        reason: error.to_string(),
    })?;
    Ok(Some(ContainerEntry::File {
        name: OVERRIDES_ENTRY.to_string(),
        data: Base64Bytes(data),
    }))
}
