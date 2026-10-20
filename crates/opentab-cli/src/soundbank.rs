use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use opentab_audio::{
    ArchiveKind, BankManager, BankSource, CatalogueEntry, InstallStage, InstalledBank,
};
use serde::Serialize;

use crate::bank_store;
use crate::cli::SoundbankAction;
use crate::error::CliError;
use crate::families::family_name;
use crate::output::{self, Rendered};
use crate::sound::format_name;
use crate::text::escape_controls;

#[derive(Serialize)]
pub struct BankWire {
    id: String,
    name: String,
    format: &'static str,
    source: &'static str,
    bytes: u64,
    path: String,
    instruments: usize,
}

#[derive(Serialize)]
pub struct ProblemWire {
    path: String,
    moved_to: Option<String>,
    message: String,
}

#[derive(Serialize)]
pub struct Listing {
    banks: Vec<BankWire>,
    assignments: BTreeMap<String, String>,
    problems: Vec<ProblemWire>,
}

#[derive(Serialize)]
pub struct Imported {
    imported: BankWire,
}

#[derive(Serialize)]
pub struct Installed {
    installed: BankWire,
}

#[derive(Serialize)]
pub struct Removed {
    removed: String,
}

#[derive(Serialize)]
pub struct Assignment {
    instrument: String,
    bank: String,
}

#[derive(Serialize)]
pub struct Assigned {
    assigned: Assignment,
}

#[derive(Serialize)]
pub struct CatalogueItem {
    id: String,
    name: String,
    format: &'static str,
    archive: &'static str,
    size: u64,
    licence: String,
    covers: String,
    installed: bool,
}

#[derive(Serialize)]
pub struct Catalogued {
    catalogue: Vec<CatalogueItem>,
}

fn source_name(source: BankSource) -> &'static str {
    match source {
        BankSource::Imported => "imported",
        BankSource::Catalogue => "catalogue",
    }
}

fn archive_name(archive: ArchiveKind) -> &'static str {
    match archive {
        ArchiveKind::Raw => "raw",
        ArchiveKind::Zip => "zip",
        ArchiveKind::SevenZip => "7z",
    }
}

fn bank_wire(bank: &InstalledBank) -> BankWire {
    BankWire {
        id: bank.id.to_string(),
        name: bank.name.clone(),
        format: format_name(bank.format),
        source: source_name(bank.source),
        bytes: bank.bytes,
        path: bank.path.display().to_string(),
        instruments: bank.instruments.len(),
    }
}

fn bank_line(bank: &InstalledBank) -> String {
    format!(
        "{}  {}  {}  {} bytes  {}",
        escape_controls(bank.id.as_str()),
        escape_controls(&bank.name),
        format_name(bank.format),
        bank.bytes,
        source_name(bank.source)
    )
}

fn list(manager: &BankManager) -> Rendered<Listing> {
    let banks = manager.list();
    let assignments: BTreeMap<String, String> = manager
        .assignments()
        .into_iter()
        .map(|(family, bank)| (family_name(family), bank.to_string()))
        .collect();
    let problems: Vec<ProblemWire> = manager
        .problems()
        .iter()
        .map(|problem| ProblemWire {
            path: problem.path.display().to_string(),
            moved_to: problem
                .moved_to
                .as_ref()
                .map(|path| path.display().to_string()),
            message: problem.message.clone(),
        })
        .collect();
    let mut lines = Vec::new();
    if banks.is_empty() {
        lines.push("no sound banks installed; the built-in synthesizer is used".to_string());
    } else {
        lines.push(format!("{} installed sound bank(s):", banks.len()));
        lines.extend(banks.iter().map(|bank| format!("  {}", bank_line(bank))));
    }
    for (family, bank) in &assignments {
        lines.push(format!("assigned: {family} -> {bank}"));
    }
    for problem in &problems {
        lines.push(format!(
            "problem: {} ({})",
            escape_controls(&problem.message),
            escape_controls(&problem.path)
        ));
    }
    Rendered {
        wire: Listing {
            banks: banks.iter().map(bank_wire).collect(),
            assignments,
            problems,
        },
        human: lines.join("\n"),
    }
}

fn catalogue(manager: &BankManager) -> Rendered<Catalogued> {
    let installed: Vec<String> = manager
        .list()
        .iter()
        .map(|bank| bank.id.to_string())
        .collect();
    let items: Vec<CatalogueItem> = manager
        .catalogue()
        .iter()
        .map(|entry| item(entry, installed.contains(&entry.id.to_string())))
        .collect();
    let lines: Vec<String> = items
        .iter()
        .map(|item| {
            format!(
                "{}{}  {}  {}  {} bytes  {}  covers {}",
                escape_controls(&item.id),
                if item.installed { " (installed)" } else { "" },
                escape_controls(&item.name),
                item.format,
                item.size,
                escape_controls(&item.licence),
                escape_controls(&item.covers)
            )
        })
        .collect();
    Rendered {
        wire: Catalogued { catalogue: items },
        human: lines.join("\n"),
    }
}

fn item(entry: &CatalogueEntry, installed: bool) -> CatalogueItem {
    CatalogueItem {
        id: entry.id.to_string(),
        name: entry.name.clone(),
        format: format_name(entry.format),
        archive: archive_name(entry.archive),
        size: entry.size,
        licence: entry.licence_name.clone(),
        covers: entry.covers.clone(),
        installed,
    }
}

fn stage_name(stage: InstallStage) -> &'static str {
    match stage {
        InstallStage::Downloading => "downloading",
        InstallStage::Verifying => "verifying",
        InstallStage::Unpacking => "unpacking",
        InstallStage::Registering => "registering",
    }
}

pub fn run(
    catalogue_file: Option<&Path>,
    action: SoundbankAction,
    json: bool,
) -> Result<(), CliError> {
    let mut manager = bank_store::open(catalogue_file)?;
    match action {
        SoundbankAction::List => output::emit(&list(&manager), json),
        SoundbankAction::Catalogue => output::emit(&catalogue(&manager), json),
        SoundbankAction::Import { path } => {
            std::fs::symlink_metadata(&path)
                .map_err(|error| opentab_files::FileError::io(&path, &error))?;
            let bank = manager.import(&path)?;
            output::emit(
                &Rendered {
                    human: format!("imported {}", bank_line(&bank)),
                    wire: Imported {
                        imported: bank_wire(&bank),
                    },
                },
                json,
            )
        }
        SoundbankAction::Install { id } => {
            let mut stage: Option<InstallStage> = None;
            let bank = manager.install(&id, &mut |progress| {
                if !json && stage != Some(progress.stage) {
                    stage = Some(progress.stage);
                    let _ = writeln!(std::io::stderr(), "{}", stage_name(progress.stage));
                }
            })?;
            output::emit(
                &Rendered {
                    human: format!("installed {}", bank_line(&bank)),
                    wire: Installed {
                        installed: bank_wire(&bank),
                    },
                },
                json,
            )
        }
        SoundbankAction::Remove { id } => {
            manager.remove(&id)?;
            output::emit(
                &Rendered {
                    human: format!("removed {}", escape_controls(id.as_str())),
                    wire: Removed {
                        removed: id.to_string(),
                    },
                },
                json,
            )
        }
        SoundbankAction::Assign { instrument, bank } => {
            manager.assign(instrument, bank.clone())?;
            let name = family_name(instrument);
            output::emit(
                &Rendered {
                    human: format!("{name} will use {bank}"),
                    wire: Assigned {
                        assigned: Assignment {
                            instrument: name,
                            bank: bank.to_string(),
                        },
                    },
                },
                json,
            )
        }
    }
}
