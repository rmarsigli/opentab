use opentab_audio::{Bank, SampleRate, Sf2Bank, Synth, TRACK_HEADROOM};
use opentab_model::MidiBank;
use opentab_playback::{Channel, MidiMessage, PITCH_BEND_RANGE_RPN};
use proptest::prelude::*;

use opentab_testkit::banks::{apply_structural, arb_structural_mutation};

use crate::sf2::{engine_panic_bank, engine_render_panic_bank};
use crate::support::tiny_bytes;

const LONG_CASES: u32 = 20_000;
const CHUNK_FRAMES: usize = 256;
const CHUNKS_PER_STEP: usize = 2;
const CHANNELS: [u8; 3] = [0, 3, 9];
const PROGRAMS: [u8; 4] = [0, 24, 33, 89];
const KEYS: [u8; 6] = [0, 36, 42, 57, 69, 127];
const BANKS: [u8; 3] = [0, 8, 120];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mutation {
    Byte { at: usize, value: u8 },
    Word { at: usize, value: u32 },
    Truncate { at: usize },
}

fn pdta_start() -> usize {
    tiny_bytes()
        .windows(4)
        .position(|window| window == b"pdta")
        .expect("pdta")
}

fn position() -> impl Strategy<Value = usize> {
    let length = tiny_bytes().len();
    prop_oneof![3 => pdta_start()..length, 1 => 0..length]
}

pub(crate) fn mutation() -> impl Strategy<Value = Mutation> {
    prop_oneof![
        4 => (position(), any::<u8>()).prop_map(|(at, value)| Mutation::Byte { at, value }),
        3 => (position(), prop_oneof![Just(0u32), Just(u32::MAX), Just(0x7FFF), any::<u32>()])
            .prop_map(|(at, value)| Mutation::Word { at, value }),
        1 => position().prop_map(|at| Mutation::Truncate { at }),
    ]
}

pub(crate) fn apply(mutations: &[Mutation]) -> Vec<u8> {
    let mut bytes = tiny_bytes().to_vec();
    for mutation in mutations {
        match *mutation {
            Mutation::Byte { at, value } => {
                if let Some(byte) = bytes.get_mut(at) {
                    *byte = value;
                }
            }
            Mutation::Word { at, value } => {
                for (offset, byte) in value.to_le_bytes().into_iter().enumerate() {
                    if let Some(target) = bytes.get_mut(at + offset) {
                        *target = byte;
                    }
                }
            }
            Mutation::Truncate { at } => bytes.truncate(at),
        }
    }
    bytes
}

fn send(synth: &mut dyn Synth, channel: u8, message: MidiMessage) {
    synth.handle(Channel::new(channel).expect("channel"), &message);
}

fn render_checked(synth: &mut dyn Synth) {
    let mut left = vec![0.0f32; CHUNK_FRAMES];
    let mut right = vec![0.0f32; CHUNK_FRAMES];
    for _ in 0..CHUNKS_PER_STEP {
        synth.render(&mut left, &mut right);
        for sample in left.iter().chain(&right) {
            assert!(
                sample.is_finite() && sample.abs() <= TRACK_HEADROOM,
                "{sample}"
            );
        }
    }
}

fn exercise(bytes: &[u8]) {
    let bank = match Sf2Bank::load(bytes) {
        Ok(bank) => bank,
        Err(error) => {
            assert!(!error.to_string().is_empty());
            return;
        }
    };
    let listed = bank.instruments();
    assert!(listed.iter().all(|instrument| instrument.program < 128));
    let mut synth = bank.synth(SampleRate::STUDIO);
    synth.reset();
    for channel in CHANNELS {
        for msb in BANKS {
            let bank_message = MidiMessage::Bank {
                bank: MidiBank::new(msb, 0).expect("bank"),
            };
            send(&mut synth, channel, bank_message);
            for program in PROGRAMS {
                send(&mut synth, channel, MidiMessage::Program { program });
                for key in KEYS {
                    send(
                        &mut synth,
                        channel,
                        MidiMessage::NoteOn { key, velocity: 127 },
                    );
                }
                render_checked(&mut synth);
                send(
                    &mut synth,
                    channel,
                    MidiMessage::Rpn {
                        parameter: PITCH_BEND_RANGE_RPN,
                        value: 24,
                    },
                );
                send(&mut synth, channel, MidiMessage::PitchBend { value: 0 });
                render_checked(&mut synth);
                for key in KEYS {
                    send(
                        &mut synth,
                        channel,
                        MidiMessage::NoteOff { key, velocity: 64 },
                    );
                }
                render_checked(&mut synth);
            }
        }
    }
    if synth.is_silenced() {
        assert!(synth.fault().is_some());
        assert!(!bank.faults().is_empty());
    }
}

#[test]
fn fuzz_regression_cases_end_in_a_typed_error_or_a_reported_silence() {
    exercise(&engine_panic_bank());
    exercise(&engine_render_panic_bank());
    exercise(tiny_bytes());
}

proptest! {
    #[test]
    fn a_mutated_tiny_bank_never_panics_and_never_emits_nan(
        mutations in prop::collection::vec(mutation(), 1..6),
    ) {
        exercise(&apply(&mutations));
    }
}

proptest! {
    #[test]
    fn a_structurally_mutated_tiny_bank_never_panics_and_never_emits_nan(
        mutations in prop::collection::vec(arb_structural_mutation(), 1..6),
    ) {
        exercise(&apply_structural(tiny_bytes(), &mutations));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(LONG_CASES))]
    #[test]
    #[ignore = "long fuzz run, about seven minutes"]
    fn a_mutated_tiny_bank_never_panics_over_twenty_thousand_cases(
        mutations in prop::collection::vec(mutation(), 1..6),
    ) {
        exercise(&apply(&mutations));
    }
}
