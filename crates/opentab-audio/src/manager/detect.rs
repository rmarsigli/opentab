use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::manager::copy::{Listed, ListedKind, Survey, storage, survey};
use crate::manager::limits::ImportLimits;
use crate::{BankError, SF3_UNSUPPORTED, SIDECAR_NAME, SIDECAR_VERSION, UnsafeBank};

const SF2_EXTENSION: &str = "sf2";
const SF3_EXTENSION: &str = "sf3";
const SFZ_EXTENSION: &str = "sfz";
const PROGRAMS_PER_FAMILY: u8 = 8;
const DRUM_WORDS: [&str; 3] = ["drum", "kit", "perc"];

struct FamilyWords {
    words: &'static [&'static str],
    first: u8,
}

struct ProgramWord {
    first: u8,
    word: &'static str,
    program: u8,
}

const FAMILIES: [FamilyWords; 9] = [
    FamilyWords {
        words: &["sax", "oboe", "clarinet", "bassoon", "reed"],
        first: 64,
    },
    FamilyWords {
        words: &["bass"],
        first: 32,
    },
    FamilyWords {
        words: &["guitar", "gtr"],
        first: 24,
    },
    FamilyWords {
        words: &["piano", "keys"],
        first: 0,
    },
    FamilyWords {
        words: &["organ"],
        first: 16,
    },
    FamilyWords {
        words: &["violin", "viola", "cello", "strings"],
        first: 40,
    },
    FamilyWords {
        words: &["choir", "voice", "ensemble"],
        first: 48,
    },
    FamilyWords {
        words: &["trumpet", "trombone", "horn", "brass", "tuba"],
        first: 56,
    },
    FamilyWords {
        words: &["flute", "piccolo", "recorder", "pipe"],
        first: 72,
    },
];

const PROGRAM_WORDS: [ProgramWord; 20] = [
    ProgramWord {
        first: 24,
        word: "nylon",
        program: 24,
    },
    ProgramWord {
        first: 24,
        word: "classical",
        program: 24,
    },
    ProgramWord {
        first: 24,
        word: "spanish",
        program: 24,
    },
    ProgramWord {
        first: 24,
        word: "steel",
        program: 25,
    },
    ProgramWord {
        first: 24,
        word: "acoustic",
        program: 25,
    },
    ProgramWord {
        first: 24,
        word: "jazz",
        program: 26,
    },
    ProgramWord {
        first: 24,
        word: "clean",
        program: 27,
    },
    ProgramWord {
        first: 24,
        word: "mute",
        program: 28,
    },
    ProgramWord {
        first: 24,
        word: "overdrive",
        program: 29,
    },
    ProgramWord {
        first: 24,
        word: "crunch",
        program: 29,
    },
    ProgramWord {
        first: 24,
        word: "distort",
        program: 30,
    },
    ProgramWord {
        first: 24,
        word: "harmonic",
        program: 31,
    },
    ProgramWord {
        first: 32,
        word: "upright",
        program: 32,
    },
    ProgramWord {
        first: 32,
        word: "acoustic",
        program: 32,
    },
    ProgramWord {
        first: 32,
        word: "double",
        program: 32,
    },
    ProgramWord {
        first: 32,
        word: "finger",
        program: 33,
    },
    ProgramWord {
        first: 32,
        word: "pick",
        program: 34,
    },
    ProgramWord {
        first: 32,
        word: "fretless",
        program: 35,
    },
    ProgramWord {
        first: 32,
        word: "slap",
        program: 36,
    },
    ProgramWord {
        first: 32,
        word: "synth",
        program: 38,
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Detected {
    Sf2 { file: PathBuf },
    Sfz { root: PathBuf, files: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SidecarProgram {
    pub program: u8,
    pub file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SidecarRecord {
    pub version: u32,
    pub programs: Vec<SidecarProgram>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drums: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

#[derive(Default)]
struct Found {
    sfz: Vec<String>,
    sf2: Vec<PathBuf>,
    sf3: usize,
    unusable: Option<UnsafeBank>,
}

fn has_extension(path: &Path, wanted: &str) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(wanted))
}

fn portable_name(relative: &Path) -> String {
    relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<String>>()
        .join("/")
}

fn descend(survey: &Survey) -> PathBuf {
    let mut root = PathBuf::new();
    loop {
        let children: Vec<&Listed> = survey
            .entries
            .iter()
            .filter(|entry| entry.relative.parent() == Some(root.as_path()))
            .take(2)
            .collect();
        match children.as_slice() {
            [only] if only.kind == ListedKind::Folder => root = only.relative.clone(),
            _ => return root,
        }
    }
}

pub(crate) fn classify(survey: &Survey) -> Result<Detected, BankError> {
    let root = descend(survey);
    let mut found = Found::default();
    for entry in &survey.entries {
        if entry.kind == ListedKind::Folder {
            continue;
        }
        let Ok(inside) = entry.relative.strip_prefix(&root) else {
            continue;
        };
        if let Some(reason) = entry.kind.refusal() {
            if has_extension(inside, SF2_EXTENSION) || has_extension(inside, SFZ_EXTENSION) {
                found.unusable.get_or_insert(reason);
            }
        } else if has_extension(inside, SFZ_EXTENSION) {
            found.sfz.push(portable_name(inside));
        } else if has_extension(inside, SF2_EXTENSION) {
            found.sf2.push(survey.root.join(&entry.relative));
        } else if has_extension(inside, SF3_EXTENSION) {
            found.sf3 += 1;
        }
    }
    found.sfz.sort();
    found.sf2.sort();
    if !found.sfz.is_empty() {
        return Ok(Detected::Sfz {
            root: survey.root.join(root),
            files: found.sfz,
        });
    }
    if found.sf2.is_empty()
        && let Some(reason) = found.unusable
    {
        return Err(BankError::Unsafe { reason });
    }
    match found.sf2.as_slice() {
        [file] => Ok(Detected::Sf2 { file: file.clone() }),
        [] if found.sf3 > 0 => Err(BankError::Unsupported {
            message: SF3_UNSUPPORTED.to_owned(),
        }),
        [] => Err(BankError::Unsupported {
            message: String::from(
                "no SF2 SoundFont or SFZ instrument was found; OpenTab imports .sf2 files and folders or archives holding .sfz instruments",
            ),
        }),
        several => Err(BankError::Unsupported {
            message: format!(
                "this holds {} SF2 SoundFonts; import them one at a time",
                several.len()
            ),
        }),
    }
}

pub(crate) fn detect(content: &Path, limits: ImportLimits) -> Result<Detected, BankError> {
    classify(&survey(content, limits)?)
}

fn stem(file: &str) -> String {
    match file.rfind('.') {
        Some(dot) if !file[dot..].contains('/') => file[..dot].to_owned(),
        _ => file.to_owned(),
    }
}

pub(crate) fn instrument_files(files: &[String], names: &[String]) -> Vec<String> {
    files
        .iter()
        .filter(|file| names.contains(&stem(file)))
        .cloned()
        .collect()
}

fn is_drum(file: &str) -> bool {
    DRUM_WORDS.iter().any(|word| file.contains(word))
}

fn family_of(file: &str) -> Option<u8> {
    FAMILIES
        .iter()
        .find(|family| family.words.iter().any(|word| file.contains(word)))
        .map(|family| family.first)
}

pub fn sidecar_for(files: &[String]) -> SidecarRecord {
    let lowered: Vec<String> = files.iter().map(|file| file.to_lowercase()).collect();
    let drums = lowered.iter().position(|file| is_drum(file));
    let default = lowered.iter().position(|file| !is_drum(file));
    let mut programs = Vec::new();
    for family in &FAMILIES {
        let members: Vec<usize> = (0..files.len())
            .filter(|index| {
                !is_drum(&lowered[*index]) && family_of(&lowered[*index]) == Some(family.first)
            })
            .collect();
        let Some(first_member) = members.first() else {
            continue;
        };
        for program in family.first..family.first + PROGRAMS_PER_FAMILY {
            let chosen = members
                .iter()
                .copied()
                .find(|index| {
                    PROGRAM_WORDS.iter().any(|hint| {
                        hint.first == family.first
                            && hint.program == program
                            && lowered[*index].contains(hint.word)
                    })
                })
                .unwrap_or(*first_member);
            programs.push(SidecarProgram {
                program,
                file: files[chosen].clone(),
            });
        }
    }
    programs.sort_by_key(|entry| entry.program);
    SidecarRecord {
        version: SIDECAR_VERSION,
        programs,
        drums: drums.map(|index| files[index].clone()),
        default: default.map(|index| files[index].clone()),
    }
}

pub(crate) fn write_sidecar(root: &Path, files: &[String]) -> Result<bool, BankError> {
    let path = root.join(SIDECAR_NAME);
    if fs::symlink_metadata(&path).is_ok() {
        return Ok(false);
    }
    let text =
        serde_json::to_vec_pretty(&sidecar_for(files)).map_err(|error| BankError::Storage {
            message: error.to_string(),
        })?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| storage(&path, &error))?;
    file.write_all(&text)
        .and_then(|()| file.sync_all())
        .map_err(|error| storage(&path, &error))?;
    Ok(true)
}
