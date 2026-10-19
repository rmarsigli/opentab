use std::ops::Range;

use opentab_audio::{
    Bank, BankError, BankFormat, BankInstrument, FAULT_LOG_LIMIT, SF3_UNSUPPORTED, SampleRate,
    Sf2Bank, Sf2Limits, Synth, SynthStage, UnsafeBank,
};
use opentab_model::MidiBank;
use opentab_playback::{Channel, MidiMessage, PITCH_BEND_RANGE_RPN};
use opentab_testkit::banks::{
    LAYERED_PROGRAM, SINGLE_PROGRAM, generator, layered_sf2, tiny_definition, tiny_sf2, write_sf2,
};

use crate::support::{tiny_bank, tiny_bytes};

const RATE: SampleRate = SampleRate::STUDIO;
const MAX_TINY_BYTES: usize = 300_000;
const REFERENCES_PER_TRACK: usize = 2;
const SAMPLE_RATE_FIELD: usize = 36;
const SAMPLE_START_FIELD: usize = 20;
const START_OFFSET_GENERATOR: u16 = 0;
const OVERFLOWING_START: i32 = i32::MAX - 16;
const PDTA_CHUNKS: [&[u8; 4]; 9] = [
    b"phdr", b"pbag", b"pmod", b"pgen", b"inst", b"ibag", b"imod", b"igen", b"shdr",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cue {
    channel: u8,
    message: MidiMessage,
}

#[derive(Debug, Clone, PartialEq)]
struct Stereo {
    left: Vec<f32>,
    right: Vec<f32>,
}

fn cue(channel: u8, message: MidiMessage) -> Cue {
    Cue { channel, message }
}

fn note(channel: u8, key: u8) -> Cue {
    cue(channel, MidiMessage::NoteOn { key, velocity: 100 })
}

fn bank(channel: u8, msb: u8, lsb: u8) -> Cue {
    cue(
        channel,
        MidiMessage::Bank {
            bank: MidiBank::new(msb, lsb).expect("bank"),
        },
    )
}

fn program(channel: u8, program: u8) -> Cue {
    cue(channel, MidiMessage::Program { program })
}

fn play(synth: &mut dyn Synth, cues: &[Cue], frames: usize) -> Stereo {
    for cue in cues {
        synth.handle(Channel::new(cue.channel).expect("channel"), &cue.message);
    }
    let mut left = vec![0.0; frames];
    let mut right = vec![0.0; frames];
    synth.render(&mut left, &mut right);
    Stereo { left, right }
}

fn fresh(cues: &[Cue], frames: usize) -> Stereo {
    let bank = tiny_bank();
    let mut synth = bank.instantiate(RATE);
    play(synth.as_mut(), cues, frames)
}

fn peak(samples: &[f32]) -> f32 {
    samples
        .iter()
        .fold(0.0f32, |peak, sample| peak.max(sample.abs()))
}

fn rising_crossings(samples: &[f32]) -> usize {
    samples
        .windows(2)
        .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
        .count()
}

pub fn pdta_chunk(bytes: &[u8], id: &[u8; 4]) -> Range<usize> {
    let pdta = bytes
        .windows(4)
        .position(|window| window == b"pdta")
        .expect("pdta");
    let at = pdta
        + bytes[pdta..]
            .windows(4)
            .position(|window| window == id)
            .expect("chunk");
    let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().expect("size")) as usize;
    at + 8..at + 8 + size
}

pub fn chunk_header(bytes: &[u8], id: &[u8; 4]) -> usize {
    bytes
        .windows(4)
        .position(|window| window == id)
        .expect("chunk")
}

pub fn engine_panic_bank() -> Vec<u8> {
    let mut definition = tiny_definition(None);
    definition.instruments[0].zones[0]
        .generators
        .insert(0, generator(START_OFFSET_GENERATOR, i16::MAX));
    let mut bytes = write_sf2(&definition);
    let shdr = pdta_chunk(&bytes, b"shdr");
    let start = shdr.start + SAMPLE_START_FIELD;
    bytes[start..start + 4].copy_from_slice(&OVERFLOWING_START.to_le_bytes());
    bytes
}

pub fn engine_render_panic_bank() -> Vec<u8> {
    let mut bytes = tiny_bytes().to_vec();
    let shdr = pdta_chunk(&bytes, b"shdr");
    let rate = shdr.start + SAMPLE_RATE_FIELD;
    bytes[rate..rate + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    bytes
}

fn instrument(msb: u8, program: u8, name: &str) -> BankInstrument {
    BankInstrument {
        bank: MidiBank::new(msb, 0).expect("bank"),
        program,
        name: name.to_owned(),
    }
}

#[test]
fn the_tiny_bank_is_small_deterministic_and_laid_out_as_soundfont_2_04() {
    let bytes = tiny_sf2();
    assert!(bytes.len() <= MAX_TINY_BYTES, "{} bytes", bytes.len());
    assert_eq!(bytes, tiny_bytes());
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"sfbk");
    let riff = u32::from_le_bytes(bytes[4..8].try_into().expect("size")) as usize;
    assert_eq!(riff + 8, bytes.len());
    assert_eq!(&bytes[20..24], b"INFO");
    let ifil = chunk_header(&bytes, b"ifil");
    assert_eq!(&bytes[ifil + 8..ifil + 12], &[2, 0, 4, 0]);
    let sdta = chunk_header(&bytes, b"sdta");
    let pdta = chunk_header(&bytes, b"pdta");
    assert!(ifil < sdta && sdta < pdta);
    let mut previous = pdta;
    for id in PDTA_CHUNKS {
        let body = pdta_chunk(&bytes, id);
        assert!(body.start > previous, "{}", String::from_utf8_lossy(id));
        assert_eq!(body.len() % 2, 0);
        previous = body.start;
    }
    assert_eq!(pdta_chunk(&bytes, b"phdr").len() % 38, 0);
    assert_eq!(pdta_chunk(&bytes, b"inst").len() % 22, 0);
    assert_eq!(pdta_chunk(&bytes, b"shdr").len() % 46, 0);
    assert_eq!(pdta_chunk(&bytes, b"pmod").len(), 10);
    let bank = Sf2Bank::load(&bytes).expect("loads");
    assert_eq!(bank.name(), "OpenTab Tiny Test Bank");
    assert_eq!(bank.format(), BankFormat::Sf2);
}

#[test]
fn the_tiny_bank_lists_guitar_bass_and_drum_presets_by_bank_msb() {
    let expected = vec![
        instrument(0, 0, "Sine Lead"),
        instrument(0, 24, "Synth Nylon"),
        instrument(0, 25, "Synth Steel"),
        instrument(0, 27, "Synth Clean"),
        instrument(0, 29, "Synth Overdrive"),
        instrument(0, 30, "Synth Distortion"),
        instrument(0, 33, "Synth Finger Bass"),
        instrument(0, 34, "Synth Pick Bass"),
        instrument(8, 24, "Sine Nylon"),
        instrument(120, 0, "Standard Kit"),
    ];
    assert_eq!(tiny_bank().instruments(), expected);
}

#[test]
fn an_sf3_file_is_refused_with_a_message_that_asks_for_conversion() {
    let mut version_three = tiny_bytes().to_vec();
    let ifil = chunk_header(&version_three, b"ifil");
    version_three[ifil + 8] = 3;
    let mut ogg_samples = tiny_bytes().to_vec();
    let smpl = chunk_header(&ogg_samples, b"smpl");
    ogg_samples[smpl + 8..smpl + 12].copy_from_slice(b"OggS");
    for bytes in [version_three, ogg_samples] {
        let error = Sf2Bank::load(&bytes).expect_err("sf3 is refused");
        assert_eq!(
            error,
            BankError::Unsupported {
                message: SF3_UNSUPPORTED.to_owned()
            }
        );
        let text = error.to_string();
        assert!(text.contains("SF3") && text.contains("SF2") && text.contains("Convert"));
    }
}

#[test]
fn malformed_files_are_refused_with_a_typed_error_and_never_a_panic() {
    let tiny = tiny_bytes();
    let mut wave = tiny.to_vec();
    wave[8..12].copy_from_slice(b"WAVE");
    let foreign: [Vec<u8>; 3] = [Vec::new(), b"<region> sample=a.wav".to_vec(), wave];
    for bytes in &foreign {
        match Sf2Bank::load(bytes) {
            Err(BankError::Invalid { reason }) => assert!(reason.contains("not a SoundFont")),
            other => panic!("{} bytes gave {other:?}", bytes.len()),
        }
    }
    let mut huge = tiny.to_vec();
    let info = chunk_header(&huge, b"INFO") - 8;
    huge[info + 4..info + 8].copy_from_slice(&0xFFFF_FFF0u32.to_le_bytes());
    let mut version = tiny.to_vec();
    let ifil = chunk_header(&version, b"ifil");
    version[ifil + 4] = 2;
    let refusals = [
        Refusal::new(tiny[..12].to_vec(), UnsafeBank::ChunkOverflow),
        Refusal::new(tiny[..100].to_vec(), UnsafeBank::ChunkOverflow),
        Refusal::new(tiny[..tiny.len() / 2].to_vec(), UnsafeBank::ChunkOverflow),
        Refusal::new(tiny[..tiny.len() - 1].to_vec(), UnsafeBank::ChunkOverflow),
        Refusal::new(huge, UnsafeBank::ChunkOverflow),
        Refusal::new(version, UnsafeBank::VersionSize),
    ];
    for refusal in &refusals {
        assert_eq!(
            Sf2Bank::load(&refusal.bytes).map(|bank| bank.name().to_owned()),
            Err(BankError::Unsafe {
                reason: refusal.reason
            }),
            "{} bytes",
            refusal.bytes.len()
        );
    }
}

struct Refusal {
    bytes: Vec<u8>,
    reason: UnsafeBank,
}

impl Refusal {
    fn new(bytes: Vec<u8>, reason: UnsafeBank) -> Refusal {
        Refusal { bytes, reason }
    }
}

#[test]
fn a_bank_that_makes_the_engine_panic_while_loading_becomes_a_bank_error() {
    match Sf2Bank::load(&engine_panic_bank()) {
        Err(BankError::Panicked { message }) => assert!(!message.is_empty()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_variation_bank_is_selected_by_the_msb_and_the_lsb_is_ignored() {
    let frames = 4_800;
    let sine = fresh(&[program(0, 0), note(0, 69)], frames);
    let pluck = fresh(&[program(0, 24), note(0, 69)], frames);
    let variation = fresh(&[bank(0, 8, 0), program(0, 24), note(0, 69)], frames);
    let lsb_only = fresh(&[bank(0, 0, 8), program(0, 24), note(0, 69)], frames);
    assert!(peak(&sine.left) > 0.01);
    assert_ne!(sine, pluck);
    assert_eq!(variation, sine);
    assert_eq!(lsb_only, pluck);
}

#[test]
fn bank_msb_120_plays_the_kit_on_any_channel_like_channel_ten() {
    let frames = 4_800;
    let kit = fresh(&[program(9, 0), note(9, 36)], frames);
    let moved = fresh(&[bank(3, 120, 0), program(3, 0), note(3, 36)], frames);
    let melodic = fresh(&[program(3, 0), note(3, 36)], frames);
    assert!(peak(&kit.left) > 0.01);
    assert_eq!(moved, kit);
    assert_ne!(melodic, kit);
    let snare = fresh(&[note(9, 38)], frames);
    assert_ne!(snare, kit);
}

#[test]
fn the_bend_range_defaults_to_six_semitones_follows_rpn_zero_and_resets() {
    let frames = 24_000;
    let window = 4_800..frames;
    let crossings = |cues: &[Cue], synth: &mut dyn Synth| {
        let audio = play(synth, cues, frames);
        rising_crossings(&audio.left[window.clone()])
    };
    let bank = tiny_bank();
    let mut synth = bank.instantiate(RATE);
    let plain = crossings(&[note(0, 69)], synth.as_mut());
    synth.reset();
    let up_six = crossings(
        &[
            cue(0, MidiMessage::PitchBend { value: 16_383 }),
            note(0, 63),
        ],
        synth.as_mut(),
    );
    synth.reset();
    let up_twelve = crossings(
        &[
            cue(
                0,
                MidiMessage::Rpn {
                    parameter: PITCH_BEND_RANGE_RPN,
                    value: 12,
                },
            ),
            cue(0, MidiMessage::PitchBend { value: 16_383 }),
            note(0, 57),
        ],
        synth.as_mut(),
    );
    synth.reset();
    let after_reset = crossings(
        &[
            cue(0, MidiMessage::PitchBend { value: 16_383 }),
            note(0, 63),
        ],
        synth.as_mut(),
    );
    let expected = 440 * window.len() / RATE.hz() as usize;
    assert_near("plain", plain, expected);
    assert_near("six", up_six, expected);
    assert_near("twelve", up_twelve, expected);
    assert_near("reset", after_reset, expected);
}

fn assert_near(label: &str, measured: usize, expected: usize) {
    assert!(
        measured.abs_diff(expected) <= 2,
        "{label}: {measured} crossings, expected {expected}"
    );
}

#[test]
fn render_overwrites_both_buffers_and_zeroes_the_longer_tail() {
    let bank = tiny_bank();
    let mut synth = bank.instantiate(RATE);
    synth.handle(
        Channel::new(0).expect("channel"),
        &MidiMessage::NoteOn {
            key: 69,
            velocity: 100,
        },
    );
    let mut left = vec![9.0f32; 300];
    let mut right = vec![9.0f32; 200];
    synth.render(&mut left, &mut right);
    assert!(left[200..].iter().all(|sample| *sample == 0.0));
    assert!(peak(&left[..200]) > 0.0 && peak(&left[..200]) < 9.0);
    assert!(peak(&right) < 9.0);
}

#[test]
fn every_track_shares_one_sound_font() {
    let bank = tiny_bank();
    assert_eq!(bank.shared_count(), 1);
    let voices: Vec<Box<dyn Synth>> = (0..5).map(|_| bank.instantiate(RATE)).collect();
    assert_eq!(bank.shared_count(), 1 + 5 * REFERENCES_PER_TRACK);
    drop(voices);
    assert_eq!(bank.shared_count(), 1);
    assert!(bank.faults().is_empty());
}

#[test]
fn a_bank_that_makes_the_engine_panic_while_rendering_silences_the_track_and_reports_it() {
    let bank = Sf2Bank::load(&engine_render_panic_bank()).expect("the header loads");
    let mut synth = bank.synth(RATE);
    let audio = play(&mut synth, &[program(0, 0), note(0, 69)], 1_024);
    assert!(synth.is_silenced());
    assert!(
        audio
            .left
            .iter()
            .chain(&audio.right)
            .all(|sample| *sample == 0.0)
    );
    let faults = bank.faults();
    assert_eq!(faults.len(), 1);
    assert_eq!(faults[0].stage, SynthStage::Render);
    assert_eq!(synth.fault(), Some(&faults[0]));
    synth.reset();
    assert!(!synth.is_silenced());
    let pluck = play(&mut synth, &[program(0, 24), note(0, 57)], 4_800);
    assert!(peak(&pluck.left) > 0.01);
    assert_eq!(bank.faults().len(), 1);
}

#[test]
fn the_wrapper_skips_a_note_that_would_start_more_voices_than_allowed_and_reports_it() {
    let frames = 4_800;
    let layered_program = LAYERED_PROGRAM as u8;
    let single_program = SINGLE_PROGRAM as u8;
    let bytes = layered_sf2(2, 1);
    let open = Sf2Bank::load(&bytes).expect("loads");
    let mut free = open.synth(RATE);
    let both = play(
        &mut free,
        &[program(0, layered_program), note(0, 69)],
        frames,
    );
    assert!(peak(&both.left) > 0.01);
    assert_eq!(free.skipped_notes(), 0);
    let strict = Sf2Limits {
        voices_per_note: 1,
        ..Sf2Limits::DEFAULT
    };
    let bank = Sf2Bank::load_with_limits(&bytes, &strict).expect("loads");
    let mut synth = bank.synth(RATE);
    let skipped = play(
        &mut synth,
        &[program(0, layered_program), note(0, 69)],
        frames,
    );
    assert!(
        skipped
            .left
            .iter()
            .chain(&skipped.right)
            .all(|sample| *sample == 0.0)
    );
    assert_eq!(synth.skipped_notes(), 1);
    let single = play(
        &mut synth,
        &[program(0, single_program), note(0, 69)],
        frames,
    );
    assert!(peak(&single.left) > 0.01);
    let again = play(
        &mut synth,
        &[program(0, layered_program), note(0, 72)],
        frames,
    );
    assert_eq!(synth.skipped_notes(), 2);
    assert!(!synth.is_silenced());
    assert!(peak(&again.left) > 0.0);
    let faults = bank.faults();
    assert_eq!(faults.len(), 1);
    assert_eq!(faults[0].stage, SynthStage::Layers);
}

fn control(channel: u8, controller: u8, value: u8) -> Cue {
    cue(channel, MidiMessage::Control { controller, value })
}

#[test]
fn every_guitar_and_bass_program_of_the_tiny_bank_sounds_different() {
    let renders: Vec<Stereo> = [24u8, 25, 27, 29, 30, 33, 34]
        .into_iter()
        .map(|number| fresh(&[program(0, number), note(0, 45)], 4_800))
        .collect();
    for (index, render) in renders.iter().enumerate() {
        assert!(peak(&render.left) > 0.001, "{index}");
        for other in &renders[index + 1..] {
            assert_ne!(render, other);
        }
    }
}

#[test]
fn channel_volume_and_pan_reach_the_engine() {
    let frames = 4_800;
    let silent = fresh(&[control(0, 7, 0), note(0, 69)], frames);
    assert!(
        silent
            .left
            .iter()
            .chain(&silent.right)
            .all(|sample| *sample == 0.0)
    );
    let centre = fresh(&[note(0, 69)], frames);
    let left = fresh(&[control(0, 10, 0), note(0, 69)], frames);
    let right = fresh(&[control(0, 10, 127), note(0, 69)], frames);
    assert!(peak(&left.right) * 2.0 < peak(&centre.right));
    assert!(peak(&left.left) > peak(&centre.left));
    assert!(peak(&right.left) * 2.0 < peak(&centre.left));
    assert!(peak(&right.right) > peak(&centre.right));
}

#[test]
fn a_note_off_on_one_channel_leaves_the_same_key_sounding_on_another() {
    let frames = 48_000;
    let window = 38_400..frames;
    let off = |channel: u8| {
        cue(
            channel,
            MidiMessage::NoteOff {
                key: 69,
                velocity: 64,
            },
        )
    };
    let held = fresh(&[note(0, 69), note(1, 69), off(1)], frames);
    let released = fresh(&[note(0, 69), note(1, 69), off(1), off(0)], frames);
    let held_peak = peak(&held.left[window.clone()]);
    let released_peak = peak(&released.left[window]);
    assert!(held_peak > 0.05, "{held_peak}");
    assert!(
        held_peak > 10.0 * released_peak,
        "{held_peak} {released_peak}"
    );
}

#[test]
fn the_fault_log_stops_growing_at_its_limit() {
    let bank = Sf2Bank::load(&engine_render_panic_bank()).expect("loads");
    let mut synth = bank.synth(RATE);
    for _ in 0..FAULT_LOG_LIMIT + 10 {
        play(&mut synth, &[program(0, 0), note(0, 69)], 128);
        synth.reset();
    }
    assert_eq!(bank.faults().len(), FAULT_LOG_LIMIT);
}
