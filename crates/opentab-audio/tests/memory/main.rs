use std::alloc::System;

use opentab_audio::{Bank, SampleRate, Sf2Bank, Synth};
use opentab_playback::{Channel, MidiMessage};
use opentab_testkit::banks::{PAD_PROGRAM, padded_sf2};
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

fn retained(change: Stats) -> usize {
    let grown = change.bytes_allocated as isize + change.bytes_reallocated;
    usize::try_from(grown - change.bytes_deallocated as isize).unwrap_or(0)
}

#[test]
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
