use std::fs;
use std::path::Path;

use opentab_audio::{Bank, BankError, BankInstrument, Sf2Bank, Sf2Limits, UnsafeBank};
use opentab_testkit::banks::{
    BypassRoute, apply_structural, arb_structural_mutation, bypass_bank, padded_sf2, zone_bomb,
};
use proptest::prelude::*;

use crate::sf2::{engine_panic_bank, engine_render_panic_bank};
use crate::sf2_fuzz::{apply, mutation};
use crate::support::{scratch_dir, tiny_bytes};

fn listing(result: Result<Sf2Bank, BankError>) -> Result<Vec<BankInstrument>, BankError> {
    result.map(|bank| bank.instruments())
}

fn same_from_path(dir: &Path, bytes: &[u8], limits: &Sf2Limits) {
    let path = dir.join("bank.sf2");
    fs::write(&path, bytes).expect("written");
    let from_path = listing(Sf2Bank::open_with_limits(&path, limits));
    let from_bytes = listing(Sf2Bank::load_with_limits(bytes, limits));
    assert_eq!(from_path, from_bytes);
}

#[test]
fn opening_from_a_path_matches_loading_the_bytes() {
    let dir = scratch_dir();
    let limits = Sf2Limits::DEFAULT;
    for bytes in [
        tiny_bytes().to_vec(),
        padded_sf2(4096),
        engine_panic_bank(),
        engine_render_panic_bank(),
        zone_bomb(64),
        Vec::new(),
        b"RIFF".to_vec(),
        tiny_bytes()[..tiny_bytes().len() / 2].to_vec(),
    ] {
        same_from_path(dir.path(), &bytes, &limits);
    }
    for route in BypassRoute::ALL {
        same_from_path(dir.path(), &bypass_bank(route, 1024), &limits);
    }
}

#[test]
fn opening_a_missing_or_oversized_file_refuses_without_reading_it() {
    let dir = scratch_dir();
    assert!(matches!(
        Sf2Bank::open(&dir.path().join("absent.sf2")),
        Err(BankError::Invalid { .. })
    ));
    let path = dir.path().join("big.sf2");
    fs::write(&path, tiny_bytes()).expect("written");
    let limits = Sf2Limits {
        file_bytes: 1 << 16,
        ..Sf2Limits::DEFAULT
    };
    assert_eq!(
        Sf2Bank::open_with_limits(&path, &limits).map(|_| ()),
        Err(BankError::Unsafe {
            reason: UnsafeBank::FileTooLarge { limit_mib: 0 }
        })
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn a_mutated_bank_opens_from_a_path_exactly_as_from_bytes(
        mutations in prop::collection::vec(mutation(), 1..6),
    ) {
        let dir = scratch_dir();
        same_from_path(dir.path(), &apply(&mutations), &Sf2Limits::DEFAULT);
    }

    #[test]
    fn a_structurally_mutated_bank_opens_from_a_path_exactly_as_from_bytes(
        mutations in prop::collection::vec(arb_structural_mutation(), 1..6),
    ) {
        let dir = scratch_dir();
        same_from_path(dir.path(), &apply_structural(tiny_bytes(), &mutations), &Sf2Limits::DEFAULT);
    }
}
