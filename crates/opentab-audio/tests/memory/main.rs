mod sfz;

use std::alloc::System;

use opentab_audio::{Bank, BankError, SampleRate, Sf2Bank, Sf2Limits, Synth, UnsafeBank};
use opentab_playback::{Channel, MidiMessage};
use opentab_testkit::banks::{
    BypassRoute, PAD_PROGRAM, apply_structural, arb_structural_mutation, bypass_bank, padded_sf2,
    tiny_sf2, zone_bomb,
};
use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, Stats, StatsAlloc};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

const TRACKS: usize = 20;
const PAD_FRAMES: u32 = 4 * 1024 * 1024;
const MIN_SAMPLE_BYTES: usize = 8 * 1024 * 1024;
const PER_TRACK_BUDGET: usize = 1024 * 1024;
const REFERENCES_PER_TRACK: usize = 2;
const COPY_FRACTION: usize = 4;
const RENDER_FRAMES: usize = 512;
const BOMB_BAGS: u16 = 512;
const INPUT_FACTOR: usize = 4;
const SLACK_BYTES: usize = 1024 * 1024;
const MAX_MUTATIONS: usize = 5;
const HOSTILE_RECORDS: usize = 262_144;
const REFUSAL_BYTES: usize = 64 * 1024;

fn retained(change: Stats) -> usize {
    let grown = change.bytes_allocated as isize + change.bytes_reallocated;
    usize::try_from(grown - change.bytes_deallocated as isize).unwrap_or(0)
}

fn bounded(input: usize) -> usize {
    INPUT_FACTOR * input + SLACK_BYTES
}

fn twenty_tracks_of_one_multi_megabyte_sf2_add_no_copy_of_its_sample_data() {
    let bytes = padded_sf2(PAD_FRAMES);
    let loading = Region::new(GLOBAL);
    let bank = Sf2Bank::load(&bytes).expect("the padded bank loads");
    let loaded = retained(loading.change());
    drop(bytes);
    let samples = bank.sample_bytes();
    assert!(samples >= MIN_SAMPLE_BYTES, "{samples} sample bytes");
    assert!(
        loaded >= samples,
        "loading kept {loaded} bytes for {samples} sample bytes"
    );
    assert_eq!(bank.shared_count(), 1);

    let instantiating = Region::new(GLOBAL);
    let mut voices: Vec<Box<dyn Synth>> = (0..TRACKS)
        .map(|_| bank.instantiate(SampleRate::STUDIO))
        .collect();
    let added = retained(instantiating.change());
    assert_eq!(bank.shared_count(), 1 + REFERENCES_PER_TRACK * TRACKS);
    assert!(
        added <= PER_TRACK_BUDGET * TRACKS,
        "{TRACKS} tracks added {added} bytes"
    );
    let per_track = added / TRACKS;
    assert!(
        per_track * COPY_FRACTION < samples,
        "each track added {per_track} bytes, one copy is {samples}"
    );

    let channel = Channel::new(0).expect("channel");
    let mut left = vec![0.0f32; RENDER_FRAMES];
    let mut right = vec![0.0f32; RENDER_FRAMES];
    let rendering = Region::new(GLOBAL);
    for voice in &mut voices {
        voice.handle(
            channel,
            &MidiMessage::Program {
                program: PAD_PROGRAM as u8,
            },
        );
        voice.handle(
            channel,
            &MidiMessage::NoteOn {
                key: 69,
                velocity: 100,
            },
        );
        voice.render(&mut left, &mut right);
    }
    let rendered = rendering.change();
    assert!(left.iter().any(|sample| *sample != 0.0));
    assert_eq!(rendered.bytes_allocated, 0, "{rendered:?}");

    drop(voices);
    assert_eq!(bank.shared_count(), 1);
    assert!(bank.faults().is_empty());
}

fn the_zone_bomb_is_refused_before_it_allocates() {
    let bomb = zone_bomb(BOMB_BAGS);
    let loading = Region::new(GLOBAL);
    let result = Sf2Bank::load(&bomb).map(|bank| bank.sample_bytes());
    let allocated = loading.change().bytes_allocated;
    assert_eq!(
        result,
        Err(BankError::Unsafe {
            reason: UnsafeBank::GeneratorOrder
        })
    );
    assert!(
        allocated <= bounded(bomb.len()),
        "{allocated} bytes for a {}-byte bomb",
        bomb.len()
    );
}

fn the_loader_estimate_covers_what_loading_allocates() {
    for bytes in [tiny_sf2(), padded_sf2(100_000)] {
        let loading = Region::new(GLOBAL);
        let bank = Sf2Bank::load(&bytes).expect("loads");
        let allocated = loading.change().bytes_allocated;
        assert!(
            allocated as u64 <= bank.shape().loader_bytes,
            "{allocated} bytes against an estimate of {}",
            bank.shape().loader_bytes
        );
    }
}

fn structural_mutations_allocate_in_proportion_to_the_input() {
    let tiny = tiny_sf2();
    let mut runner = TestRunner::new(Config::default());
    let strategy = prop::collection::vec(arb_structural_mutation(), 1..MAX_MUTATIONS);
    runner
        .run(&strategy, |mutations| {
            let bytes = apply_structural(&tiny, &mutations);
            let loading = Region::new(GLOBAL);
            let loaded = Sf2Bank::load(&bytes).map(|bank| bank.sample_bytes());
            let allocated = loading.change().bytes_allocated;
            prop_assert!(
                allocated <= bounded(bytes.len()),
                "{allocated} bytes for {} input bytes ({loaded:?})",
                bytes.len()
            );
            Ok(())
        })
        .expect("bounded allocation");
}

fn route_limits(route: BypassRoute) -> Sf2Limits {
    match route {
        BypassRoute::MissingInstrumentGenerators | BypassRoute::DuplicateInstrumentBags => {
            Sf2Limits::DEFAULT
        }
        BypassRoute::HugePresetBags | BypassRoute::HugeSampleHeaders => Sf2Limits {
            loader_slack_bytes: 0,
            ..Sf2Limits::DEFAULT
        },
    }
}

fn route_reason(route: BypassRoute) -> UnsafeBank {
    match route {
        BypassRoute::MissingInstrumentGenerators => UnsafeBank::MissingChunk,
        BypassRoute::DuplicateInstrumentBags => UnsafeBank::DuplicateChunk,
        BypassRoute::HugePresetBags | BypassRoute::HugeSampleHeaders => UnsafeBank::LoaderBudget,
    }
}

fn every_bypass_route_is_refused_before_it_allocates() {
    for route in BypassRoute::ALL {
        let bytes = bypass_bank(route, HOSTILE_RECORDS);
        let limits = route_limits(route);
        let loading = Region::new(GLOBAL);
        let result = Sf2Bank::load_with_limits(&bytes, &limits).map(|bank| bank.sample_bytes());
        let allocated = loading.change().bytes_allocated;
        assert_eq!(
            result,
            Err(BankError::Unsafe {
                reason: route_reason(route)
            }),
            "{route:?}"
        );
        assert!(
            allocated <= REFUSAL_BYTES,
            "{route:?} allocated {allocated} bytes for {} input bytes",
            bytes.len()
        );
    }
}

fn main() {
    twenty_tracks_of_one_multi_megabyte_sf2_add_no_copy_of_its_sample_data();
    the_zone_bomb_is_refused_before_it_allocates();
    every_bypass_route_is_refused_before_it_allocates();
    the_loader_estimate_covers_what_loading_allocates();
    structural_mutations_allocate_in_proportion_to_the_input();
    sfz::twenty_tracks_of_one_sfz_bank_decode_each_sample_once();
    sfz::hostile_samples_never_allocate_what_they_declare();
    sfz::text_bombs_stop_at_the_expansion_cap();
    sfz::a_huge_declared_file_is_refused_before_it_is_read();
    println!("memory: 9 checks passed");
}
