use crate::BankFormat;
use crate::manager::id::BankId;

const CC0: &str = include_str!("../../catalogue/cc0-1.0.txt");
const CC_BY: &str = include_str!("../../catalogue/cc-by-4.0.txt");
const CC_BY_SA: &str = include_str!("../../catalogue/cc-by-sa-4.0.txt");
const MUSESCORE_GENERAL: &str = include_str!("../../catalogue/musescore-general-licence.md");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArchiveKind {
    Raw,
    Zip,
    SevenZip,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CatalogueEntry {
    pub id: BankId,
    pub name: String,
    pub format: BankFormat,
    pub archive: ArchiveKind,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub licence_name: String,
    pub licence_url: String,
    pub licence_text: String,
    pub attribution: Option<String>,
    pub covers: String,
}

struct Listing {
    id: &'static str,
    name: &'static str,
    format: BankFormat,
    archive: ArchiveKind,
    url: &'static str,
    sha256: &'static str,
    size: u64,
    licence_name: &'static str,
    licence_url: &'static str,
    licence_text: &'static str,
    attribution: Option<&'static str>,
    covers: &'static str,
}

const LISTINGS: [Listing; 9] = [
    Listing {
        id: "musescore-general",
        name: "MuseScore General 0.2",
        format: BankFormat::Sf2,
        archive: ArchiveKind::Raw,
        url: "https://ftp.osuosl.org/pub/musescore/soundfont/MuseScore_General/MuseScore_General.sf2",
        sha256: "ee51d2c4b1525e70f19a45909c4fd7a2e26d91d115fa89dbf5a6bc413d8b9bf3",
        size: 215_614_036,
        licence_name: "MIT",
        licence_url: "https://ftp.osuosl.org/pub/musescore/soundfont/MuseScore_General/MuseScore_General_License.md",
        licence_text: MUSESCORE_GENERAL,
        attribution: Some(
            "FluidR3 by Frank Wen, Copyright (c) 2000-02, 2008. FluidR3Mono by Michael Cowgill, Copyright (c) 2014-17. Adaptation for MuseScore_General.sf2 by S. Christian Collins, Copyright (c) 2018-19. Temple Blocks by Ethan Winer, Copyright (c) 2002. Drumline Cymbals by Michael Schorsch, Copyright (c) 2016. Shared under the MIT licence.",
        ),
        covers: "every General MIDI instrument and drum kit",
    },
    Listing {
        id: "spanish-guitar",
        name: "FreePats Spanish Classical Guitar",
        format: BankFormat::Sfz,
        archive: ArchiveKind::Zip,
        url: "https://github.com/freepats/spanish-classical-guitar/releases/download/v1.0.0/SpanishClassicalGuitar-20190618.zip",
        sha256: "173f775222991061291767724d57c7c21242a3c3254407b266aabd548df23dbb",
        size: 4_943_846,
        licence_name: "CC0-1.0",
        licence_url: "https://github.com/freepats/spanish-classical-guitar/blob/main/LICENSE",
        licence_text: CC0,
        attribution: None,
        covers: "nylon guitar",
    },
    Listing {
        id: "fsbs-clean",
        name: "FreePats FSBS Electric Guitar Clean",
        format: BankFormat::Sfz,
        archive: ArchiveKind::SevenZip,
        url: "https://github.com/freepats/electric-guitar-FSBS-clean/releases/download/2026-08-07/EGuitarFSBS-clean-bridge-small-SFZ%2BFLAC-20260807.7z",
        sha256: "07c8cdf94dc9b4875ff6c43e51440d84f939a45530060c0761093a56d4a1f032",
        size: 3_096_261,
        licence_name: "CC0-1.0",
        licence_url: "https://github.com/freepats/electric-guitar-FSBS-clean/blob/main/LICENSE.txt",
        licence_text: CC0,
        attribution: None,
        covers: "clean electric guitar",
    },
    Listing {
        id: "finger-bass-yr",
        name: "FreePats Finger Bass YR",
        format: BankFormat::Sfz,
        archive: ArchiveKind::SevenZip,
        url: "https://github.com/freepats/electric-bass-YR/releases/download/2019-09-30/FingerBassYR-SFZ%2BFLAC-20190930.7z",
        sha256: "36f87ec9eb086ef25050312522aacf71213c582f46989e0c5a12681304596587",
        size: 3_252_347,
        licence_name: "CC0-1.0",
        licence_url: "https://github.com/freepats/electric-bass-YR/blob/main/LICENSE.txt",
        licence_text: CC0,
        attribution: None,
        covers: "electric bass (finger)",
    },
    Listing {
        id: "muldjordkit",
        name: "FreePats MuldjordKit",
        format: BankFormat::Sf2,
        archive: ArchiveKind::SevenZip,
        url: "https://github.com/freepats/muldjordkit/releases/download/2020-10-18/MuldjordKit-SF2-20201018.7z",
        sha256: "aef4c9e0a96f9a899a7838f65ff07565f6af247bbb4ff31e24dba3331a671a96",
        size: 54_877_940,
        licence_name: "CC-BY-4.0",
        licence_url: "https://github.com/freepats/muldjordkit/blob/main/LICENSE.txt",
        licence_text: CC_BY,
        attribution: Some(
            "MuldjordKit by Lars Muldjord (www.muldjord.com), FreePats version for the FreePats project. Drum samples provided by DrumGizmo.org. Licensed under the Creative Commons Attribution 4.0 International License (https://creativecommons.org/licenses/by/4.0/).",
        ),
        covers: "acoustic drum kit",
    },
    Listing {
        id: "emilyguitar",
        name: "Karoryfer Emilyguitar v1.001",
        format: BankFormat::Sfz,
        archive: ArchiveKind::Zip,
        url: "https://github.com/sfzinstruments/karoryfer.emilyguitar/releases/download/v1.001/Karoryfer.Emilyguitar.v1.001.zip",
        sha256: "ffef3b28972763820bee73660a25cf47ea6e13b184342fd5d325a5631598cacb",
        size: 103_484_350,
        licence_name: "CC0-1.0",
        licence_url: "https://github.com/sfzinstruments/karoryfer.emilyguitar/blob/master/LICENSE",
        licence_text: CC0,
        attribution: None,
        covers: "acoustic guitar",
    },
    Listing {
        id: "lately-bass",
        name: "FreePats Lately Bass",
        format: BankFormat::Sfz,
        archive: ArchiveKind::SevenZip,
        url: "https://github.com/freepats/lately-bass/releases/download/2024-04-09/LatelyBass-SFZ%2BFLAC-20240409.7z",
        sha256: "3cb6d4081b39cea265011a21f05a424fc72845abfd17d7409ac16acc7122a3e0",
        size: 1_181_315,
        licence_name: "CC0-1.0",
        licence_url: "https://github.com/freepats/lately-bass/blob/main/LICENSE.txt",
        licence_text: CC0,
        attribution: None,
        covers: "electric bass",
    },
    Listing {
        id: "sams-sonor",
        name: "Sam's Sonor Force",
        format: BankFormat::Sfz,
        archive: ArchiveKind::Zip,
        url: "https://github.com/sfzinstruments/SamsSonor/archive/ef3e32058924ed1c3335e86d03094d3f310a9104.zip",
        sha256: "fcbca4dd398666f2908995218394c3af938817580304316f618dead41eaaff2c",
        size: 36_203_868,
        licence_name: "CC-BY-SA-4.0",
        licence_url: "https://github.com/sfzinstruments/SamsSonor/blob/master/LICENSE",
        licence_text: CC_BY_SA,
        attribution: Some(
            "Sam's Sonor by Sam Greene, mapped to SFZ by kinwie (sfzinstruments). Licensed under the Creative Commons Attribution-ShareAlike 4.0 International License (https://creativecommons.org/licenses/by-sa/4.0/).",
        ),
        covers: "drum kit",
    },
    Listing {
        id: "meatbass",
        name: "Karoryfer Meatbass v1.001",
        format: BankFormat::Sfz,
        archive: ArchiveKind::Zip,
        url: "https://github.com/sfzinstruments/karoryfer.meatbass/releases/download/v1.001/Karoryfer.Meatbass.v1.001.zip",
        sha256: "bc053061d4f39fb76ba56bc1f323efa228d1f63757c331738812479bcf04fe96",
        size: 255_425_351,
        licence_name: "CC0-1.0",
        licence_url: "https://github.com/sfzinstruments/karoryfer.meatbass/blob/master/LICENSE",
        licence_text: CC0,
        attribution: None,
        covers: "upright bass",
    },
];

pub fn default_catalogue() -> Vec<CatalogueEntry> {
    LISTINGS
        .iter()
        .filter_map(|listing| {
            BankId::new(listing.id).ok().map(|id| CatalogueEntry {
                id,
                name: listing.name.to_owned(),
                format: listing.format,
                archive: listing.archive,
                url: listing.url.to_owned(),
                sha256: listing.sha256.to_owned(),
                size: listing.size,
                licence_name: listing.licence_name.to_owned(),
                licence_url: listing.licence_url.to_owned(),
                licence_text: listing.licence_text.to_owned(),
                attribution: listing.attribution.map(str::to_owned),
                covers: listing.covers.to_owned(),
            })
        })
        .collect()
}
