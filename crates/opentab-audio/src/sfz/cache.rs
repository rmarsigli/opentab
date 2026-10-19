use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::sfz::decode::{DecodedSample, decode_within};
use crate::sfz::limits::{MIB, SfzLimits};
use crate::sfz::paths::{ReadProblem, read_capped};
use crate::{BankError, UnsafeBank};

#[derive(Debug, Clone)]
enum Slot {
    Pending,
    Ready(Arc<DecodedSample>),
    Failed(BankError),
}

#[derive(Debug)]
struct CacheState {
    slots: Vec<Slot>,
    used: u64,
    decodes: u64,
}

#[derive(Debug)]
pub(crate) struct SampleCache {
    paths: Vec<PathBuf>,
    limits: SfzLimits,
    state: Mutex<CacheState>,
}

#[derive(Debug, Clone)]
pub(crate) struct Fetched {
    pub(crate) sample: Result<Arc<DecodedSample>, BankError>,
    pub(crate) first_failure: bool,
}

impl SampleCache {
    pub(crate) fn new(paths: Vec<PathBuf>, limits: SfzLimits) -> SampleCache {
        let slots = vec![Slot::Pending; paths.len()];
        SampleCache {
            paths,
            limits,
            state: Mutex::new(CacheState {
                slots,
                used: 0,
                decodes: 0,
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, CacheState> {
        match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.paths.len()
    }

    pub(crate) fn used_bytes(&self) -> u64 {
        self.lock().used
    }

    pub(crate) fn decodes(&self) -> u64 {
        self.lock().decodes
    }

    fn load(&self, index: usize, budget: u64) -> Result<DecodedSample, BankError> {
        let path = self.paths.get(index).ok_or_else(|| BankError::Invalid {
            reason: "a region names an unknown sample".to_owned(),
        })?;
        let bytes =
            read_capped(path, self.limits.audio_file_bytes).map_err(|problem| match problem {
                ReadProblem::TooLarge => BankError::Unsafe {
                    reason: UnsafeBank::AudioFileTooLarge {
                        limit_mib: self.limits.audio_file_bytes / MIB,
                    },
                },
                ReadProblem::Missing => BankError::Invalid {
                    reason: format!("the sample {} cannot be read", path.display()),
                },
            })?;
        decode_within(&bytes, &self.limits, budget)
    }

    pub(crate) fn fetch(&self, index: usize) -> Fetched {
        let mut state = self.lock();
        match state.slots.get(index) {
            Some(Slot::Ready(sample)) => {
                return Fetched {
                    sample: Ok(Arc::clone(sample)),
                    first_failure: false,
                };
            }
            Some(Slot::Failed(error)) => {
                return Fetched {
                    sample: Err(error.clone()),
                    first_failure: false,
                };
            }
            Some(Slot::Pending) | None => {}
        }
        let budget = self.limits.decoded_bytes.saturating_sub(state.used);
        state.decodes += 1;
        let outcome = self.load(index, budget);
        let (slot, fetched) = match outcome {
            Ok(sample) => {
                state.used = state.used.saturating_add(sample.bytes());
                let shared = Arc::new(sample);
                (
                    Slot::Ready(Arc::clone(&shared)),
                    Fetched {
                        sample: Ok(shared),
                        first_failure: false,
                    },
                )
            }
            Err(error) => (
                Slot::Failed(error.clone()),
                Fetched {
                    sample: Err(error),
                    first_failure: true,
                },
            ),
        };
        if let Some(target) = state.slots.get_mut(index) {
            *target = slot;
        }
        fetched
    }
}
