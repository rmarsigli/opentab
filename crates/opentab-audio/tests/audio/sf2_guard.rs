use opentab_audio::{BankError, BankShape, Sf2Bank, Sf2Limits, UnsafeBank};
use opentab_testkit::banks::{
    BypassRoute, bypass_bank, layered_sf2, padded_sf2, tiny_definition, write_sf2, zone_bomb,
};

use crate::sf2::{chunk_header, pdta_chunk};
use crate::support::tiny_bytes;

const PRESET_RECORD: usize = 38;
const PRESET_BAG: usize = 24;
const INSTRUMENT_RECORD: usize = 22;
const INSTRUMENT_BAG: usize = 20;
const BAG_RECORD: usize = 4;
const BAG_MODULATOR: usize = 2;
const SMALL_BOMB: u16 = 64;
const HOSTILE_RECORDS: usize = 4096;

fn passes(bytes: &[u8], limits: &Sf2Limits) -> BankShape {
    match Sf2Bank::check(bytes, limits) {
        Ok(shape) => shape,
        Err(error) => panic!("{error:?}"),
    }
}

fn refused(bytes: &[u8], limits: &Sf2Limits) -> UnsafeBank {
    match Sf2Bank::check(bytes, limits) {
        Err(BankError::Unsafe { reason }) => reason,
        other => panic!("{other:?}"),
    }
}

fn refused_by_default(bytes: &[u8]) -> UnsafeBank {
    refused(bytes, &Sf2Limits::DEFAULT)
}

fn set_word(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn set_size(bytes: &mut [u8], header: usize, size: u32) {
    bytes[header + 4..header + 8].copy_from_slice(&size.to_le_bytes());
}

fn size_of(bytes: &[u8], header: usize) -> u32 {
    u32::from_le_bytes(bytes[header + 4..header + 8].try_into().expect("size"))
}

fn record_field(bytes: &[u8], id: &[u8; 4], width: usize, record: usize, offset: usize) -> usize {
    pdta_chunk(bytes, id).start + record * width + offset
}

fn records(bytes: &[u8], id: &[u8; 4], width: usize) -> usize {
    pdta_chunk(bytes, id).len() / width
}

#[test]
fn the_generated_banks_pass_every_guard_check() {
    let tiny = passes(tiny_bytes(), &Sf2Limits::DEFAULT);
    assert_eq!(tiny.layer_bound, 3);
    assert_eq!(tiny.region_pairs, 12);
    assert!(tiny.loader_bytes > 0);
    passes(&padded_sf2(10_000), &Sf2Limits::DEFAULT);
    let layered = passes(&layered_sf2(8, 8), &Sf2Limits::DEFAULT);
    assert_eq!(layered.layer_bound, 64);
}

#[test]
fn a_file_above_the_size_limit_is_refused() {
    let length = tiny_bytes().len() as u64;
    let tight = Sf2Limits {
        file_bytes: length - 1,
        ..Sf2Limits::DEFAULT
    };
    assert_eq!(
        refused(tiny_bytes(), &tight),
        UnsafeBank::FileTooLarge { limit_mib: 0 }
    );
    passes(
        tiny_bytes(),
        &Sf2Limits {
            file_bytes: length,
            ..Sf2Limits::DEFAULT
        },
    );
}

#[test]
fn a_chunk_larger_than_its_section_is_refused() {
    for id in [b"INAM", b"smpl", b"pgen"] {
        let mut bytes = tiny_bytes().to_vec();
        let header = chunk_header(&bytes, id);
        set_size(&mut bytes, header, 0xFFFF_FFF0);
        assert_eq!(refused_by_default(&bytes), UnsafeBank::ChunkOverflow);
    }
    let cut = &tiny_bytes()[..tiny_bytes().len() - 1];
    assert_eq!(refused_by_default(cut), UnsafeBank::ChunkOverflow);
}

#[test]
fn a_list_without_a_type_is_refused() {
    let mut bytes = tiny_bytes().to_vec();
    let info = chunk_header(&bytes, b"INFO") - 8;
    set_size(&mut bytes, info, 2);
    assert_eq!(refused_by_default(&bytes), UnsafeBank::ListType);
}

#[test]
fn a_version_field_with_the_wrong_size_is_refused() {
    let mut bytes = tiny_bytes().to_vec();
    let ifil = chunk_header(&bytes, b"ifil");
    set_size(&mut bytes, ifil, 2);
    assert_eq!(refused_by_default(&bytes), UnsafeBank::VersionSize);
}

#[test]
fn sample_data_with_an_odd_or_tiny_size_is_refused() {
    let mut odd = tiny_bytes().to_vec();
    let smpl = chunk_header(&odd, b"smpl");
    let size = size_of(&odd, smpl);
    set_size(&mut odd, smpl, size - 1);
    assert_eq!(refused_by_default(&odd), UnsafeBank::OddSamples);
    let mut tiny = tiny_bytes().to_vec();
    set_size(&mut tiny, smpl, 2);
    assert_eq!(refused_by_default(&tiny), UnsafeBank::OddSamples);
}

#[test]
fn zone_numbers_that_run_backwards_or_past_the_end_are_refused() {
    let tiny = tiny_bytes();
    let mut backwards = tiny.to_vec();
    let at = record_field(&backwards, b"phdr", PRESET_RECORD, 1, PRESET_BAG);
    set_word(&mut backwards, at, 9);
    assert_eq!(refused_by_default(&backwards), UnsafeBank::ZoneOrder);
    let mut past = tiny.to_vec();
    let terminal = records(&past, b"phdr", PRESET_RECORD) - 1;
    let bags = records(&past, b"pbag", BAG_RECORD) as u16;
    let at = record_field(&past, b"phdr", PRESET_RECORD, terminal, PRESET_BAG);
    set_word(&mut past, at, bags);
    assert_eq!(refused_by_default(&past), UnsafeBank::ZoneOrder);
    let mut instrument = tiny.to_vec();
    let terminal = records(&instrument, b"inst", INSTRUMENT_RECORD) - 1;
    let bags = records(&instrument, b"ibag", BAG_RECORD) as u16;
    let at = record_field(
        &instrument,
        b"inst",
        INSTRUMENT_RECORD,
        terminal,
        INSTRUMENT_BAG,
    );
    set_word(&mut instrument, at, bags);
    assert_eq!(refused_by_default(&instrument), UnsafeBank::ZoneOrder);
}

#[test]
fn generator_numbers_that_run_backwards_or_past_the_end_are_refused() {
    assert_eq!(
        refused_by_default(&zone_bomb(SMALL_BOMB)),
        UnsafeBank::GeneratorOrder
    );
    let mut past = tiny_bytes().to_vec();
    let terminal = records(&past, b"ibag", BAG_RECORD) - 1;
    let generators = records(&past, b"igen", BAG_RECORD) as u16;
    let at = record_field(&past, b"ibag", BAG_RECORD, terminal, 0);
    set_word(&mut past, at, generators);
    assert_eq!(refused_by_default(&past), UnsafeBank::GeneratorOrder);
}

#[test]
fn modulator_numbers_that_run_backwards_or_past_the_end_are_refused() {
    let mut backwards = tiny_bytes().to_vec();
    let at = record_field(&backwards, b"pbag", BAG_RECORD, 0, BAG_MODULATOR);
    set_word(&mut backwards, at, 1);
    assert_eq!(refused_by_default(&backwards), UnsafeBank::ModulatorOrder);
    let mut past = tiny_bytes().to_vec();
    let terminal = records(&past, b"ibag", BAG_RECORD) - 1;
    let at = record_field(&past, b"ibag", BAG_RECORD, terminal, BAG_MODULATOR);
    set_word(&mut past, at, 5);
    assert_eq!(refused_by_default(&past), UnsafeBank::ModulatorOrder);
}

#[test]
fn a_bank_whose_loader_would_exceed_the_memory_budget_is_refused() {
    let needed = passes(tiny_bytes(), &Sf2Limits::DEFAULT).loader_bytes;
    let budget = |slack: u64| Sf2Limits {
        loader_file_factor: 0,
        loader_slack_bytes: slack,
        ..Sf2Limits::DEFAULT
    };
    assert_eq!(
        refused(tiny_bytes(), &budget(needed - 1)),
        UnsafeBank::LoaderBudget
    );
    passes(tiny_bytes(), &budget(needed));
    let file = |slack: u64| Sf2Limits {
        loader_file_factor: 2,
        loader_slack_bytes: slack,
        ..Sf2Limits::DEFAULT
    };
    for route in [BypassRoute::HugePresetBags, BypassRoute::HugeSampleHeaders] {
        let bytes = bypass_bank(route, HOSTILE_RECORDS);
        assert_eq!(
            refused(&bytes, &file(0)),
            UnsafeBank::LoaderBudget,
            "{route:?}"
        );
        passes(&bytes, &Sf2Limits::DEFAULT);
    }
    let overflowing = Sf2Limits {
        loader_file_factor: u64::MAX,
        ..Sf2Limits::DEFAULT
    };
    assert_eq!(
        refused(tiny_bytes(), &overflowing),
        UnsafeBank::LoaderBudget
    );
}

#[test]
fn the_loader_estimate_counts_the_sample_data_and_every_header() {
    let padded = padded_sf2(100_000);
    let shape = passes(&padded, &Sf2Limits::DEFAULT);
    assert!(
        shape.loader_bytes >= 200_000 + 256 * 1024,
        "{}",
        shape.loader_bytes
    );
    let small = passes(
        &bypass_bank(BypassRoute::HugeSampleHeaders, 1),
        &Sf2Limits::DEFAULT,
    );
    let large = passes(
        &bypass_bank(BypassRoute::HugeSampleHeaders, HOSTILE_RECORDS),
        &Sf2Limits::DEFAULT,
    );
    assert!(large.loader_bytes - small.loader_bytes >= 46 * (HOSTILE_RECORDS as u64 - 1));
}

#[test]
fn a_missing_table_or_sample_chunk_is_refused() {
    assert_eq!(
        refused_by_default(&bypass_bank(BypassRoute::MissingInstrumentGenerators, 4)),
        UnsafeBank::MissingChunk
    );
    let mut no_samples = tiny_bytes().to_vec();
    let smpl = chunk_header(&no_samples, b"smpl");
    no_samples[smpl..smpl + 4].copy_from_slice(b"sm24");
    assert_eq!(refused_by_default(&no_samples), UnsafeBank::MissingChunk);
    let mut swapped = tiny_bytes().to_vec();
    let info = chunk_header(&swapped, b"INFO");
    swapped[info..info + 4].copy_from_slice(b"sdta");
    assert_eq!(refused_by_default(&swapped), UnsafeBank::MissingChunk);
}

#[test]
fn a_duplicated_table_is_refused() {
    assert_eq!(
        refused_by_default(&bypass_bank(BypassRoute::DuplicateInstrumentBags, 4)),
        UnsafeBank::DuplicateChunk
    );
}

#[test]
fn an_unknown_section_is_refused() {
    for (old, new) in [(b"ISFT", b"IXXX"), (b"pmod", b"pmox")] {
        let mut bytes = tiny_bytes().to_vec();
        let at = chunk_header(&bytes, old);
        bytes[at..at + 4].copy_from_slice(new);
        assert_eq!(refused_by_default(&bytes), UnsafeBank::UnknownChunk);
    }
}

#[test]
fn a_table_with_a_broken_size_is_refused() {
    let mut definition = tiny_definition(None);
    definition.instruments.clear();
    assert_eq!(
        refused_by_default(&write_sf2(&definition)),
        UnsafeBank::MalformedTable
    );
}

#[test]
fn too_many_heavily_layered_presets_are_refused() {
    let strict = Sf2Limits {
        voices_per_note: 1,
        grid_presets: 0,
        ..Sf2Limits::DEFAULT
    };
    assert_eq!(
        refused(&layered_sf2(2, 1), &strict),
        UnsafeBank::LayeredPresets
    );
    passes(
        &layered_sf2(2, 1),
        &Sf2Limits {
            grid_presets: 1,
            ..strict
        },
    );
}

#[test]
fn a_preset_that_layers_too_many_regions_is_refused() {
    assert_eq!(
        refused_by_default(&layered_sf2(300, 300)),
        UnsafeBank::RegionPairs
    );
    let tight = Sf2Limits {
        region_pairs: 11,
        ..Sf2Limits::DEFAULT
    };
    assert_eq!(refused(tiny_bytes(), &tight), UnsafeBank::RegionPairs);
}

#[test]
fn a_key_that_would_start_too_many_voices_is_refused() {
    assert_eq!(refused_by_default(&layered_sf2(10, 10)), UnsafeBank::Layers);
    assert_eq!(refused_by_default(&layered_sf2(65, 1)), UnsafeBank::Layers);
    assert_eq!(
        passes(&layered_sf2(64, 1), &Sf2Limits::DEFAULT).layer_bound,
        64
    );
}

#[test]
fn every_refusal_reads_as_a_sentence_for_the_user() {
    let error = BankError::Unsafe {
        reason: UnsafeBank::Layers,
    };
    assert_eq!(
        error.to_string(),
        "this sound bank was not loaded because one key would start too many voices at once. The file may be damaged or built to crash programs."
    );
}

#[test]
fn the_size_refusal_names_the_limit() {
    let error = BankError::Unsafe {
        reason: UnsafeBank::FileTooLarge {
            limit_mib: Sf2Limits::DEFAULT.file_bytes >> 20,
        },
    };
    assert!(
        error.to_string().contains("larger than 1024 MiB"),
        "{error}"
    );
}
