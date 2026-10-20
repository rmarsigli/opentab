use std::fmt;

use opentab_model::{InstrumentFamily, TrackId};

use crate::manager::id::BankRef;
use crate::{Synth, TrackBackend};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FallbackReason {
    NotInstalled,
    FilesMissing,
    BadName,
    Unloadable { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Fallback {
    pub wanted: String,
    pub reason: FallbackReason,
}

impl fmt::Display for Fallback {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.reason {
            FallbackReason::NotInstalled => {
                write!(formatter, "sound bank {} is not installed", self.wanted)
            }
            FallbackReason::FilesMissing => {
                write!(
                    formatter,
                    "the files of sound bank {} are missing",
                    self.wanted
                )
            }
            FallbackReason::BadName => {
                write!(formatter, "{:?} is not a sound bank name", self.wanted)
            }
            FallbackReason::Unloadable { message } => {
                write!(
                    formatter,
                    "sound bank {} cannot be used: {message}",
                    self.wanted
                )
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TrackSound {
    pub track: TrackId,
    pub family: InstrumentFamily,
    pub bank: BankRef,
    pub fallback: Option<Fallback>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct SoundPlan {
    pub tracks: Vec<TrackSound>,
}

pub struct LoadedVoices {
    pub voices: Vec<Box<dyn Synth>>,
    pub backends: Vec<TrackBackend>,
}

impl fmt::Debug for LoadedVoices {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoadedVoices")
            .field("voices", &self.voices.len())
            .field("backends", &self.backends)
            .finish()
    }
}
