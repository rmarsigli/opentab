use std::fs;
use std::path::Path;

use opentab_audio::{
    Bank, BankFormat, IgnoreReason, LoopMode, OffMode, SampleRate, SfzBank, SfzScope, SoundUse,
    Synth, Trigger,
};
use opentab_model::MidiBank;
use opentab_playback::{BEND_RANGE_SEMITONES, Channel, MidiMessage, PITCH_BEND_RANGE_RPN};
use opentab_testkit::banks::{SIDECAR_FILE, samples, wav16};

use crate::support::{scratch_dir, tiny_sfz_bank, tiny_sfz_root};

const RATE: SampleRate = SampleRate::STUDIO;

fn channel(number: u8) -> Channel {
    Channel::new(number).expect("channel")
}

fn note_on(key: u8, velocity: u8) -> MidiMessage {
    MidiMessage::NoteOn { key, velocity }
}

fn note_off(key: u8) -> MidiMessage {
    MidiMessage::NoteOff { key, velocity: 0 }
}

pub fn render(synth: &mut dyn Synth, frames: usize) -> (Vec<f32>, Vec<f32>) {
    let mut left = vec![0.0f32; frames];
    let mut right = vec![0.0f32; frames];
    synth.render(&mut left, &mut right);
    (left, right)
}

pub fn energy(samples: &[f32]) -> f64 {
    samples.iter().map(|value| f64::from(*value).powi(2)).sum()
}

pub fn write_bank(
    dir: &Path,
    sfz: &[(&str, &str)],
    waves: &[(&str, Vec<u8>)],
) -> std::path::PathBuf {
    let root = dir.join("bank");
    for (name, text) in sfz {
        let path = root.join(name);
        fs::create_dir_all(path.parent().expect("parent")).expect("folder");
        fs::write(path, text).expect("sfz written");
    }
    for (name, bytes) in waves {
        let path = root.join(name);
        fs::create_dir_all(path.parent().expect("parent")).expect("folder");
        fs::write(path, bytes).expect("sample written");
    }
    root
}

#[test]
fn the_tiny_bank_lists_its_root_instruments_and_never_its_fragments() {
    let bank = tiny_sfz_bank();
    assert_eq!(bank.format(), BankFormat::Sfz);
    assert_eq!(
        bank.instrument_names(),
        vec![
            "Programs/bass",
            "Programs/drum kit",
            "Programs/guitar",
            "Programs/lead"
        ]
    );
    assert!(bank.skipped().is_empty());
    let listed = bank.instruments();
    let names: Vec<(u8, u8, &str)> = listed
        .iter()
        .map(|entry| (entry.bank.msb(), entry.program, entry.name.as_str()))
        .collect();
    assert_eq!(
        names,
        vec![
            (0, 32, "Programs/bass"),
            (120, 0, "Programs/drum kit"),
            (0, 24, "Programs/guitar"),
            (0, 0, "Programs/lead"),
        ]
    );
    assert_eq!(bank.sample_count(), 9);
}

#[test]
fn the_tiny_bank_resolves_its_regions_through_includes_defines_and_paths() {
    let bank = tiny_sfz_bank();
    let guitar = bank.regions("Programs/guitar");
    assert_eq!(guitar.len(), 2);
    assert_eq!(guitar[0].pitch_keycenter, 57);
    assert_eq!(guitar[0].sample, "..\\Samples\\Pluck A3.wav");
    assert_eq!((guitar[1].lovel, guitar[1].hivel), (0, 63));
    let kit = bank.regions("Programs/drum kit");
    assert_eq!(kit.len(), 8);
    assert_eq!((kit[0].lokey, kit[0].hikey), (36, 36));
    assert_eq!(kit[0].loop_mode, LoopMode::OneShot);
    let open = kit
        .iter()
        .find(|region| region.lokey == 46)
        .expect("open hat");
    assert_eq!(
        (open.group, open.off_by, open.off_mode),
        (2, 1, OffMode::Normal)
    );
    let bass = bank.regions("Programs/bass");
    assert_eq!(bass[0].loop_mode, LoopMode::Sustain);
    assert_eq!(
        (bass[0].loop_start, bass[0].loop_end),
        (Some(535), Some(1604))
    );
    let ignored = bank.ignored("Programs/guitar");
    let has = |name: &str, reason: IgnoreReason| {
        ignored
            .iter()
            .any(|entry| entry.name == name && entry.reason == reason)
    };
    assert!(has("lfoN_freq", IgnoreReason::Unsupported));
    assert!(has("cutoff", IgnoreReason::Unsupported));
    assert!(has("<curve>", IgnoreReason::Unsupported));
    assert!(has("trigger", IgnoreReason::ReleaseTrigger));
    assert_eq!(guitar[0].trigger, Trigger::Attack);
    let global = ignored
        .iter()
        .find(|entry| entry.name == "cutoff")
        .expect("cutoff listed");
    assert_eq!(global.scope, SfzScope::Global);
}

#[test]
fn the_sidecar_maps_programs_and_drums() {
    let bank = tiny_sfz_bank();
    assert_eq!(
        bank.melodic_instrument(25).as_deref(),
        Some("Programs/guitar")
    );
    assert_eq!(
        bank.melodic_instrument(33).as_deref(),
        Some("Programs/bass")
    );
    assert_eq!(bank.melodic_instrument(0).as_deref(), Some("Programs/lead"));
    assert_eq!(
        bank.melodic_instrument(73).as_deref(),
        Some("Programs/lead")
    );
    assert_eq!(bank.drum_instrument().as_deref(), Some("Programs/drum kit"));
}

#[test]
fn without_a_sidecar_the_first_instrument_serves_every_program_and_drums_use_the_drum_file() {
    let dir = scratch_dir();
    let root = opentab_testkit::banks::tiny_sfz(dir.path());
    fs::remove_file(root.join(SIDECAR_FILE)).expect("sidecar removed");
    let bank = SfzBank::load(&root).expect("loads");
    for program in [0, 24, 33, 127] {
        assert_eq!(
            bank.melodic_instrument(program).as_deref(),
            Some("Programs/bass")
        );
    }
    assert_eq!(bank.drum_instrument().as_deref(), Some("Programs/drum kit"));
}

#[test]
fn a_broken_sidecar_is_refused() {
    let dir = scratch_dir();
    let root = opentab_testkit::banks::tiny_sfz(dir.path());
    for text in [
        "{",
        r#"{"version": 2}"#,
        r#"{"version": 1, "drums": "Programs/none.sfz"}"#,
        r#"{"version": 1, "programs": [{"program": 200, "file": "Programs/bass.sfz"}]}"#,
        r#"{"version": 1, "surprise": true}"#,
    ] {
        fs::write(root.join(SIDECAR_FILE), text).expect("sidecar written");
        assert!(
            matches!(
                SfzBank::load(&root),
                Err(opentab_audio::BankError::Invalid { .. })
            ),
            "{text}"
        );
    }
}

#[test]
fn every_tiny_instrument_sounds_and_decoding_is_shared() {
    let bank = tiny_sfz_bank();
    let mut first = bank.synth(RATE);
    first.prepare_all();
    let mut second = bank.synth(RATE);
    second.prepare_all();
    for (program, key) in [(24u8, 57u8), (33, 40), (0, 72)] {
        for synth in [&mut first, &mut second] {
            synth.reset();
            synth.handle(channel(0), &MidiMessage::Program { program });
            synth.handle(channel(0), &note_on(key, 100));
            let (left, right) = render(synth, 4800);
            assert!(
                energy(&left) > 1.0 && energy(&right) > 1.0,
                "program {program}"
            );
        }
    }
    for key in [36u8, 38, 42, 46, 60] {
        first.handle(Channel::DRUMS, &note_on(key, 100));
        let (left, _) = render(&mut first, 2400);
        assert!(energy(&left) > 0.1, "drum key {key}");
    }
    assert!(bank.faults().is_empty(), "{:?}", bank.faults());
    assert!(bank.decode_count() <= bank.sample_count() as u64);
    assert_eq!(bank.shared_count(), 3);
}

#[test]
fn a_missing_sample_is_listed_and_the_bank_still_loads() {
    let dir = scratch_dir();
    let root = write_bank(
        dir.path(),
        &[(
            "a.sfz",
            "<region> sample=gone.wav\n<region> sample=there.wav key=60\n<region> sample=song.aif",
        )],
        &[
            ("there.wav", wav16(&samples::pluck())),
            ("song.aif", vec![0; 16]),
        ],
    );
    let bank = SfzBank::load(&root).expect("loads");
    assert_eq!(bank.regions("a").len(), 1);
    let ignored = bank.ignored("a");
    assert!(
        ignored
            .iter()
            .any(|entry| entry.reason == IgnoreReason::MissingFile)
    );
    assert!(
        ignored
            .iter()
            .any(|entry| entry.reason == IgnoreReason::UnsupportedAudioFormat)
    );
}

#[test]
fn bend_follows_the_rpn_range() {
    let bank = tiny_sfz_bank();
    let crossings = |bend: u16, range: u8| {
        let mut synth = bank.synth(RATE);
        synth.prepare_all();
        synth.handle(
            channel(0),
            &MidiMessage::Rpn {
                parameter: PITCH_BEND_RANGE_RPN,
                value: range,
            },
        );
        synth.handle(channel(0), &MidiMessage::Program { program: 33 });
        synth.handle(channel(0), &MidiMessage::PitchBend { value: bend });
        synth.handle(channel(0), &note_on(40, 127));
        let (left, _) = render(&mut synth, 48_000);
        left.windows(2)
            .filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
            .count() as f64
    };
    let centre = crossings(8192, BEND_RANGE_SEMITONES);
    let up_six = crossings(16383, BEND_RANGE_SEMITONES);
    let up_twelve = crossings(16383, 12);
    let ratio_six = up_six / centre;
    let ratio_twelve = up_twelve / centre;
    assert!(
        (ratio_six - 2f64.powf(6.0 / 12.0)).abs() < 0.03,
        "{ratio_six}"
    );
    assert!((ratio_twelve - 2.0).abs() < 0.05, "{ratio_twelve}");
}

#[test]
fn note_off_releases_and_one_shots_ignore_it() {
    let bank = tiny_sfz_bank();
    let mut synth = bank.synth(RATE);
    synth.prepare_all();
    synth.handle(channel(0), &MidiMessage::Program { program: 33 });
    synth.handle(channel(0), &note_on(40, 100));
    render(&mut synth, 4800);
    synth.handle(channel(0), &note_off(40));
    render(&mut synth, 48_000);
    assert_eq!(synth.sounding(), 0);
    synth.handle(Channel::DRUMS, &note_on(36, 100));
    render(&mut synth, 480);
    synth.handle(Channel::DRUMS, &note_off(36));
    let (left, _) = render(&mut synth, 4800);
    assert!(energy(&left[2400..]) > 0.01);
    let _ = tiny_sfz_root();
}

#[test]
fn a_bank_without_a_drum_file_plays_no_drums() {
    let dir = scratch_dir();
    let root = write_bank(
        dir.path(),
        &[("guitar.sfz", "<region> sample=pluck.wav")],
        &[("pluck.wav", wav16(&samples::pluck()))],
    );
    let bank = SfzBank::load(&root).expect("loads");
    assert_eq!(bank.drum_instrument(), None);
    assert_eq!(bank.melodic_instrument(24).as_deref(), Some("guitar"));
    assert!(bank.instruments().iter().all(|entry| entry.bank.msb() == 0));
    let mut synth = bank.synth(RATE);
    synth.prepare_all();
    synth.handle(Channel::DRUMS, &note_on(38, 100));
    assert_eq!(synth.sounding(), 0);
    synth.handle(channel(0), &note_on(60, 100));
    assert_eq!(synth.sounding(), 1);
}

#[test]
fn an_unprepared_sample_is_skipped_and_counted_and_never_decoded_on_note_on() {
    let bank = tiny_sfz_bank();
    let mut synth = bank.synth(RATE);
    synth.handle(channel(0), &MidiMessage::Program { program: 24 });
    synth.handle(channel(0), &note_on(57, 100));
    assert_eq!(synth.sounding(), 0);
    assert_eq!(energy(&render(&mut synth, 4800).0), 0.0);
    assert_eq!(bank.decode_count(), 0);
    let skipped = bank.unprepared_count();
    assert!(skipped > 0);
    synth.prepare(&[SoundUse {
        channel: channel(0),
        bank: MidiBank::default(),
        program: 24,
    }]);
    let prepared = bank.decode_count();
    assert!(prepared > 0);
    synth.handle(channel(0), &note_on(57, 100));
    assert!(synth.sounding() > 0);
    assert!(energy(&render(&mut synth, 4800).0) > 1.0);
    assert_eq!(bank.unprepared_count(), skipped);
    synth.handle(channel(0), &MidiMessage::Program { program: 33 });
    synth.handle(channel(0), &note_on(40, 100));
    assert!(bank.unprepared_count() > skipped);
    assert_eq!(bank.decode_count(), prepared);
    assert!(bank.faults().is_empty(), "{:?}", bank.faults());
}
