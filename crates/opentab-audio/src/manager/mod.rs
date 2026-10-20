mod archive;
mod catalogue;
mod copy;
mod detect;
mod download;
mod id;
mod install;
mod limits;
mod location;
mod plan;
mod store;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use opentab_model::{InstrumentFamily, Score};

pub use catalogue::{ArchiveKind, CatalogueEntry, default_catalogue};
pub use detect::{SidecarProgram, SidecarRecord, sidecar_for};
pub use download::{
    DOWNLOAD_CONNECT_SECONDS, DOWNLOAD_DEADLINE_FLOOR_SECONDS, DOWNLOAD_MAX_WAIT_SECONDS,
    DOWNLOAD_MIN_BYTES_PER_SECOND, DOWNLOAD_REDIRECTS, DOWNLOAD_RESPONSE_SECONDS,
    DOWNLOAD_STALL_SECONDS, DownloadTimeouts, InstallCancel, check_download_url,
};
pub use id::{BANK_ID_MAX_BYTES, BankId, BankRef};
pub use install::{InstallError, InstallProgress, InstallStage};
pub use limits::{
    ARCHIVE_MAX_RATIO, ARCHIVE_RATIO_FLOOR_BYTES, BANK_MAX_BYTES, BANK_MAX_DEPTH, BANK_MAX_ENTRIES,
    BANK_MAX_FILE_BYTES, BANK_MAX_NAME_BYTES, ImportLimits, STORE_MAX_BYTES,
};
pub use location::{DATA_DIR_NAME, DATA_DIR_VARIABLE, default_data_dir};
pub use plan::{Fallback, FallbackReason, LoadedVoices, SoundPlan, TrackSound};
pub use store::{
    BankSource, DAMAGED_SUFFIX, PREFERENCES_NAME, REGISTRY_NAME, STORE_VERSION, StoreProblem,
};

use archive::Unpacked;
use copy::{
    Budget, Listed, ListedKind, Survey, copy_files, copy_head, listed_file, refuse, storage,
};
use detect::Detected;
use download::{Expected, Fetch};
use store::{Preferences, Registry, StoredBank, StoredFormat};

use crate::{
    Bank, BankError, BankFormat, BankInstrument, BuiltInSynth, SF3_UNSUPPORTED, SFZ_MAX_FILE_BYTES,
    SIDECAR_NAME, SampleRate, Sf2Bank, SfzBank, Synth, TrackBackend, UnsafeBank,
};

pub const BANKS_FOLDER: &str = "banks";
pub const STAGING_FOLDER: &str = "staging";
const CONTENT_FOLDER: &str = "content";
const PROBE_FOLDER: &str = "probe";
const SFZ_EXTENSION: &str = "sfz";
const SAMPLE_EXTENSIONS: [&str; 4] = ["wav", "flac", "ogg", "oga"];
const INSTALL_CANCELLED: &str = "the install was cancelled";
const DOWNLOAD_NAME: &str = "download.part";
pub const LOCK_NAME: &str = ".lock";
const SNIFF_BYTES: usize = 12;
const ZIP_MAGIC: [&[u8]; 2] = [b"PK\x03\x04", b"PK\x05\x06"];
const SEVEN_ZIP_MAGIC: &[u8] = b"7z\xBC\xAF\x27\x1C";
const SEVEN_ZIP_IMPORT: &str = "OpenTab installs .7z sound banks from its catalogue only. Extract this archive (7-Zip can do it), then import the extracted folder.";
const SFZ_FILE_IMPORT: &str = "an SFZ instrument needs its samples: import the folder that holds the .sfz file and its samples, or a .zip archive of that folder";
const UNKNOWN_FILE: &str = "this is not a sound bank OpenTab imports; pick an .sf2 SoundFont, a folder holding .sfz instruments, or a .zip archive of one";
const NOT_UNICODE: &str = "a file name of the bank is not valid Unicode";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledBank {
    pub id: BankId,
    pub name: String,
    pub format: BankFormat,
    pub path: PathBuf,
    pub bytes: u64,
    pub source: BankSource,
    pub instruments: Vec<BankInstrument>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileKind {
    Zip,
    SevenZip,
    Sf2,
    Sf3,
    Sfz,
    Other,
}

struct Staging {
    path: PathBuf,
    lock: Option<File>,
}

impl Drop for Staging {
    fn drop(&mut self) {
        drop(self.lock.take());
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct Finished {
    id: Option<BankId>,
    name: String,
    format: Option<BankFormat>,
    source: BankSource,
    bytes: u64,
}

struct Usable {
    path: PathBuf,
    format: BankFormat,
}

struct Validated {
    format: BankFormat,
    path: PathBuf,
    instruments: Vec<BankInstrument>,
    title: String,
}

struct LoadedBank {
    format: BankFormat,
    bank: Box<dyn Bank>,
}

struct ChosenVoice {
    synth: Box<dyn Synth>,
    format: BankFormat,
    name: String,
}

#[derive(Debug)]
pub struct BankManager {
    root: PathBuf,
    catalogue: Vec<CatalogueEntry>,
    limits: ImportLimits,
    timeouts: DownloadTimeouts,
    registry: Registry,
    preferences: Preferences,
    problems: Vec<StoreProblem>,
}

fn create_folder(path: &Path) -> Result<(), BankError> {
    fs::create_dir_all(path).map_err(|error| storage(path, &error))
}

fn lock_file(path: &Path) -> Result<File, BankError> {
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(path)
        .map_err(|error| storage(path, &error))
}

fn stale(entry: &Path) -> bool {
    if !entry.is_dir() {
        return true;
    }
    let Ok(file) = File::open(entry.join(LOCK_NAME)) else {
        return true;
    };
    file.try_lock().is_ok()
}

fn clear_stale(staging: &Path, problems: &mut Vec<StoreProblem>) {
    let listing = match fs::read_dir(staging) {
        Ok(listing) => listing,
        Err(error) => {
            problems.push(StoreProblem {
                path: staging.to_path_buf(),
                moved_to: None,
                message: storage(staging, &error).to_string(),
            });
            return;
        }
    };
    for entry in listing.flatten() {
        let path = entry.path();
        if !stale(&path) {
            continue;
        }
        let removed = if path.is_dir() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        if let Err(error) = removed {
            problems.push(StoreProblem {
                message: storage(&path, &error).to_string(),
                path,
                moved_to: None,
            });
        }
    }
}

fn extension_is(path: &Path, wanted: &str) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(wanted))
}

fn sniff(path: &Path) -> Result<FileKind, BankError> {
    let mut head = Vec::with_capacity(SNIFF_BYTES);
    File::open(path)
        .and_then(|file| file.take(SNIFF_BYTES as u64).read_to_end(&mut head))
        .map_err(|error| storage(path, &error))?;
    if ZIP_MAGIC.iter().any(|magic| head.starts_with(magic)) {
        return Ok(FileKind::Zip);
    }
    if head.starts_with(SEVEN_ZIP_MAGIC) {
        return Ok(FileKind::SevenZip);
    }
    if extension_is(path, "sf3") {
        return Ok(FileKind::Sf3);
    }
    if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"sfbk".as_slice()) {
        return Ok(FileKind::Sf2);
    }
    if extension_is(path, "sfz") {
        return Ok(FileKind::Sfz);
    }
    if extension_is(path, "sf2") {
        return Ok(FileKind::Sf2);
    }
    Ok(FileKind::Other)
}

fn hint_of(path: &Path) -> String {
    let stem = if path.is_dir() {
        path.file_name()
    } else {
        path.file_stem()
    };
    stem.map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn portable(relative: &Path) -> Result<String, BankError> {
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => {
                parts.push(part.to_str().ok_or_else(|| BankError::Unsupported {
                    message: NOT_UNICODE.to_owned(),
                })?)
            }
            _ => return Err(refuse(UnsafeBank::ArchiveEscape)),
        }
    }
    Ok(parts.join("/"))
}

fn is_sample(relative: &Path) -> bool {
    SAMPLE_EXTENSIONS
        .iter()
        .any(|extension| extension_is(relative, extension))
}

fn placeholder(path: &Path) -> Result<(), BankError> {
    if let Some(parent) = path.parent() {
        create_folder(parent)?;
    }
    File::create(path)
        .map(drop)
        .map_err(|error| storage(path, &error))
}

fn cancelled_marker() -> BankError {
    BankError::Invalid {
        reason: INSTALL_CANCELLED.to_owned(),
    }
}

fn interrupted(cancel: &InstallCancel, error: BankError) -> InstallError {
    if cancel.is_cancelled() {
        return InstallError::Cancelled;
    }
    InstallError::Bank(error)
}

fn load_bank(format: BankFormat, path: &Path) -> Result<Box<dyn Bank>, BankError> {
    match format {
        BankFormat::Sf2 => Ok(Box::new(Sf2Bank::open(path)?)),
        BankFormat::Sfz => Ok(Box::new(SfzBank::load(path)?)),
        BankFormat::BuiltIn => Ok(Box::new(crate::BuiltInBank)),
    }
}

impl BankManager {
    pub fn open(root: &Path) -> Result<BankManager, BankError> {
        BankManager::with_catalogue(root, default_catalogue())
    }

    pub fn with_catalogue(
        root: &Path,
        entries: Vec<CatalogueEntry>,
    ) -> Result<BankManager, BankError> {
        create_folder(root)?;
        let root = root.canonicalize().map_err(|error| storage(root, &error))?;
        create_folder(&root.join(BANKS_FOLDER))?;
        let staging = root.join(STAGING_FOLDER);
        create_folder(&staging)?;
        let lock_path = root.join(LOCK_NAME);
        let lock = lock_file(&lock_path)?;
        lock.lock().map_err(|error| storage(&lock_path, &error))?;
        let mut problems = Vec::new();
        clear_stale(&staging, &mut problems);
        let registry: Registry = store::read_or_set_aside(&root.join(REGISTRY_NAME), &mut problems);
        let preferences: Preferences =
            store::read_or_set_aside(&root.join(PREFERENCES_NAME), &mut problems);
        drop(lock);
        Ok(BankManager {
            root,
            catalogue: entries,
            limits: ImportLimits::DEFAULT,
            timeouts: DownloadTimeouts::DEFAULT,
            registry,
            preferences,
            problems,
        })
    }

    pub fn with_limits(mut self, limits: ImportLimits) -> BankManager {
        self.limits = limits;
        self
    }

    pub fn with_download_timeouts(mut self, timeouts: DownloadTimeouts) -> BankManager {
        self.timeouts = timeouts;
        self
    }

    pub fn problems(&self) -> &[StoreProblem] {
        &self.problems
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn staging_dir(&self) -> PathBuf {
        self.root.join(STAGING_FOLDER)
    }

    fn stage(&self) -> Result<Staging, BankError> {
        let parent = self.staging_dir();
        create_folder(&parent)?;
        let process = std::process::id();
        for attempt in 0u64.. {
            let path = parent.join(format!("{process}-{attempt}"));
            match fs::create_dir(&path) {
                Ok(()) => {
                    let lock = lock_file(&path.join(LOCK_NAME))?;
                    lock.lock().map_err(|error| storage(&path, &error))?;
                    return Ok(Staging {
                        path,
                        lock: Some(lock),
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(storage(&path, &error)),
            }
        }
        Err(BankError::Storage {
            message: parent.display().to_string(),
        })
    }

    fn locked(&self) -> Result<File, BankError> {
        let path = self.root.join(LOCK_NAME);
        let file = lock_file(&path)?;
        file.lock().map_err(|error| storage(&path, &error))?;
        Ok(file)
    }

    fn refresh(&mut self) -> Result<(), BankError> {
        self.registry = store::read(&self.root.join(REGISTRY_NAME))?;
        self.preferences = store::read(&self.root.join(PREFERENCES_NAME))?;
        Ok(())
    }

    pub fn reload(&mut self) -> Result<(), BankError> {
        let _lock = self.locked()?;
        self.refresh()
    }

    fn stored(&self, id: &BankId) -> Option<&StoredBank> {
        self.registry.banks.iter().find(|bank| &bank.id == id)
    }

    fn bank_path(&self, stored: &StoredBank) -> Option<PathBuf> {
        let mut parts = stored.path.split('/');
        if parts.next() != Some(BANKS_FOLDER) || parts.next() != Some(stored.id.as_str()) {
            return None;
        }
        let mut path = self.root.join(BANKS_FOLDER).join(stored.id.as_str());
        for part in parts {
            path.push(part);
        }
        let normal = path.strip_prefix(&self.root).is_ok_and(|relative| {
            relative
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
        });
        normal.then_some(path)
    }

    fn installed(&self, stored: &StoredBank) -> InstalledBank {
        InstalledBank {
            id: stored.id.clone(),
            name: stored.name.clone(),
            format: stored.format.format(),
            path: self
                .bank_path(stored)
                .unwrap_or_else(|| self.root.join(BANKS_FOLDER).join(stored.id.as_str())),
            bytes: stored.bytes,
            source: stored.source,
            instruments: store::listed_instruments(&stored.instruments),
        }
    }

    pub fn list(&self) -> Vec<InstalledBank> {
        self.registry
            .banks
            .iter()
            .map(|stored| self.installed(stored))
            .collect()
    }

    pub fn catalogue(&self) -> &[CatalogueEntry] {
        &self.catalogue
    }

    fn taken(&self, candidate: &str) -> bool {
        self.registry
            .banks
            .iter()
            .any(|bank| bank.id.as_str() == candidate)
            || self
                .catalogue
                .iter()
                .any(|entry| entry.id.as_str() == candidate)
            || fs::symlink_metadata(self.root.join(BANKS_FOLDER).join(candidate)).is_ok()
    }

    fn fresh_id(&self, hint: &str) -> Result<BankId, BankError> {
        let base = BankId::slug(hint);
        if !self.taken(&base) {
            return BankId::new(&base);
        }
        for number in 2u64.. {
            let candidate = format!("{base}-{number}");
            if !self.taken(&candidate) {
                return BankId::new(&candidate);
            }
        }
        Err(BankError::BadName { text: base })
    }

    fn folder_files(&self, source: &Path) -> Result<Vec<Listed>, BankError> {
        let survey = copy::survey(source, self.limits)?;
        match detect::classify(&survey)? {
            Detected::Sf2 { file } => {
                let listed = file
                    .strip_prefix(source)
                    .ok()
                    .and_then(|relative| survey.find(relative))
                    .ok_or_else(|| refuse(UnsafeBank::BankChanged))?;
                Ok(vec![listed.clone()])
            }
            Detected::Sfz { root, .. } => {
                let base = root
                    .strip_prefix(source)
                    .map_err(|_| refuse(UnsafeBank::BankChanged))?;
                self.sfz_references(&survey, base)
            }
        }
    }

    fn sfz_references(&self, survey: &Survey, base: &Path) -> Result<Vec<Listed>, BankError> {
        let staging = self.stage()?;
        let probe = staging.path.join(PROBE_FOLDER);
        create_folder(&probe)?;
        let under: Vec<&Listed> = survey
            .entries
            .iter()
            .filter(|entry| entry.relative.starts_with(base))
            .collect();
        for entry in &under {
            let path = probe.join(&entry.relative);
            match entry.kind {
                ListedKind::Folder => create_folder(&path)?,
                ListedKind::File | ListedKind::Link | ListedKind::Special => placeholder(&path)?,
            }
        }
        let probe = probe
            .canonicalize()
            .map_err(|error| storage(&probe, &error))?;
        let sidecar = base.join(SIDECAR_NAME);
        let mut wanted: Vec<&Listed> = under
            .iter()
            .copied()
            .filter(|entry| {
                entry.kind != ListedKind::Folder
                    && (extension_is(&entry.relative, SFZ_EXTENSION) || entry.relative == sidecar)
            })
            .collect();
        let mut budget = Budget::new(self.limits);
        let mut filled: HashSet<PathBuf> = HashSet::new();
        loop {
            for listed in wanted {
                if let Some(reason) = listed.kind.refusal() {
                    return Err(refuse(reason));
                }
                let target = probe.join(&listed.relative);
                fs::remove_file(&target).map_err(|error| storage(&target, &error))?;
                copy_head(
                    &survey.root.join(&listed.relative),
                    listed,
                    &target,
                    &mut budget,
                    SFZ_MAX_FILE_BYTES.saturating_add(1),
                )?;
                filled.insert(listed.relative.clone());
            }
            let mut files = Vec::new();
            let mut next = Vec::new();
            for path in SfzBank::referenced_files(&probe.join(base))? {
                let listed = path
                    .strip_prefix(&probe)
                    .ok()
                    .and_then(|relative| survey.find(relative))
                    .ok_or_else(|| refuse(UnsafeBank::BankChanged))?;
                if let Some(reason) = listed.kind.refusal() {
                    return Err(refuse(reason));
                }
                if listed.kind != ListedKind::File {
                    continue;
                }
                if !filled.contains(&listed.relative) && !is_sample(&listed.relative) {
                    next.push(listed);
                }
                files.push(listed.clone());
            }
            if next.is_empty() {
                return Ok(files);
            }
            wanted = next;
        }
    }

    pub fn import(&mut self, path: &Path) -> Result<InstalledBank, BankError> {
        let source = path.canonicalize().map_err(|error| storage(path, &error))?;
        let metadata = fs::metadata(&source).map_err(|error| storage(&source, &error))?;
        let mut budget = Budget::new(self.limits);
        let staging = if metadata.is_dir() {
            let files = self.folder_files(&source)?;
            let staging = self.stage()?;
            let content = staging.path.join(CONTENT_FOLDER);
            create_folder(&content)?;
            copy_files(&source, &files, &content, &mut budget)?;
            staging
        } else if metadata.is_file() {
            match sniff(&source)? {
                FileKind::Zip => {
                    let staging = self.stage()?;
                    let content = staging.path.join(CONTENT_FOLDER);
                    create_folder(&content)?;
                    archive::unzip(&source, &content, &mut budget, &mut |_| Ok(()))?;
                    staging
                }
                FileKind::Sf2 => {
                    let unnamed = || BankError::Unsupported {
                        message: UNKNOWN_FILE.to_owned(),
                    };
                    let folder = source.parent().ok_or_else(unnamed)?;
                    let name = source.file_name().ok_or_else(unnamed)?;
                    let listed = listed_file(&source, PathBuf::from(name))?;
                    let files = [listed];
                    let staging = self.stage()?;
                    let content = staging.path.join(CONTENT_FOLDER);
                    create_folder(&content)?;
                    copy_files(folder, &files, &content, &mut budget)?;
                    staging
                }
                FileKind::SevenZip => {
                    return Err(BankError::Unsupported {
                        message: SEVEN_ZIP_IMPORT.to_owned(),
                    });
                }
                FileKind::Sf3 => {
                    return Err(BankError::Unsupported {
                        message: SF3_UNSUPPORTED.to_owned(),
                    });
                }
                FileKind::Sfz => {
                    return Err(BankError::Unsupported {
                        message: SFZ_FILE_IMPORT.to_owned(),
                    });
                }
                FileKind::Other => {
                    return Err(BankError::Unsupported {
                        message: UNKNOWN_FILE.to_owned(),
                    });
                }
            }
        } else {
            return Err(refuse(UnsafeBank::SpecialFile));
        };
        let content = staging.path.join(CONTENT_FOLDER);
        let finished = Finished {
            id: None,
            name: hint_of(&source),
            format: None,
            source: BankSource::Imported,
            bytes: budget.written(),
        };
        let validated = self.validate(&content)?;
        self.finish(staging, &content, finished, validated)
    }

    fn validate(&self, content: &Path) -> Result<Validated, BankError> {
        Ok(match detect::detect(content, self.limits)? {
            Detected::Sf2 { file } => {
                let bank = Sf2Bank::open(&file)?;
                Validated {
                    format: BankFormat::Sf2,
                    instruments: bank.instruments(),
                    title: bank.name().trim().to_owned(),
                    path: file,
                }
            }
            Detected::Sfz { root, files } => {
                let first = SfzBank::load(&root)?;
                let named = detect::instrument_files(&files, &first.instrument_names());
                drop(first);
                detect::write_sidecar(&root, &named)?;
                let bank = SfzBank::load(&root)?;
                Validated {
                    format: BankFormat::Sfz,
                    instruments: bank.instruments(),
                    title: String::new(),
                    path: root,
                }
            }
        })
    }

    fn finish(
        &mut self,
        staging: Staging,
        content: &Path,
        finished: Finished,
        validated: Validated,
    ) -> Result<InstalledBank, BankError> {
        let format = validated.format;
        let title = validated.title;
        let instruments = validated.instruments;
        let bank_path = validated.path;
        if let Some(expected) = finished.format
            && expected != format
        {
            return Err(BankError::Invalid {
                reason: format!(
                    "the catalogue lists a {expected:?} bank, but the download holds a {format:?} bank"
                ),
            });
        }
        let _lock = self.locked()?;
        self.refresh()?;
        let id = match finished.id {
            Some(id) => {
                if let Some(stored) = self.stored(&id) {
                    return Ok(self.installed(stored));
                }
                id
            }
            None => self.fresh_id(&finished.name)?,
        };
        let name = match finished.source {
            BankSource::Catalogue => finished.name.clone(),
            BankSource::Imported if !title.is_empty() => title,
            BankSource::Imported if !finished.name.is_empty() => finished.name.clone(),
            BankSource::Imported => id.to_string(),
        };
        let relative = bank_path.strip_prefix(content).unwrap_or(Path::new(""));
        let relative = portable(relative)?;
        let folder = self.root.join(BANKS_FOLDER).join(id.as_str());
        if fs::symlink_metadata(&folder).is_ok() {
            fs::remove_dir_all(&folder).map_err(|error| storage(&folder, &error))?;
        }
        fs::rename(content, &folder).map_err(|error| storage(&folder, &error))?;
        let stored_path = if relative.is_empty() {
            format!("{BANKS_FOLDER}/{id}")
        } else {
            format!("{BANKS_FOLDER}/{id}/{relative}")
        };
        let stored = StoredBank {
            id: id.clone(),
            name,
            format: StoredFormat::of(format).unwrap_or(StoredFormat::Sf2),
            path: stored_path,
            bytes: finished.bytes,
            source: finished.source,
            instruments: store::stored_instruments(&instruments),
        };
        self.registry.banks.push(stored.clone());
        if let Err(error) = store::write(&self.root.join(REGISTRY_NAME), &self.registry) {
            self.registry.banks.pop();
            let _ = fs::remove_dir_all(&folder);
            return Err(error);
        }
        drop(staging);
        Ok(self.installed(&stored))
    }

    pub fn remove(&mut self, id: &BankId) -> Result<(), BankError> {
        let _lock = self.locked()?;
        self.refresh()?;
        let index = self
            .registry
            .banks
            .iter()
            .position(|bank| &bank.id == id)
            .ok_or_else(|| BankError::NotInstalled { id: id.to_string() })?;
        let stored = self.registry.banks.remove(index);
        if let Err(error) = store::write(&self.root.join(REGISTRY_NAME), &self.registry) {
            self.registry.banks.insert(index, stored);
            return Err(error);
        }
        let folder = self.root.join(BANKS_FOLDER).join(id.as_str());
        if fs::symlink_metadata(&folder).is_ok() {
            fs::remove_dir_all(&folder).map_err(|error| storage(&folder, &error))?;
        }
        Ok(())
    }

    pub fn install(
        &mut self,
        id: &BankId,
        progress: &mut dyn FnMut(InstallProgress),
    ) -> Result<InstalledBank, InstallError> {
        self.install_with(id, progress, &InstallCancel::new())
    }

    pub fn install_with(
        &mut self,
        id: &BankId,
        progress: &mut dyn FnMut(InstallProgress),
        cancel: &InstallCancel,
    ) -> Result<InstalledBank, InstallError> {
        let entry = self
            .catalogue
            .iter()
            .find(|entry| &entry.id == id)
            .cloned()
            .ok_or_else(|| InstallError::Unknown { id: id.to_string() })?;
        if let Some(stored) = self.stored(id) {
            return Ok(self.installed(stored));
        }
        check_download_url(&entry.url)?;
        let staging = self.stage()?;
        let download = staging.path.join(DOWNLOAD_NAME);
        download::fetch(
            &Expected {
                url: &entry.url,
                size: entry.size,
                sha256: &entry.sha256,
            },
            &download,
            &Fetch {
                timeouts: &self.timeouts,
                cancel,
            },
            progress,
        )?;
        cancel.check()?;
        progress(InstallProgress {
            stage: InstallStage::Unpacking,
            done: 0,
            total: entry.size,
        });
        let content = staging.path.join(CONTENT_FOLDER);
        create_folder(&content)?;
        let mut budget = Budget::new(self.limits);
        let mut step = |unpacked: Unpacked| {
            progress(InstallProgress {
                stage: InstallStage::Unpacking,
                done: unpacked.done,
                total: unpacked.total,
            });
            if cancel.is_cancelled() {
                return Err(cancelled_marker());
            }
            Ok(())
        };
        let bytes = match entry.archive {
            ArchiveKind::Raw => {
                let target = content.join(format!("{}.sf2", entry.id));
                fs::rename(&download, &target).map_err(|error| storage(&target, &error))?;
                entry.size
            }
            ArchiveKind::Zip => {
                archive::unzip(&download, &content, &mut budget, &mut step)
                    .map_err(|error| interrupted(cancel, error))?;
                budget.written()
            }
            ArchiveKind::SevenZip => {
                archive::unseven(&download, &content, &mut budget, &mut step)
                    .map_err(|error| interrupted(cancel, error))?;
                budget.written()
            }
        };
        if fs::symlink_metadata(&download).is_ok() {
            fs::remove_file(&download).map_err(|error| storage(&download, &error))?;
        }
        cancel.check()?;
        let validated = self.validate(&content)?;
        progress(InstallProgress {
            stage: InstallStage::Registering,
            done: entry.size,
            total: entry.size,
        });
        cancel.check()?;
        let finished = Finished {
            id: Some(entry.id.clone()),
            name: entry.name.clone(),
            format: Some(entry.format),
            source: BankSource::Catalogue,
            bytes,
        };
        Ok(self.finish(staging, &content, finished, validated)?)
    }

    pub fn assign(&mut self, family: InstrumentFamily, bank: BankRef) -> Result<(), BankError> {
        let _lock = self.locked()?;
        self.refresh()?;
        if let BankRef::Installed(id) = &bank
            && self.stored(id).is_none()
        {
            return Err(BankError::NotInstalled { id: id.to_string() });
        }
        let previous = self.preferences.families.insert(family, bank);
        if let Err(error) = store::write(&self.root.join(PREFERENCES_NAME), &self.preferences) {
            match previous {
                Some(previous) => self.preferences.families.insert(family, previous),
                None => self.preferences.families.remove(&family),
            };
            return Err(error);
        }
        Ok(())
    }

    pub fn assignments(&self) -> BTreeMap<InstrumentFamily, BankRef> {
        self.preferences.families.clone()
    }

    fn usable(&self, bank: &BankRef) -> Result<Option<Usable>, FallbackReason> {
        let BankRef::Installed(id) = bank else {
            return Ok(None);
        };
        let stored = self.stored(id).ok_or(FallbackReason::NotInstalled)?;
        let path = self.bank_path(stored).ok_or(FallbackReason::FilesMissing)?;
        if fs::symlink_metadata(&path).is_err() {
            return Err(FallbackReason::FilesMissing);
        }
        Ok(Some(Usable {
            path,
            format: stored.format.format(),
        }))
    }

    pub fn resolve(&self, score: &Score) -> SoundPlan {
        let mut tracks = Vec::with_capacity(score.tracks.len());
        for track in &score.tracks {
            let family = InstrumentFamily::of(
                track.instrument.kind,
                track.instrument.set,
                track.instrument.midi_program,
            );
            let mut fallback: Option<Fallback> = None;
            let mut wanted: Vec<BankRef> = Vec::new();
            if let Some(chosen) = score
                .sound_overrides
                .iter()
                .find(|item| item.track == track.id)
            {
                match BankRef::parse(&chosen.bank) {
                    Ok(bank) => wanted.push(bank),
                    Err(_) => {
                        fallback = Some(Fallback {
                            wanted: chosen.bank.clone(),
                            reason: FallbackReason::BadName,
                        });
                    }
                }
            }
            if let Some(preferred) = self.preferences.families.get(&family) {
                wanted.push(preferred.clone());
            }
            let mut bank = BankRef::BuiltIn;
            for candidate in wanted {
                match self.usable(&candidate) {
                    Ok(_) => {
                        bank = candidate;
                        break;
                    }
                    Err(reason) => {
                        if fallback.is_none() {
                            fallback = Some(Fallback {
                                wanted: candidate.to_string(),
                                reason,
                            });
                        }
                    }
                }
            }
            tracks.push(TrackSound {
                track: track.id,
                family,
                bank,
                fallback,
            });
        }
        SoundPlan { tracks }
    }

    pub fn load_voices(&self, plan: &SoundPlan, rate: SampleRate) -> LoadedVoices {
        let mut loaded: HashMap<BankId, Result<LoadedBank, BankError>> = HashMap::new();
        let mut voices: Vec<Box<dyn Synth>> = Vec::with_capacity(plan.tracks.len());
        let mut backends = Vec::with_capacity(plan.tracks.len());
        for (index, sound) in plan.tracks.iter().enumerate() {
            let mut fallback = sound.fallback.clone();
            let mut chosen: Option<ChosenVoice> = None;
            if let BankRef::Installed(id) = &sound.bank {
                let bank = loaded.entry(id.clone()).or_insert_with(|| {
                    let usable = self
                        .usable(&sound.bank)
                        .map_err(|reason| BankError::Invalid {
                            reason: Fallback {
                                wanted: id.to_string(),
                                reason,
                            }
                            .to_string(),
                        })?;
                    let usable =
                        usable.ok_or_else(|| BankError::NotInstalled { id: id.to_string() })?;
                    load_bank(usable.format, &usable.path).map(|bank| LoadedBank {
                        format: usable.format,
                        bank,
                    })
                });
                match bank {
                    Ok(loaded) => {
                        chosen = Some(ChosenVoice {
                            synth: loaded.bank.instantiate(rate),
                            format: loaded.format,
                            name: id.to_string(),
                        });
                    }
                    Err(error) => {
                        fallback = Some(Fallback {
                            wanted: id.to_string(),
                            reason: FallbackReason::Unloadable {
                                message: error.to_string(),
                            },
                        });
                    }
                }
            }
            let voice = chosen.unwrap_or_else(|| ChosenVoice {
                synth: Box::new(BuiltInSynth::new(rate)),
                format: BankFormat::BuiltIn,
                name: BankRef::BuiltIn.to_string(),
            });
            voices.push(voice.synth);
            backends.push(TrackBackend {
                track: index,
                format: voice.format,
                bank: voice.name,
                fallback: fallback.map(|fallback| fallback.to_string()),
            });
        }
        LoadedVoices { voices, backends }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTRUMENT: &[u8] = b"<region> sample=a-first.wav\n<region> sample=b.wav\n";

    struct Written {
        name: &'static str,
        bytes: usize,
    }

    struct Counted {
        outcome: Result<InstalledBank, BankError>,
        copied: Vec<PathBuf>,
    }

    fn import_counting(sample_bytes: usize) -> Counted {
        let dir = tempfile::tempdir().expect("scratch");
        let root = dir.path().join("bank");
        fs::create_dir_all(&root).expect("folder");
        let files = [
            Written {
                name: "a.sfz",
                bytes: 300,
            },
            Written {
                name: "a-first.wav",
                bytes: sample_bytes,
            },
            Written {
                name: "b.wav",
                bytes: sample_bytes,
            },
            Written {
                name: "unrelated.txt",
                bytes: 50,
            },
        ];
        for file in files {
            let mut data = vec![b' '; file.bytes];
            if file.name.ends_with(".sfz") {
                data[..INSTRUMENT.len()].copy_from_slice(INSTRUMENT);
            }
            fs::write(root.join(file.name), data).expect("file");
        }
        let limits = ImportLimits {
            total_bytes: 1_000,
            file_bytes: 600,
            ..ImportLimits::DEFAULT
        };
        let mut manager = BankManager::with_catalogue(&dir.path().join("data"), Vec::new())
            .expect("opens")
            .with_limits(limits);
        copy::take_copied();
        let outcome = manager.import(&root);
        Counted {
            outcome,
            copied: copy::take_copied(),
        }
    }

    #[test]
    fn an_over_budget_folder_is_refused_before_any_bank_file_is_copied() {
        let within = import_counting(100);
        assert!(within.outcome.is_ok(), "{:?}", within.outcome);
        assert_eq!(
            within.copied,
            [
                PathBuf::from("a-first.wav"),
                PathBuf::from("a.sfz"),
                PathBuf::from("b.wav")
            ]
        );
        let over = import_counting(400);
        assert_eq!(
            over.outcome,
            Err(refuse(UnsafeBank::BankTooLarge { limit_mib: 0 }))
        );
        assert_eq!(over.copied, Vec::<PathBuf>::new());
    }
}
