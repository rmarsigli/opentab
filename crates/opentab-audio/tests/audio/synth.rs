use opentab_audio::{
    Bank, BankFormat, BuiltInBank, BuiltInSynth, DRUM_DECAY_MS, MELODIC_RELEASE_MS, SampleRate,
    Synth,
};
use opentab_model::MidiBank;
use opentab_playback::{
    BEND_MAX, Channel, MidiMessage, PAN_CONTROLLER, PITCH_BEND_RANGE_RPN, VOLUME_CONTROLLER,
};
use proptest::prelude::*;
use proptest::test_runner::{FileFailurePersistence, RngSeed};

const STEEL: u8 = 25;
const PIANO: u8 = 0;
const A4: u8 = 69;
const A2: u8 = 45;
const KICK: u8 = 36;
const LOUD: u8 = 110;
const SETTLE: usize = 4_800;
const WINDOW: usize = 4_800;
const PROPERTY_SEED: u64 = 0x0074_3112;
const FULL_SCALE: f32 = 8_388_608.0;

fn channel(number: u8) -> Channel {
    Channel::new(number).expect("channel")
}

fn synth(rate: SampleRate) -> BuiltInSynth {
    BuiltInSynth::new(rate)
}

fn send(synth: &mut BuiltInSynth, number: u8, message: MidiMessage) {
    synth.handle(channel(number), &message);
}

fn program(synth: &mut BuiltInSynth, number: u8, program: u8) {
    send(synth, number, MidiMessage::Program { program });
}

fn note_on(synth: &mut BuiltInSynth, number: u8, key: u8, velocity: u8) {
    send(synth, number, MidiMessage::NoteOn { key, velocity });
}

fn note_off(synth: &mut BuiltInSynth, number: u8, key: u8) {
    send(synth, number, MidiMessage::NoteOff { key, velocity: 64 });
}

#[derive(Debug, Clone, PartialEq)]
struct Stereo {
    left: Vec<f32>,
    right: Vec<f32>,
}

fn render(synth: &mut BuiltInSynth, frames: usize) -> Stereo {
    let mut left = vec![0.0; frames];
    let mut right = vec![0.0; frames];
    synth.render(&mut left, &mut right);
    Stereo { left, right }
}

fn energy(samples: &[f32]) -> f64 {
    samples.iter().map(|sample| f64::from(sample.abs())).sum()
}

fn best_lag(samples: &[f32], low: usize, high: usize) -> usize {
    let window = &samples[..samples.len() - high];
    (low..=high)
        .max_by(|left, right| {
            let score = |lag: usize| -> f64 {
                window
                    .iter()
                    .zip(&samples[lag..])
                    .map(|(a, b)| f64::from(*a) * f64::from(*b))
                    .sum()
            };
            score(*left).total_cmp(&score(*right))
        })
        .expect("lag")
}

fn plucked(rate: SampleRate, key: u8) -> Vec<f32> {
    let mut synth = synth(rate);
    program(&mut synth, 0, STEEL);
    note_on(&mut synth, 0, key, LOUD);
    render(&mut synth, SETTLE);
    render(&mut synth, WINDOW).left
}

#[test]
fn a_plucked_string_sounds_at_its_key() {
    assert_eq!(best_lag(&plucked(SampleRate::STUDIO, A4), 60, 200), 109);
    assert_eq!(best_lag(&plucked(SampleRate::CD, A4), 60, 200), 100);
    assert_eq!(best_lag(&plucked(SampleRate::STUDIO, A2), 300, 600), 436);
}

#[test]
fn pitch_bend_moves_the_delay_line_read_by_the_rpn_range() {
    let mut synth = synth(SampleRate::STUDIO);
    program(&mut synth, 0, STEEL);
    send(
        &mut synth,
        0,
        MidiMessage::Rpn {
            parameter: PITCH_BEND_RANGE_RPN,
            value: 2,
        },
    );
    note_on(&mut synth, 0, A4, LOUD);
    render(&mut synth, SETTLE);
    send(&mut synth, 0, MidiMessage::PitchBend { value: BEND_MAX });
    render(&mut synth, SETTLE);
    assert_eq!(best_lag(&render(&mut synth, WINDOW).left, 60, 200), 97);
    send(
        &mut synth,
        0,
        MidiMessage::Rpn {
            parameter: PITCH_BEND_RANGE_RPN,
            value: 12,
        },
    );
    send(&mut synth, 0, MidiMessage::PitchBend { value: 0 });
    render(&mut synth, SETTLE);
    assert_eq!(best_lag(&render(&mut synth, WINDOW).left, 150, 300), 218);
}

#[test]
fn a_released_note_falls_to_exact_silence_within_the_release_time() {
    for program_number in [STEEL, PIANO, 48, 88, 33] {
        let rate = SampleRate::STUDIO;
        let mut synth = synth(rate);
        program(&mut synth, 0, program_number);
        note_on(&mut synth, 0, A4, LOUD);
        let held = render(&mut synth, SETTLE);
        assert!(energy(&held.left) > 0.0, "{program_number}");
        note_off(&mut synth, 0, A4);
        let release = usize::try_from(rate.frames_of_millis(MELODIC_RELEASE_MS)).expect("frames");
        render(&mut synth, release);
        let after = render(&mut synth, WINDOW);
        assert!(
            after
                .left
                .iter()
                .chain(&after.right)
                .all(|sample| *sample == 0.0),
            "{program_number}"
        );
        assert_eq!(synth.sounding(), 0);
    }
}

#[test]
fn drums_ring_out_on_their_own_within_the_drum_decay() {
    let rate = SampleRate::STUDIO;
    for key in [KICK, 38, 42, 46, 49, 51, 62, 81, 20, 100] {
        let mut synth = synth(rate);
        note_on(&mut synth, 9, key, LOUD);
        note_off(&mut synth, 9, key);
        let hit = render(&mut synth, SETTLE / 4);
        assert!(energy(&hit.left) > 0.0, "{key}");
        let decay = usize::try_from(rate.frames_of_millis(DRUM_DECAY_MS)).expect("frames");
        render(&mut synth, decay);
        assert_eq!(synth.sounding(), 0, "{key}");
        let after = render(&mut synth, WINDOW);
        assert!(after.left.iter().all(|sample| *sample == 0.0), "{key}");
    }
}

#[test]
fn bank_120_plays_the_drum_kit_on_any_channel() {
    let rate = SampleRate::STUDIO;
    let mut drums = synth(rate);
    note_on(&mut drums, 9, KICK, LOUD);
    let mut banked = synth(rate);
    send(
        &mut banked,
        2,
        MidiMessage::Bank {
            bank: MidiBank::new(120, 0).expect("bank"),
        },
    );
    note_on(&mut banked, 2, KICK, LOUD);
    let mut melodic = synth(rate);
    note_on(&mut melodic, 2, KICK, LOUD);
    let expected = render(&mut drums, SETTLE);
    assert_eq!(render(&mut banked, SETTLE), expected);
    assert_ne!(render(&mut melodic, SETTLE), expected);
}

#[test]
fn pan_volume_and_velocity_shape_the_level() {
    let rate = SampleRate::STUDIO;
    let level = |pan: u8, volume: u8, velocity: u8| {
        let mut synth = synth(rate);
        program(&mut synth, 3, STEEL);
        send(
            &mut synth,
            3,
            MidiMessage::Control {
                controller: PAN_CONTROLLER,
                value: pan,
            },
        );
        send(
            &mut synth,
            3,
            MidiMessage::Control {
                controller: VOLUME_CONTROLLER,
                value: volume,
            },
        );
        note_on(&mut synth, 3, A4, velocity);
        let out = render(&mut synth, SETTLE);
        (energy(&out.left), energy(&out.right))
    };
    let (left, right) = level(0, 100, LOUD);
    assert!(left > 0.0);
    assert_eq!(right, 0.0);
    let (left, right) = level(127, 100, LOUD);
    assert_eq!(left, 0.0);
    assert!(right > 0.0);
    let (left, right) = level(64, 100, LOUD);
    assert_eq!(left, right);
    assert_eq!(level(64, 0, LOUD), (0.0, 0.0));
    assert!(level(64, 127, LOUD).0 > level(64, 64, LOUD).0);
    assert!(level(64, 100, 127).0 > level(64, 100, 40).0);
}

#[test]
fn programs_choose_distinct_timbres() {
    let rate = SampleRate::STUDIO;
    let sound = |program_number: u8| {
        let mut synth = synth(rate);
        program(&mut synth, 0, program_number);
        note_on(&mut synth, 0, A4, LOUD);
        render(&mut synth, SETTLE)
    };
    let all: Vec<Stereo> = [24, 25, 29, 30, 33, 105, 0, 40, 56, 80].map(sound).into();
    for (index, first) in all.iter().enumerate() {
        for second in &all[index + 1..] {
            assert_ne!(first, second);
        }
    }
}

#[test]
fn reset_silences_every_voice_and_restores_the_channels() {
    let rate = SampleRate::STUDIO;
    let mut fresh = synth(rate);
    note_on(&mut fresh, 0, A4, LOUD);
    let expected = render(&mut fresh, SETTLE);
    let mut used = synth(rate);
    program(&mut used, 0, 30);
    send(
        &mut used,
        0,
        MidiMessage::Control {
            controller: PAN_CONTROLLER,
            value: 0,
        },
    );
    note_on(&mut used, 0, 40, LOUD);
    note_on(&mut used, 9, KICK, LOUD);
    render(&mut used, SETTLE);
    used.reset();
    assert_eq!(used.sounding(), 0);
    assert!(
        render(&mut used, SETTLE)
            .left
            .iter()
            .all(|sample| *sample == 0.0)
    );
    note_on(&mut used, 0, A4, LOUD);
    assert_eq!(render(&mut used, SETTLE), expected);
}

#[test]
fn the_float_output_is_the_fixed_point_output_scaled_exactly() {
    let rate = SampleRate::STUDIO;
    let mut floating = synth(rate);
    let mut fixed = synth(rate);
    for target in [&mut floating, &mut fixed] {
        program(target, 0, 30);
        note_on(target, 0, 40, 127);
        note_on(target, 0, 47, 127);
        note_on(target, 9, KICK, 127);
    }
    let out = render(&mut floating, SETTLE);
    let mut left = vec![0; SETTLE];
    let mut right = vec![0; SETTLE];
    fixed.render_fixed(&mut left, &mut right);
    for (sample, integer) in out
        .left
        .iter()
        .chain(&out.right)
        .zip(left.iter().chain(&right))
    {
        assert_eq!(*sample * FULL_SCALE, *integer as f32);
        assert!(integer.abs() <= 1 << 23);
    }
}

#[test]
fn the_built_in_bank_lists_general_midi_and_a_kit() {
    let bank = BuiltInBank;
    assert_eq!(bank.format(), BankFormat::BuiltIn);
    let instruments = bank.instruments();
    assert_eq!(instruments.len(), 129);
    assert_eq!(instruments[25].program, 25);
    assert_eq!(instruments[25].name, "Acoustic Guitar (steel)");
    assert_eq!(instruments[128].bank, MidiBank::new(120, 0).expect("bank"));
    let mut synth = bank.instantiate(SampleRate::CD);
    synth.handle(
        channel(0),
        &MidiMessage::NoteOn {
            key: A4,
            velocity: LOUD,
        },
    );
    let mut left = vec![0.0; 64];
    let mut right = vec![0.0; 64];
    synth.render(&mut left, &mut right);
    assert!(energy(&left) > 0.0);
}

#[derive(Debug, Clone, Copy)]
struct Step {
    kind: u8,
    channel: u8,
    key: u8,
    value: u8,
    gap: usize,
    split: usize,
}

fn arb_steps() -> impl Strategy<Value = Vec<Step>> {
    prop::collection::vec(
        (
            0u8..8,
            0u8..16,
            0u8..128,
            0u8..128,
            0usize..3_000,
            1usize..700,
        )
            .prop_map(|(kind, channel, key, value, gap, split)| Step {
                kind,
                channel,
                key,
                value,
                gap,
                split,
            }),
        1..40,
    )
}

fn message(step: Step) -> MidiMessage {
    match step.kind {
        0 | 1 => MidiMessage::NoteOn {
            key: step.key,
            velocity: step.value,
        },
        2 => MidiMessage::NoteOff {
            key: step.key,
            velocity: 0,
        },
        3 => MidiMessage::Program {
            program: step.value,
        },
        4 => MidiMessage::PitchBend {
            value: u16::from(step.value) << 7,
        },
        5 => MidiMessage::Rpn {
            parameter: PITCH_BEND_RANGE_RPN,
            value: step.value % 25,
        },
        6 => MidiMessage::Control {
            controller: PAN_CONTROLLER,
            value: step.value,
        },
        _ => MidiMessage::Control {
            controller: VOLUME_CONTROLLER,
            value: step.value,
        },
    }
}

fn config() -> ProptestConfig {
    ProptestConfig {
        failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
        rng_seed: RngSeed::Fixed(PROPERTY_SEED),
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn output_does_not_depend_on_how_the_render_is_split(steps in arb_steps()) {
        let rate = SampleRate::STUDIO;
        let mut whole = synth(rate);
        let mut pieces = synth(rate);
        for step in steps {
            send(&mut whole, step.channel, message(step));
            send(&mut pieces, step.channel, message(step));
            let expected = render(&mut whole, step.gap);
            let mut left = Vec::new();
            let mut right = Vec::new();
            let mut done = 0;
            while done < step.gap {
                let count = step.split.min(step.gap - done);
                let piece = render(&mut pieces, count);
                left.extend(piece.left);
                right.extend(piece.right);
                done += count;
            }
            prop_assert!(expected.left.iter().all(|sample| sample.is_finite() && sample.abs() <= 1.0));
            prop_assert_eq!(&expected.left, &left);
            prop_assert_eq!(&expected.right, &right);
        }
    }
}
