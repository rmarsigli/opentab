use std::collections::BTreeMap;

use opentab_audio::{
    ArchiveKind, BankManager, BankRef, BankSource, CatalogueEntry, InstalledBank, default_data_dir,
};
use opentab_model::InstrumentFamily;
use serde::Serialize;

use crate::args::{AssignArgs, InstallArgs, OpenArgs};
use crate::error::{ToolError, echo};
use crate::report::format_name;

#[derive(Serialize)]
pub struct BankWire {
    pub id: String,
    pub name: String,
    pub format: &'static str,
    pub source: &'static str,
    pub bytes: u64,
    pub path: String,
    pub instruments: usize,
}

#[derive(Serialize)]
pub struct ProblemWire {
    pub path: String,
    pub moved_to: Option<String>,
    pub message: String,
}

#[derive(Serialize)]
pub struct CatalogueItem {
    pub id: String,
    pub name: String,
    pub format: &'static str,
    pub archive: &'static str,
    pub size: u64,
    pub licence: String,
    pub covers: String,
    pub installed: bool,
}

#[derive(Serialize)]
pub struct Listing {
    pub banks: Vec<BankWire>,
    pub assignments: BTreeMap<String, String>,
    pub catalogue: Vec<CatalogueItem>,
    pub problems: Vec<ProblemWire>,
}

#[derive(Serialize)]
pub struct Imported {
    pub imported: BankWire,
}

#[derive(Serialize)]
pub struct Installed {
    pub installed: BankWire,
}

#[derive(Serialize)]
pub struct Assignment {
    pub instrument: String,
    pub bank: String,
}

#[derive(Serialize)]
pub struct Assigned {
    pub assigned: Assignment,
}

pub fn open_manager() -> Result<BankManager, ToolError> {
    let Some(root) = default_data_dir() else {
        return Err(ToolError::new(
            "sound_bank",
            "OpenTab cannot find a folder for its data; set the OPENTAB_DATA_DIR environment variable to one",
        ));
    };
    Ok(BankManager::open(&root)?)
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

pub fn family_name(family: InstrumentFamily) -> String {
    match serde_json::to_value(family) {
        Ok(serde_json::Value::String(name)) => name,
        _ => String::new(),
    }
}

fn parse_family(text: &str) -> Result<InstrumentFamily, ToolError> {
    let normal = text.to_ascii_lowercase().replace('-', "_");
    serde_json::from_value(serde_json::Value::String(normal)).map_err(|_| {
        ToolError::invalid_input(format!(
            "{} is not an instrument family; use one of piano, chromatic_percussion, organ, guitar, bass, strings, ensemble, brass, reed, pipe, synth_lead, synth_pad, synth_effects, ethnic, percussive, sound_effects, drums",
            echo(text)
        ))
    })
}

pub fn list() -> Result<Listing, ToolError> {
    let manager = open_manager()?;
    let banks = manager.list();
    let installed: Vec<String> = banks.iter().map(|bank| bank.id.to_string()).collect();
    Ok(Listing {
        banks: banks.iter().map(bank_wire).collect(),
        assignments: manager
            .assignments()
            .into_iter()
            .map(|(family, bank)| (family_name(family), bank.to_string()))
            .collect(),
        catalogue: manager
            .catalogue()
            .iter()
            .map(|entry| item(entry, installed.contains(&entry.id.to_string())))
            .collect(),
        problems: manager
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
            .collect(),
    })
}

pub fn import(args: OpenArgs) -> Result<Imported, ToolError> {
    let mut manager = open_manager()?;
    let bank = manager.import(std::path::Path::new(&args.path))?;
    Ok(Imported {
        imported: bank_wire(&bank),
    })
}

pub fn install(args: InstallArgs) -> Result<Installed, ToolError> {
    let id = opentab_audio::BankId::new(&args.id)?;
    let mut manager = open_manager()?;
    let bank = manager.install(&id, &mut |_| {})?;
    Ok(Installed {
        installed: bank_wire(&bank),
    })
}

pub fn assign(args: AssignArgs) -> Result<Assigned, ToolError> {
    let family = parse_family(&args.instrument)?;
    let bank = BankRef::parse(&args.bank)?;
    let mut manager = open_manager()?;
    manager.assign(family, bank.clone())?;
    Ok(Assigned {
        assigned: Assignment {
            instrument: family_name(family),
            bank: bank.to_string(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_names_are_case_and_dash_tolerant_and_round_trip() {
        assert_eq!(
            parse_family("Synth-Lead").unwrap(),
            InstrumentFamily::SynthLead
        );
        assert_eq!(family_name(InstrumentFamily::SynthLead), "synth_lead");
        assert_eq!(family_name(parse_family("drums").unwrap()), "drums");
        let error = parse_family("banjo").unwrap_err();
        assert_eq!(error.kind, "invalid_input");
        assert!(error.message.contains("\"banjo\""));
    }
}
