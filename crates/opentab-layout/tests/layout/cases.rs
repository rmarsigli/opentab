use proptest::test_runner::{Config, RngSeed};

pub const REFERENCE_CASES: u32 = 256;
pub const REDUCED_RUN_SEED: u64 = 0x006f_7065_6e74_6162;

pub fn config() -> Config {
    let base = Config::default();
    let rng_seed = match base.rng_seed {
        RngSeed::Random if base.cases < REFERENCE_CASES => RngSeed::Fixed(REDUCED_RUN_SEED),
        seed => seed,
    };
    Config { rng_seed, ..base }
}

pub fn cases() -> u32 {
    config().cases
}

pub fn scaled(minimum: usize) -> usize {
    scaled_for(minimum, cases())
}

pub fn scaled_for(minimum: usize, cases: u32) -> usize {
    let cases = usize::try_from(cases).unwrap_or(usize::MAX);
    let reference = usize::try_from(REFERENCE_CASES).unwrap_or(usize::MAX);
    minimum.saturating_mul(cases) / reference
}

#[test]
fn reduced_runs_use_a_fixed_seed_and_full_runs_keep_the_configured_one() {
    let config = config();
    if config.cases < REFERENCE_CASES {
        assert!(matches!(config.rng_seed, RngSeed::Fixed(_)));
    } else {
        assert_eq!(config.rng_seed, Config::default().rng_seed);
    }
}

#[test]
fn tally_minimums_keep_their_values_at_256_cases_and_scale_with_the_case_count() {
    assert_eq!(scaled_for(3_000, 256), 3_000);
    assert_eq!(scaled_for(200, 256), 200);
    assert_eq!(scaled_for(3_000, 16), 187);
    assert_eq!(scaled_for(200, 16), 12);
    assert_eq!(scaled_for(10, 16), 0);
    assert_eq!(scaled_for(100, 512), 200);
    assert_eq!(scaled_for(usize::MAX, 512), usize::MAX / 256);
}
