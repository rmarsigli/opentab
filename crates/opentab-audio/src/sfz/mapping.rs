use serde::Deserialize;

use crate::BankError;

pub const SIDECAR_NAME: &str = "opentab-bank.json";
pub const SIDECAR_VERSION: u32 = 1;
const PROGRAMS: usize = 128;
const DRUM_WORDS: [&str; 3] = ["drum", "kit", "perc"];

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct SidecarProgram {
    program: u8,
    file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sidecar {
    version: u32,
    #[serde(default)]
    programs: Vec<SidecarProgram>,
    #[serde(default)]
    drums: Option<String>,
    #[serde(default)]
    default: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProgramMap {
    programs: Vec<Option<usize>>,
    default: Option<usize>,
    drums: Option<usize>,
}

fn invalid(reason: impl Into<String>) -> BankError {
    BankError::Invalid {
        reason: reason.into(),
    }
}

pub(crate) fn is_drum_named(file: &str) -> bool {
    let lower = file.to_lowercase();
    DRUM_WORDS.iter().any(|word| lower.contains(word))
}

fn find(files: &[String], wanted: &str) -> Result<usize, BankError> {
    let wanted = wanted.replace('\\', "/");
    files
        .iter()
        .position(|file| *file == wanted)
        .ok_or_else(|| {
            invalid(format!(
                "the program map names {wanted}, which is not an instrument of this bank"
            ))
        })
}

impl ProgramMap {
    pub(crate) fn fallback(files: &[String]) -> ProgramMap {
        let melodic = files
            .iter()
            .position(|file| !is_drum_named(file))
            .or(if files.is_empty() { None } else { Some(0) });
        let drums = files
            .iter()
            .position(|file| is_drum_named(file))
            .or(melodic);
        ProgramMap {
            programs: vec![None; PROGRAMS],
            default: melodic,
            drums,
        }
    }

    pub(crate) fn from_sidecar(files: &[String], text: &[u8]) -> Result<ProgramMap, BankError> {
        let sidecar: Sidecar = serde_json::from_slice(text)
            .map_err(|error| invalid(format!("the program map cannot be read: {error}")))?;
        if sidecar.version != SIDECAR_VERSION {
            return Err(invalid(format!(
                "the program map has version {}, and OpenTab reads version {SIDECAR_VERSION}",
                sidecar.version
            )));
        }
        let mut map = ProgramMap::fallback(files);
        for entry in &sidecar.programs {
            let slot = map
                .programs
                .get_mut(usize::from(entry.program))
                .ok_or_else(|| {
                    invalid(format!("the program map names program {}", entry.program))
                })?;
            *slot = Some(find(files, &entry.file)?);
        }
        if let Some(default) = &sidecar.default {
            map.default = Some(find(files, default)?);
        }
        if let Some(drums) = &sidecar.drums {
            map.drums = Some(find(files, drums)?);
        }
        Ok(map)
    }

    pub(crate) fn melodic(&self, program: u8) -> Option<usize> {
        self.programs
            .get(usize::from(program))
            .copied()
            .flatten()
            .or(self.default)
    }

    pub(crate) fn drums(&self) -> Option<usize> {
        self.drums
    }

    pub(crate) fn lowest_program(&self, instrument: usize) -> Option<u8> {
        self.programs
            .iter()
            .position(|slot| *slot == Some(instrument))
            .and_then(|program| u8::try_from(program).ok())
    }
}
