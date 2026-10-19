use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use crate::sfz::limits::{KIB, MIB, SfzLimits};
use crate::sfz::listing::{IgnoreReason, Listing, SfzScope};
use crate::sfz::paths::{BankFs, PathProblem, ReadProblem, read_capped, refusal};
use crate::{BankError, UnsafeBank};

const BYTE_ORDER_MARK: char = '\u{feff}';
const DEFINE: &str = "#define";
const INCLUDE: &str = "#include";
const DIRECTIVE: char = '#';
const VARIABLE: char = '$';
const QUOTE: char = '"';

pub(crate) enum Opened {
    Text { key: PathBuf, bytes: Vec<u8> },
    Missing,
    Outside,
    TooLarge,
    Refused(PathProblem),
}

pub(crate) trait IncludeSource {
    fn include(&mut self, root_dir: &Path, including_dir: &Path, spec: &str) -> Opened;
}

pub(crate) struct FsIncludes<'a> {
    pub(crate) fs: &'a mut BankFs,
    pub(crate) cap: u64,
}

impl IncludeSource for FsIncludes<'_> {
    fn include(&mut self, root_dir: &Path, including_dir: &Path, spec: &str) -> Opened {
        let resolved = match self.fs.resolve(root_dir, spec) {
            Err(PathProblem::Missing) => self.fs.resolve(including_dir, spec),
            other => other,
        };
        match resolved {
            Ok(path) => match read_capped(&path, self.cap) {
                Ok(bytes) => Opened::Text { key: path, bytes },
                Err(ReadProblem::Missing) => Opened::Missing,
                Err(ReadProblem::TooLarge) => Opened::TooLarge,
            },
            Err(PathProblem::Outside) => Opened::Outside,
            Err(problem @ (PathProblem::Exhausted | PathProblem::Lookups)) => {
                Opened::Refused(problem)
            }
            Err(PathProblem::Missing | PathProblem::Rejected) => Opened::Missing,
        }
    }
}

pub(crate) struct MemoryIncludes<'a> {
    pub(crate) files: &'a BTreeMap<String, String>,
    pub(crate) cap: u64,
}

impl IncludeSource for MemoryIncludes<'_> {
    fn include(&mut self, _root_dir: &Path, _including_dir: &Path, spec: &str) -> Opened {
        let name = spec.replace('\\', "/");
        match self.files.get(&name) {
            Some(text) if text.len() as u64 > self.cap => Opened::TooLarge,
            Some(text) => Opened::Text {
                key: PathBuf::from(name),
                bytes: text.as_bytes().to_vec(),
            },
            None => Opened::Missing,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Expanded {
    pub(crate) text: String,
    pub(crate) included: Vec<PathBuf>,
    pub(crate) listing: Listing,
    pub(crate) read_bytes: u64,
    pub(crate) steps: u64,
}

pub(crate) struct MeasuredExpansion {
    pub(crate) result: Result<Expanded, BankError>,
    pub(crate) steps: u64,
}

fn unsafe_bank(reason: UnsafeBank) -> BankError {
    BankError::Unsafe { reason }
}

pub(crate) fn decode_text(bytes: &[u8]) -> Option<&str> {
    let text = std::str::from_utf8(bytes).ok()?;
    Some(text.strip_prefix(BYTE_ORDER_MARK).unwrap_or(text))
}

pub(crate) struct Stripped {
    pub(crate) text: String,
    pub(crate) scanned: u64,
}

pub(crate) fn strip_comments(text: &str) -> Stripped {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut scanned: u64 = 0;
    let mut index = 0;
    let mut copied = 0;
    while index + 1 < bytes.len() {
        scanned += 1;
        match (bytes[index], bytes[index + 1]) {
            (b'/', b'/') => {
                out.push_str(&text[copied..index]);
                let end = text[index..]
                    .find(['\n', '\r'])
                    .map_or(text.len(), |end| index + end);
                scanned += (end - index) as u64;
                index = end;
                copied = index;
            }
            (b'/', b'*') => {
                out.push_str(&text[copied..index]);
                match text[index + 2..].find("*/") {
                    Some(end) => {
                        out.push(' ');
                        scanned += (end + 2) as u64;
                        index += 2 + end + 2;
                    }
                    None => {
                        scanned += (text.len() - index) as u64;
                        index = text.len();
                    }
                }
                copied = index;
            }
            _ => index += 1,
        }
    }
    out.push_str(&text[copied..]);
    Stripped { text: out, scanned }
}

fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.split('\n')
        .flat_map(|line| line.split('\r'))
        .filter(|line| !line.is_empty())
}

fn is_name_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

struct Defines {
    values: HashMap<String, String>,
    lengths: Vec<bool>,
    longest: usize,
    count: u64,
    bytes: u64,
}

pub(crate) struct Preprocessor<'a, S: IncludeSource> {
    source: &'a mut S,
    limits: &'a SfzLimits,
    root_dir: PathBuf,
    defines: Defines,
    includes: u64,
    stack: Vec<PathBuf>,
    out: Expanded,
    in_path: bool,
}

impl<'a, S: IncludeSource> Preprocessor<'a, S> {
    pub(crate) fn new(source: &'a mut S, limits: &'a SfzLimits, root_dir: &Path) -> Self {
        Preprocessor {
            source,
            limits,
            root_dir: root_dir.to_path_buf(),
            defines: Defines {
                values: HashMap::new(),
                lengths: Vec::new(),
                longest: 0,
                count: 0,
                bytes: 0,
            },
            includes: 0,
            stack: Vec::new(),
            out: Expanded::default(),
            in_path: false,
        }
    }

    pub(crate) fn run(self, key: &Path, text: &str) -> Result<Expanded, BankError> {
        self.run_measured(key, text).result
    }

    pub(crate) fn run_measured(mut self, key: &Path, text: &str) -> MeasuredExpansion {
        let outcome = self.count_read(text.len()).and_then(|()| {
            self.stack.push(key.to_path_buf());
            self.expand(text, key, 0)
        });
        let steps = self.out.steps;
        MeasuredExpansion {
            result: outcome.map(|()| self.out),
            steps,
        }
    }

    fn count_read(&mut self, bytes: usize) -> Result<(), BankError> {
        self.out.read_bytes = self.out.read_bytes.saturating_add(bytes as u64);
        if self.out.read_bytes > self.limits.read_bytes {
            return Err(unsafe_bank(UnsafeBank::TextReadTooLarge {
                limit_mib: self.limits.read_bytes / MIB,
            }));
        }
        Ok(())
    }

    fn reserve(&self, extra: usize) -> Result<(), BankError> {
        let total = self.out.text.len() as u64 + extra as u64;
        if self.in_path && total > self.limits.path_bytes as u64 {
            return Err(unsafe_bank(UnsafeBank::PathTooLong {
                limit: self.limits.path_bytes as u64,
            }));
        }
        if total > self.limits.expanded_bytes {
            return Err(unsafe_bank(UnsafeBank::ExpandedTextTooLarge {
                limit_mib: self.limits.expanded_bytes / MIB,
            }));
        }
        Ok(())
    }

    fn push(&mut self, piece: &str) -> Result<(), BankError> {
        self.reserve(piece.len())?;
        self.out.steps = self.out.steps.saturating_add(piece.len() as u64);
        self.out.text.push_str(piece);
        Ok(())
    }

    fn substitute(&mut self, text: &str) -> Result<(), BankError> {
        let mut rest = text;
        while let Some(at) = rest.find(VARIABLE) {
            self.push(&rest[..at])?;
            let after = &rest[at + 1..];
            let window = &after.as_bytes()[..after.len().min(self.defines.longest)];
            let run = window
                .iter()
                .take_while(|byte| is_name_char(char::from(**byte)))
                .count();
            self.out.steps = self.out.steps.saturating_add(run as u64 + 1);
            let mut found = None;
            for length in (1..=run).rev() {
                if !self.defines.lengths.get(length).copied().unwrap_or(false) {
                    continue;
                }
                self.out.steps = self.out.steps.saturating_add(length as u64);
                if let Some(value) = self.defines.values.get(&after[..length]) {
                    found = Some((length, value.clone()));
                    break;
                }
            }
            match found {
                Some((length, value)) => {
                    self.push(&value)?;
                    rest = &after[length..];
                }
                None => {
                    self.push("$")?;
                    rest = after;
                }
            }
        }
        self.push(rest)
    }

    fn define(&mut self, line: &str) {
        let body = line[DEFINE.len()..].trim_start();
        let name_end = body.find(char::is_whitespace).unwrap_or(body.len());
        let (name, value) = body.split_at(name_end);
        let value = value.trim();
        let Some(bare) = name.strip_prefix(VARIABLE) else {
            self.out
                .listing
                .note(DEFINE, SfzScope::File, IgnoreReason::UnknownDirective);
            return;
        };
        if bare.is_empty()
            || bare.len() > self.limits.define_name_bytes
            || !bare.chars().all(is_name_char)
        {
            self.out
                .listing
                .note(DEFINE, SfzScope::File, IgnoreReason::UnknownDirective);
            return;
        }
        self.defines.count += 1;
        self.defines.bytes += (bare.len() + value.len()) as u64;
        self.defines.longest = self.defines.longest.max(bare.len());
        if self.defines.lengths.len() <= bare.len() {
            self.defines.lengths.resize(bare.len() + 1, false);
        }
        if let Some(slot) = self.defines.lengths.get_mut(bare.len()) {
            *slot = true;
        }
        self.defines
            .values
            .insert(bare.to_owned(), value.to_owned());
    }

    fn check_defines(&self) -> Result<(), BankError> {
        if self.defines.count > self.limits.defines {
            return Err(unsafe_bank(UnsafeBank::TooManyDefines {
                limit: self.limits.defines,
            }));
        }
        if self.defines.bytes > self.limits.define_bytes {
            return Err(unsafe_bank(UnsafeBank::DefineTextTooLarge {
                limit_kib: self.limits.define_bytes / KIB,
            }));
        }
        Ok(())
    }

    fn include(&mut self, spec: &str, including: &Path, depth: u64) -> Result<(), BankError> {
        let mut resolved = String::new();
        std::mem::swap(&mut resolved, &mut self.out.text);
        self.in_path = true;
        let substituted = self.substitute(spec);
        self.in_path = false;
        std::mem::swap(&mut resolved, &mut self.out.text);
        substituted?;
        self.count_read(resolved.len())?;
        if resolved.contains(VARIABLE) {
            self.out
                .listing
                .note(INCLUDE, SfzScope::File, IgnoreReason::UnresolvedVariable);
            return Ok(());
        }
        if depth + 1 > self.limits.include_depth {
            return Err(unsafe_bank(UnsafeBank::IncludeTooDeep {
                limit: self.limits.include_depth,
            }));
        }
        self.includes += 1;
        if self.includes > self.limits.includes {
            return Err(unsafe_bank(UnsafeBank::TooManyIncludes {
                limit: self.limits.includes,
            }));
        }
        let including_dir = including.parent().unwrap_or(including).to_path_buf();
        match self
            .source
            .include(&self.root_dir, &including_dir, &resolved)
        {
            Opened::Text { key, bytes } => {
                if self.stack.contains(&key) {
                    return Err(unsafe_bank(UnsafeBank::IncludeCycle));
                }
                self.count_read(bytes.len())?;
                let Some(text) = decode_text(&bytes) else {
                    self.out
                        .listing
                        .note(INCLUDE, SfzScope::File, IgnoreReason::MissingInclude);
                    return Ok(());
                };
                let text = text.to_owned();
                drop(bytes);
                self.out.included.push(key.clone());
                self.stack.push(key.clone());
                self.expand(&text, &key, depth + 1)?;
                self.stack.pop();
                self.push("\n")
            }
            Opened::Missing => {
                self.out
                    .listing
                    .note(INCLUDE, SfzScope::File, IgnoreReason::MissingInclude);
                Ok(())
            }
            Opened::Outside => {
                self.out
                    .listing
                    .note(INCLUDE, SfzScope::File, IgnoreReason::PathOutsideBank);
                Ok(())
            }
            Opened::TooLarge => Err(unsafe_bank(UnsafeBank::SfzFileTooLarge {
                limit_kib: self.limits.file_bytes / KIB,
            })),
            Opened::Refused(problem) => Err(refusal(problem, self.limits).unwrap_or(unsafe_bank(
                UnsafeBank::ListedEntries {
                    limit: self.limits.listed_entries,
                },
            ))),
        }
    }

    fn include_line(&mut self, line: &str, key: &Path, depth: u64) -> Result<(), BankError> {
        let mut rest = line;
        while let Some(at) = rest.find(INCLUDE) {
            self.substitute(&rest[..at])?;
            let after = rest[at + INCLUDE.len()..].trim_start();
            let (spec, remainder) = match after.strip_prefix(QUOTE) {
                Some(quoted) => match quoted.find(QUOTE) {
                    Some(end) => (&quoted[..end], &quoted[end + 1..]),
                    None => (quoted, ""),
                },
                None => {
                    let end = after.find(char::is_whitespace).unwrap_or(after.len());
                    (&after[..end], &after[end..])
                }
            };
            self.include(spec, key, depth)?;
            rest = remainder;
        }
        self.substitute(rest)?;
        self.push("\n")
    }

    fn expand(&mut self, text: &str, key: &Path, depth: u64) -> Result<(), BankError> {
        let stripped = strip_comments(text);
        self.out.steps = self.out.steps.saturating_add(stripped.scanned);
        for line in lines(&stripped.text) {
            if line.len() as u64 > self.limits.line_bytes {
                return Err(unsafe_bank(UnsafeBank::LineTooLong {
                    limit_kib: self.limits.line_bytes / KIB,
                }));
            }
            let trimmed = line.trim_start();
            if trimmed.starts_with(DEFINE) {
                self.define(trimmed);
                self.check_defines()?;
            } else if line.contains(INCLUDE) {
                self.include_line(line, key, depth)?;
            } else if trimmed.starts_with(DIRECTIVE) {
                let name = trimmed.split(char::is_whitespace).next().unwrap_or("#");
                self.out
                    .listing
                    .note(name, SfzScope::File, IgnoreReason::UnknownDirective);
            } else {
                self.substitute(line)?;
                self.push("\n")?;
            }
        }
        Ok(())
    }
}
