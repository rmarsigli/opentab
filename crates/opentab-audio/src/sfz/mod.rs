mod cache;
mod decode;
mod limits;
mod listing;
mod mapping;
mod parse;
mod paths;
mod player;
mod region;
mod text;
mod vorbis;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use opentab_model::MidiBank;

pub use decode::{AudioKind, DecodedSample, audio_kind, decode_audio, vorbis_setup_cost};
pub use limits::{
    SFZ_MAX_AUDIO_FILE_BYTES, SFZ_MAX_BANK_INCLUDES, SFZ_MAX_BANK_TEXT_BYTES,
    SFZ_MAX_DECODED_BYTES, SFZ_MAX_DEFINE_BYTES, SFZ_MAX_DEFINE_NAME_BYTES, SFZ_MAX_DEFINES,
    SFZ_MAX_EXPANDED_BYTES, SFZ_MAX_FILE_BYTES, SFZ_MAX_FOLDER_DEPTH, SFZ_MAX_FOLDER_ENTRIES,
    SFZ_MAX_INCLUDE_DEPTH, SFZ_MAX_INCLUDES, SFZ_MAX_LINE_BYTES, SFZ_MAX_LISTED_ENTRIES,
    SFZ_MAX_LOADER_BYTES, SFZ_MAX_PATH_BYTES, SFZ_MAX_PATH_LOOKUPS, SFZ_MAX_READ_BYTES,
    SFZ_MAX_REGIONS, SFZ_MAX_SAMPLE_CHANNELS, SFZ_MAX_SAMPLE_FRAMES, SFZ_MAX_SAMPLE_RATE,
    SFZ_MAX_SFZ_FILES, SFZ_MAX_SIDECAR_BYTES, SFZ_MAX_VOICES, SFZ_MAX_VOICES_PER_NOTE,
    SFZ_MAX_VORBIS_COMMENT_BYTES, SFZ_MAX_VORBIS_ENTRIES, SFZ_MAX_VORBIS_SETUP_BYTES,
    SFZ_MIN_SAMPLE_RATE, SfzLimits,
};
pub use listing::{IgnoreReason, IgnoredOpcode, SfzScope};
pub use mapping::{SIDECAR_NAME, SIDECAR_VERSION};
pub use player::{SFZ_MASTER_GAIN, SfzSynth};
pub use region::{CcRange, Envelope, LoopMode, OffMode, SfzRegion, Trigger, VelocityPoint};
pub use vorbis::VorbisSetupCost;

use cache::SampleCache;
use limits::{KIB, MIB};
use listing::Listing;
use mapping::ProgramMap;
use parse::{parse, region_cost};
use paths::{BankFs, PathProblem, ReadProblem, read_capped, refusal};
use text::{FsIncludes, MemoryIncludes, Preprocessor, decode_text};

use crate::sf2::{FaultLog, SynthFault};
use crate::{Bank, BankError, BankFormat, BankInstrument, SampleRate, Synth, UnsafeBank};

const AUDIO_EXTENSIONS: [&str; 4] = ["wav", "flac", "ogg", "oga"];
const DRUM_BANK_MSB: u8 = 120;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlayRegion {
    pub(crate) region: SfzRegion,
    pub(crate) sample: usize,
}

#[derive(Debug)]
pub(crate) struct SfzInstrument {
    pub(crate) name: String,
    pub(crate) file: String,
    pub(crate) regions: Vec<PlayRegion>,
    pub(crate) set_cc: Vec<(u8, u8)>,
    pub(crate) active_switch: Option<u8>,
    pub(crate) listing: Listing,
}

#[derive(Debug)]
pub(crate) struct SfzShared {
    pub(crate) instruments: Vec<SfzInstrument>,
    pub(crate) map: ProgramMap,
    pub(crate) cache: SampleCache,
    pub(crate) faults: FaultLog,
    pub(crate) limits: SfzLimits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedInstrument {
    pub file: String,
    pub error: BankError,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SfzText {
    pub regions: Vec<SfzRegion>,
    pub ignored: Vec<IgnoredOpcode>,
    pub default_path: String,
    pub set_cc: Vec<(u8, u8)>,
    pub sw_default: Option<u8>,
    pub expanded_bytes: usize,
    pub read_bytes: u64,
}

pub struct SfzBank {
    shared: Arc<SfzShared>,
    root: PathBuf,
    skipped: Vec<SkippedInstrument>,
    loader_bytes: u64,
    listed_entries: u64,
    path_lookups: u64,
    lookup_steps: u64,
    next_instance: AtomicU32,
}

impl fmt::Debug for SfzBank {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SfzBank")
            .field("root", &self.root)
            .field("instruments", &self.shared.instruments.len())
            .field("samples", &self.shared.cache.len())
            .finish()
    }
}

fn refuse(reason: UnsafeBank) -> BankError {
    BankError::Unsafe { reason }
}

fn file_too_large(limits: &SfzLimits) -> BankError {
    refuse(UnsafeBank::SfzFileTooLarge {
        limit_kib: limits.file_bytes / KIB,
    })
}

fn active_switch(regions: &[PlayRegion], sw_default: Option<u8>) -> Option<u8> {
    sw_default.or_else(|| regions.iter().filter_map(|play| play.region.sw_last).min())
}

fn strip_extension(file: &str) -> String {
    match file.rfind('.') {
        Some(dot) if !file[dot..].contains('/') => file[..dot].to_owned(),
        _ => file.to_owned(),
    }
}

fn is_audio(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        AUDIO_EXTENSIONS
            .iter()
            .any(|known| extension.eq_ignore_ascii_case(known))
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct SfzTextRun {
    pub result: Result<SfzText, BankError>,
    pub steps: u64,
}

pub fn parse_sfz_text(
    text: &str,
    includes: &BTreeMap<String, String>,
    limits: &SfzLimits,
) -> Result<SfzText, BankError> {
    measure_sfz_text(text, includes, limits).result
}

pub fn measure_sfz_text(
    text: &str,
    includes: &BTreeMap<String, String>,
    limits: &SfzLimits,
) -> SfzTextRun {
    if text.len() as u64 > limits.file_bytes {
        return SfzTextRun {
            result: Err(file_too_large(limits)),
            steps: 0,
        };
    }
    let mut source = MemoryIncludes {
        files: includes,
        cap: limits.file_bytes,
    };
    let root_dir = PathBuf::new();
    let measured =
        Preprocessor::new(&mut source, limits, &root_dir).run_measured(Path::new("root.sfz"), text);
    let steps = measured.steps;
    let result = measured.result.and_then(|expanded| {
        let mut parsed = parse(&expanded.text, limits, limits.loader_bytes)?;
        parsed.listing.merge(expanded.listing);
        Ok(SfzText {
            regions: parsed.regions,
            ignored: parsed.listing.entries(),
            default_path: parsed.control.default_path,
            set_cc: parsed.control.set_cc,
            sw_default: parsed.control.sw_default,
            expanded_bytes: expanded.text.len(),
            read_bytes: expanded.read_bytes,
        })
    });
    SfzTextRun { result, steps }
}

struct Loader<'a> {
    fs: BankFs,
    limits: &'a SfzLimits,
    text_bytes: u64,
    loader_bytes: u64,
    samples: Vec<PathBuf>,
    sample_index: HashMap<PathBuf, usize>,
    resolved: HashMap<(PathBuf, String), Result<usize, IgnoreReason>>,
    include_count: u64,
}

enum Expansion {
    Done(text::Expanded),
    NotText,
}

impl Loader<'_> {
    fn count_text(&mut self, bytes: usize) -> Result<(), BankError> {
        self.text_bytes = self.text_bytes.saturating_add(bytes as u64);
        if self.text_bytes > self.limits.bank_text_bytes {
            return Err(refuse(UnsafeBank::BankTextTooLarge {
                limit_mib: self.limits.bank_text_bytes / MIB,
            }));
        }
        Ok(())
    }

    fn expand(&mut self, file: &Path) -> Result<Expansion, BankError> {
        let bytes = match read_capped(file, self.limits.file_bytes) {
            Ok(bytes) => bytes,
            Err(ReadProblem::TooLarge) => return Err(file_too_large(self.limits)),
            Err(ReadProblem::Missing) => return Ok(Expansion::NotText),
        };
        let Some(text) = decode_text(&bytes) else {
            return Ok(Expansion::NotText);
        };
        let root_dir = file.parent().unwrap_or(file).to_path_buf();
        let limits = self.limits;
        let mut source = FsIncludes {
            fs: &mut self.fs,
            cap: limits.file_bytes,
        };
        let expanded = Preprocessor::new(&mut source, limits, &root_dir)
            .after_includes(self.include_count)
            .run(file, text)?;
        self.include_count = self.include_count.saturating_add(expanded.includes);
        self.count_text(
            expanded
                .text
                .len()
                .saturating_add(usize::try_from(expanded.read_bytes).unwrap_or(usize::MAX)),
        )?;
        Ok(Expansion::Done(expanded))
    }

    fn sample(
        &mut self,
        root_dir: &Path,
        text: &str,
    ) -> Result<Result<usize, IgnoreReason>, BankError> {
        self.count_text(text.len())?;
        let key = (root_dir.to_path_buf(), text.to_owned());
        if let Some(found) = self.resolved.get(&key) {
            return Ok(*found);
        }
        let outcome = match self.fs.resolve(root_dir, text) {
            Ok(path) if !is_audio(&path) => Err(IgnoreReason::UnsupportedAudioFormat),
            Ok(path) => {
                let next = self.samples.len();
                let index = *self.sample_index.entry(path.clone()).or_insert(next);
                if index == next {
                    self.samples.push(path);
                }
                Ok(index)
            }
            Err(PathProblem::Outside | PathProblem::Rejected) => Err(IgnoreReason::PathOutsideBank),
            Err(PathProblem::Missing) => Err(IgnoreReason::MissingFile),
            Err(
                problem @ (PathProblem::Exhausted | PathProblem::Lookups | PathProblem::Budget),
            ) => {
                return Err(refusal(problem, self.limits).unwrap_or(refuse(
                    UnsafeBank::ListedEntries {
                        limit: self.limits.listed_entries,
                    },
                )));
            }
        };
        self.loader_bytes = self
            .loader_bytes
            .saturating_add((key.0.as_os_str().len() + key.1.len()) as u64 * 2);
        self.resolved.insert(key, outcome);
        Ok(outcome)
    }

    fn instrument(
        &mut self,
        file: &Path,
        expanded: text::Expanded,
    ) -> Result<SfzInstrument, BankError> {
        let budget = self.limits.loader_bytes.saturating_sub(self.loader_bytes);
        let parsed = parse(&expanded.text, self.limits, budget)?;
        drop(expanded.text);
        let root_dir = file.parent().unwrap_or(file).to_path_buf();
        let mut listing = parsed.listing;
        listing.merge(expanded.listing);
        let mut kept = Vec::new();
        for region in parsed.regions {
            match self.sample(&root_dir, &region.sample)? {
                Ok(sample) => kept.push(PlayRegion { region, sample }),
                Err(reason) => listing.note("sample", SfzScope::Region, reason),
            }
        }
        let retained: u64 = kept
            .iter()
            .map(|play| region_cost(&play.region))
            .sum::<u64>()
            + listing.bytes();
        self.loader_bytes = self.loader_bytes.saturating_add(retained);
        if self.loader_bytes.saturating_add(self.fs.cache_bytes()) > self.limits.loader_bytes {
            return Err(refuse(UnsafeBank::LoaderBudget));
        }
        let file_name = self.fs.relative_name(file);
        Ok(SfzInstrument {
            name: strip_extension(&file_name),
            file: file_name,
            active_switch: active_switch(&kept, parsed.control.sw_default),
            regions: kept,
            set_cc: parsed.control.set_cc,
            listing,
        })
    }
}

impl SfzBank {
    pub fn load(root: &Path) -> Result<SfzBank, BankError> {
        SfzBank::load_with_limits(root, &SfzLimits::DEFAULT)
    }

    pub fn load_with_limits(root: &Path, limits: &SfzLimits) -> Result<SfzBank, BankError> {
        let fs = BankFs::open(root, limits)?;
        let files = fs.sfz_files(limits)?;
        let sidecar = match read_capped(&fs.root().join(SIDECAR_NAME), limits.sidecar_bytes) {
            Ok(bytes) => Some(bytes),
            Err(ReadProblem::Missing) => None,
            Err(ReadProblem::TooLarge) => {
                return Err(refuse(UnsafeBank::SidecarTooLarge {
                    limit_kib: limits.sidecar_bytes / KIB,
                }));
            }
        };
        let canonical_root = fs.root().to_path_buf();
        let mut loader = Loader {
            fs,
            limits,
            text_bytes: 0,
            loader_bytes: 0,
            samples: Vec::new(),
            sample_index: HashMap::new(),
            resolved: HashMap::new(),
            include_count: 0,
        };
        let mut included: HashSet<PathBuf> = HashSet::new();
        for file in &files {
            if let Expansion::Done(expanded) = loader.expand(file)? {
                included.extend(expanded.included);
            }
        }
        let mut instruments = Vec::new();
        let mut skipped = Vec::new();
        for file in files.iter().filter(|file| !included.contains(*file)) {
            match loader.expand(file)? {
                Expansion::Done(expanded) => instruments.push(loader.instrument(file, expanded)?),
                Expansion::NotText => skipped.push(SkippedInstrument {
                    file: loader.fs.relative_name(file),
                    error: BankError::Invalid {
                        reason: "the instrument file is not UTF-8 text".to_owned(),
                    },
                }),
            }
        }
        if instruments.is_empty() {
            return Err(BankError::Invalid {
                reason: "the folder holds no playable SFZ instrument".to_owned(),
            });
        }
        let names: Vec<String> = instruments
            .iter()
            .map(|instrument| instrument.file.clone())
            .collect();
        let map = match &sidecar {
            Some(bytes) => ProgramMap::from_sidecar(&names, bytes)?,
            None => ProgramMap::fallback(&names),
        };
        let loader_bytes = loader.loader_bytes;
        let listed_entries = loader.fs.listed();
        let path_lookups = loader.fs.lookups();
        let lookup_steps = loader.fs.steps();
        let cache = SampleCache::new(loader.samples, *limits);
        Ok(SfzBank {
            shared: Arc::new(SfzShared {
                instruments,
                map,
                cache,
                faults: FaultLog::default(),
                limits: *limits,
            }),
            root: canonical_root,
            skipped,
            loader_bytes,
            listed_entries,
            path_lookups,
            lookup_steps,
            next_instance: AtomicU32::new(0),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn instrument_names(&self) -> Vec<String> {
        self.shared
            .instruments
            .iter()
            .map(|instrument| instrument.name.clone())
            .collect()
    }

    fn find(&self, name: &str) -> Option<&SfzInstrument> {
        self.shared
            .instruments
            .iter()
            .find(|instrument| instrument.name == name)
    }

    pub fn regions(&self, name: &str) -> Vec<SfzRegion> {
        self.find(name)
            .map(|instrument| {
                instrument
                    .regions
                    .iter()
                    .map(|play| play.region.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn ignored(&self, name: &str) -> Vec<IgnoredOpcode> {
        self.find(name)
            .map(|instrument| instrument.listing.entries())
            .unwrap_or_default()
    }

    pub fn skipped(&self) -> &[SkippedInstrument] {
        &self.skipped
    }

    pub fn melodic_instrument(&self, program: u8) -> Option<String> {
        let index = self.shared.map.melodic(program)?;
        self.shared
            .instruments
            .get(index)
            .map(|instrument| instrument.name.clone())
    }

    pub fn drum_instrument(&self) -> Option<String> {
        let index = self.shared.map.drums()?;
        self.shared
            .instruments
            .get(index)
            .map(|instrument| instrument.name.clone())
    }

    pub fn sample_count(&self) -> usize {
        self.shared.cache.len()
    }

    pub fn decoded_bytes(&self) -> u64 {
        self.shared.cache.used_bytes()
    }

    pub fn decode_count(&self) -> u64 {
        self.shared.cache.decodes()
    }

    pub fn loader_bytes(&self) -> u64 {
        self.loader_bytes
    }

    pub fn listed_entries(&self) -> u64 {
        self.listed_entries
    }

    pub fn path_lookups(&self) -> u64 {
        self.path_lookups
    }

    pub fn lookup_steps(&self) -> u64 {
        self.lookup_steps
    }

    pub fn shared_count(&self) -> usize {
        Arc::strong_count(&self.shared)
    }

    pub fn faults(&self) -> Vec<SynthFault> {
        self.shared.faults.snapshot()
    }

    pub fn preload(&self, name: &str) -> Result<usize, BankError> {
        let Some(instrument) = self.find(name) else {
            return Ok(0);
        };
        let mut samples: Vec<usize> = instrument.regions.iter().map(|play| play.sample).collect();
        samples.sort_unstable();
        samples.dedup();
        for sample in &samples {
            self.shared.cache.fetch(*sample).sample?;
        }
        Ok(samples.len())
    }

    pub fn synth(&self, rate: SampleRate) -> SfzSynth {
        let instance = self.next_instance.fetch_add(1, Ordering::Relaxed);
        SfzSynth::new(Arc::clone(&self.shared), rate, instance)
    }
}

impl Bank for SfzBank {
    fn format(&self) -> BankFormat {
        BankFormat::Sfz
    }

    fn instruments(&self) -> Vec<BankInstrument> {
        let drums = self.shared.map.drums();
        self.shared
            .instruments
            .iter()
            .enumerate()
            .map(|(index, instrument)| {
                let drum = drums == Some(index)
                    && self.shared.map.lowest_program(index).is_none()
                    && self.shared.map.melodic(0) != Some(index);
                let bank = if drum {
                    MidiBank::new(DRUM_BANK_MSB, 0).unwrap_or_default()
                } else {
                    MidiBank::default()
                };
                BankInstrument {
                    bank,
                    program: self.shared.map.lowest_program(index).unwrap_or(0),
                    name: instrument.name.clone(),
                }
            })
            .collect()
    }

    fn instantiate(&self, rate: SampleRate) -> Box<dyn Synth> {
        Box::new(self.synth(rate))
    }
}
