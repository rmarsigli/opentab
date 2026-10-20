use std::collections::BTreeMap;

use opentab_audio::{
    IgnoreReason, InitialController, LoopMode, OffMode, SampleRate, SfzBank, SfzLimits, SfzRegion,
    SfzSynth, SfzText, Synth, Trigger, parse_sfz_text,
};
use opentab_playback::{Channel, MidiMessage};
use opentab_testkit::banks::{Waveform, samples, wav16};

use crate::sfz::{SampleFile, Stereo, energy, render, sample_file, text_file, write_bank};
use crate::support::scratch_dir;

const RATE: SampleRate = SampleRate::STUDIO;
const SECOND: usize = 48_000;
const ENVELOPE_BLOCK: usize = 1088;

fn text(sfz: &str) -> SfzText {
    parse_sfz_text(sfz, &BTreeMap::new(), &SfzLimits::DEFAULT).expect("parses")
}

fn only(sfz: &str) -> SfzRegion {
    let parsed = text(sfz);
    assert_eq!(parsed.regions.len(), 1, "{sfz}");
    parsed.regions[0].clone()
}

fn listed(parsed: &SfzText, name: &str, reason: IgnoreReason) -> bool {
    parsed
        .ignored
        .iter()
        .any(|entry| entry.name == name && entry.reason == reason)
}

struct TestBank {
    _dir: tempfile::TempDir,
    bank: SfzBank,
}

fn waves() -> Vec<SampleFile<'static>> {
    vec![
        sample_file("sine.wav", wav16(&samples::sine_loop())),
        sample_file("pluck.wav", wav16(&samples::pluck())),
        sample_file("kick.wav", wav16(&samples::kick())),
        sample_file("hat.wav", wav16(&samples::hat())),
        sample_file("open.wav", wav16(&samples::open_hat())),
        sample_file("snare.wav", wav16(&samples::snare())),
    ]
}

fn bank(sfz: &str) -> TestBank {
    let dir = scratch_dir();
    let root = write_bank(dir.path(), &[text_file("test.sfz", sfz)], &waves());
    TestBank {
        bank: SfzBank::load(&root).expect("loads"),
        _dir: dir,
    }
}

fn channel() -> Channel {
    Channel::new(0).expect("channel")
}

fn on(synth: &mut SfzSynth, key: u8, velocity: u8) {
    synth.handle(channel(), &MidiMessage::NoteOn { key, velocity });
}

fn off(synth: &mut SfzSynth, key: u8) {
    synth.handle(channel(), &MidiMessage::NoteOff { key, velocity: 0 });
}

fn cc(synth: &mut SfzSynth, controller: u8, value: u8) {
    synth.handle(channel(), &MidiMessage::Control { controller, value });
}

fn played(bank: &TestBank, key: u8, velocity: u8, frames: usize) -> Vec<f32> {
    let mut synth = bank.bank.synth(RATE);
    synth.prepare_all();
    on(&mut synth, key, velocity);
    render(&mut synth, frames).left
}

fn crossings_per_second(samples: &[f32]) -> f64 {
    let count = samples
        .windows(2)
        .filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
        .count();
    count as f64 * SECOND as f64 / samples.len() as f64
}

fn looped_sine(extra: &str) -> String {
    format!(
        "<region> sample=sine.wav loop_mode=loop_continuous loop_start=200 loop_end=599 pitch_keycenter=69 {extra}\n"
    )
}

fn frequency(bank: &TestBank, key: u8) -> f64 {
    crossings_per_second(&played(bank, key, 100, SECOND))
}

fn sounding_length(samples: &[f32]) -> usize {
    samples
        .iter()
        .rposition(|value| value.abs() > 1e-6)
        .map_or(0, |last| last + 1)
}

fn rms(samples: &[f32]) -> f64 {
    (energy(samples) / samples.len().max(1) as f64).sqrt()
}

#[test]
fn hikey_and_lokey_select_regions() {
    let test = bank(
        "<region> lokey=0 hikey=59 sample=kick.wav\n<region> lokey=60 hikey=127 sample=hat.wav\n",
    );
    for key in [30u8, 59, 60, 90] {
        let mut synth = test.bank.synth(RATE);
        synth.prepare_all();
        on(&mut synth, key, 100);
        assert_eq!(synth.sounding(), 1, "key {key}");
    }
    let parsed = text("<region> lokey=c4 hikey=b4 sample=a.wav");
    assert_eq!((parsed.regions[0].lokey, parsed.regions[0].hikey), (60, 71));
}

#[test]
fn key_sets_the_range_and_the_centre() {
    let region = only("<region> key=40 sample=a.wav");
    assert_eq!(
        (region.lokey, region.hikey, region.pitch_keycenter),
        (40, 40, 40)
    );
    let test = bank("<region> key=40 sample=kick.wav");
    let mut synth = test.bank.synth(RATE);
    synth.prepare_all();
    on(&mut synth, 41, 100);
    assert_eq!(synth.sounding(), 0);
    on(&mut synth, 40, 100);
    assert_eq!(synth.sounding(), 1);
}

#[test]
fn lovel_and_hivel_select_layers_and_an_inverted_range_never_sounds() {
    let test = bank(
        "<region> lovel=0 hivel=63 sample=kick.wav\n<region> lovel=64 hivel=127 sample=hat.wav\n<region> lovel=101 hivel=100 sample=snare.wav\n",
    );
    for velocity in [1u8, 63, 64, 127] {
        let mut synth = test.bank.synth(RATE);
        synth.prepare_all();
        on(&mut synth, 60, velocity);
        assert_eq!(synth.sounding(), 1, "velocity {velocity}");
    }
}

#[test]
fn pitch_keycenter_tune_transpose_and_keytrack_set_the_pitch() {
    let base = bank(&looped_sine(""));
    let at_centre = frequency(&base, 69);
    assert!((at_centre - 441.0).abs() < 3.0, "{at_centre}");
    let octave = frequency(&base, 81);
    assert!((octave / at_centre - 2.0).abs() < 0.02, "{octave}");
    let tuned = bank(&looped_sine("tune=-1200"));
    assert!((frequency(&tuned, 69) / at_centre - 0.5).abs() < 0.01);
    let transposed = bank(&looped_sine("transpose=12"));
    assert!((frequency(&transposed, 69) / at_centre - 2.0).abs() < 0.02);
    let fixed = bank(&looped_sine("pitch_keytrack=0"));
    assert!((frequency(&fixed, 93) / at_centre - 1.0).abs() < 0.01);
    let centred = bank(&looped_sine("pitch_keycenter=57"));
    assert!((frequency(&centred, 69) / at_centre - 2.0).abs() < 0.02);
}

#[test]
fn volume_amplitude_and_pan_set_the_level() {
    let loud = rms(&played(&bank(&looped_sine("")), 69, 127, SECOND / 4));
    let quiet = rms(&played(
        &bank(&looped_sine("volume=-6")),
        69,
        127,
        SECOND / 4,
    ));
    assert!((20.0 * (quiet / loud).log10() + 6.0).abs() < 0.1);
    let half = rms(&played(
        &bank(&looped_sine("amplitude=50")),
        69,
        127,
        SECOND / 4,
    ));
    assert!((half / loud - 0.5).abs() < 0.01);
    let panned = bank(&looped_sine("pan=-100"));
    let mut synth = panned.bank.synth(RATE);
    synth.prepare_all();
    on(&mut synth, 69, 127);
    let Stereo { left, right } = render(&mut synth, SECOND / 4);
    assert!(energy(&right) < energy(&left) * 1e-6);
    let centre = bank(&looped_sine(""));
    let mut synth = centre.bank.synth(RATE);
    synth.prepare_all();
    on(&mut synth, 69, 127);
    let Stereo { left, right } = render(&mut synth, SECOND / 4);
    assert!((energy(&left) / energy(&right) - 1.0).abs() < 1e-6);
}

#[test]
fn amp_veltrack_and_amp_velcurve_shape_velocity() {
    let flat = bank(&looped_sine("amp_veltrack=0"));
    let soft = rms(&played(&flat, 69, 20, SECOND / 4));
    let hard = rms(&played(&flat, 69, 127, SECOND / 4));
    assert!((soft / hard - 1.0).abs() < 1e-6);
    let tracked = bank(&looped_sine(""));
    let soft = rms(&played(&tracked, 69, 64, SECOND / 4));
    let hard = rms(&played(&tracked, 69, 127, SECOND / 4));
    let expected = (64.0f64 / 127.0).powi(2);
    assert!((soft / hard - expected).abs() < 0.01, "{}", soft / hard);
    let curved = bank(&looped_sine("amp_velcurve_64=1"));
    let soft = rms(&played(&curved, 69, 64, SECOND / 4));
    let hard = rms(&played(&curved, 69, 127, SECOND / 4));
    assert!((soft / hard - 1.0).abs() < 1e-3);
    let region = only("<region> sample=a.wav amp_velcurve_100=0.5 amp_velcurve_20=0.1");
    assert_eq!(region.amp_velcurve.len(), 2);
    assert!((region.velocity_gain(60) - 0.3).abs() < 1e-6);
    assert!((region.velocity_gain(127) - 0.5).abs() < 1e-6);
}

#[test]
fn the_amplitude_envelope_follows_attack_hold_decay_sustain_and_release() {
    let test = bank(&looped_sine(
        "ampeg_attack=0.1 ampeg_hold=0.1 ampeg_decay=0.1 ampeg_sustain=25 ampeg_release=0.2",
    ));
    let mut synth = test.bank.synth(RATE);
    synth.prepare_all();
    on(&mut synth, 69, 127);
    let Stereo { left, .. } = render(&mut synth, SECOND / 2);
    let block = |from: f64| {
        let start = (from * SECOND as f64) as usize;
        rms(&left[start..start + ENVELOPE_BLOCK])
    };
    assert!(block(0.0) < block(0.08) * 0.2);
    assert!((block(0.12) / block(0.175) - 1.0).abs() < 0.02);
    assert!((block(0.35) / block(0.15) - 0.25).abs() < 0.02);
    off(&mut synth, 69);
    let Stereo { left: tail, .. } = render(&mut synth, SECOND / 2);
    let length = sounding_length(&tail) as f64 / SECOND as f64;
    assert!((length - 0.2).abs() < 0.005, "{length}");
    assert_eq!(synth.sounding(), 0);
}

#[test]
fn loop_modes_decide_how_a_sample_ends() {
    let frames = samples::sine_loop().points.len() as f64 * 48_000.0 / 44_100.0;
    let plain = bank("<region> sample=sine.wav pitch_keycenter=69");
    let length = sounding_length(&played(&plain, 69, 127, SECOND)) as f64;
    assert!((length - frames).abs() < 3.0, "{length} {frames}");
    let looped = bank(&looped_sine(""));
    assert_eq!(sounding_length(&played(&looped, 69, 127, SECOND)), SECOND);
    let sustain = bank(
        "<region> sample=sine.wav loop_mode=loop_sustain loop_start=200 loop_end=599 pitch_keycenter=69 ampeg_release=5",
    );
    let mut synth = sustain.bank.synth(RATE);
    synth.prepare_all();
    on(&mut synth, 69, 127);
    render(&mut synth, SECOND / 10);
    off(&mut synth, 69);
    let Stereo { left: tail, .. } = render(&mut synth, SECOND / 10);
    assert!(sounding_length(&tail) < 600, "{}", sounding_length(&tail));
    let shot = bank("<region> sample=kick.wav loop_mode=one_shot");
    let mut synth = shot.bank.synth(RATE);
    synth.prepare_all();
    on(&mut synth, 60, 127);
    off(&mut synth, 60);
    let Stereo { left: tail, .. } = render(&mut synth, SECOND);
    assert!(sounding_length(&tail) > SECOND / 4);
    assert_eq!(
        only("<region> sample=a.wav loopmode=one_shot").loop_mode,
        LoopMode::OneShot
    );
}

#[test]
fn offset_and_end_trim_the_sample_and_end_minus_one_disables_it() {
    let full = sounding_length(&played(&bank("<region> sample=kick.wav"), 60, 127, SECOND));
    let trimmed = sounding_length(&played(
        &bank("<region> sample=kick.wav offset=4410 end=8819"),
        60,
        127,
        SECOND,
    ));
    let expected = 4410.0 * 48_000.0 / 44_100.0;
    assert!((trimmed as f64 - expected).abs() < 3.0, "{trimmed}");
    assert!(full > trimmed * 3);
    let parsed = text("<region> sample=a.wav end=-1\n<region> sample=b.wav");
    assert_eq!(parsed.regions.len(), 1);
    assert!(listed(&parsed, "end", IgnoreReason::DisabledRegion));
}

#[test]
fn a_closed_hat_chokes_the_open_hat_through_off_by() {
    let test = bank(
        "<group> group=1\n<region> key=42 sample=hat.wav loop_mode=one_shot\n<group> group=2 off_by=1\n<region> key=46 sample=open.wav loop_mode=one_shot\n",
    );
    let mut synth = test.bank.synth(RATE);
    synth.prepare_all();
    on(&mut synth, 46, 127);
    render(&mut synth, 480);
    on(&mut synth, 42, 127);
    assert_eq!(synth.sounding(), 2);
    render(&mut synth, SECOND / 50);
    assert_eq!(synth.sounding(), 1);
    let mut free = test.bank.synth(RATE);
    free.prepare_all();
    on(&mut free, 46, 127);
    render(&mut free, 480);
    on(&mut free, 46, 127);
    render(&mut free, SECOND / 50);
    assert_eq!(free.sounding(), 2);
}

#[test]
fn voices_of_one_note_never_choke_each_other() {
    let test = bank(
        "<group> group=1 off_by=1\n<region> key=42 sample=hat.wav\n<region> key=42 sample=snare.wav\n",
    );
    let mut synth = test.bank.synth(RATE);
    synth.prepare_all();
    on(&mut synth, 42, 127);
    render(&mut synth, SECOND / 50);
    assert_eq!(synth.sounding(), 2);
    on(&mut synth, 42, 127);
    render(&mut synth, SECOND / 50);
    assert_eq!(synth.sounding(), 2);
}

#[test]
fn off_mode_sets_the_choke_fade() {
    let tail = |mode: &str| {
        let test = bank(&format!(
            "<group> group=1\n<region> key=42 sample=hat.wav\n<group> group=2 off_by=1 ampeg_release=0.05 {mode}\n<region> key=46 sample=sine.wav loop_mode=loop_continuous loop_start=200 loop_end=599\n"
        ));
        let mut synth = test.bank.synth(RATE);
        synth.prepare_all();
        on(&mut synth, 46, 127);
        render(&mut synth, 480);
        on(&mut synth, 42, 127);
        let mut frames = 0;
        while synth.sounding() > 1 && frames < SECOND {
            render(&mut synth, 48);
            frames += 48;
        }
        frames
    };
    let fast = tail("off_mode=fast");
    let normal = tail("off_mode=normal");
    let timed = tail("off_mode=time off_time=0.02");
    assert!((fast as i64 - 288).abs() <= 48, "{fast}");
    assert!((normal as i64 - 2400).abs() <= 48, "{normal}");
    assert!((timed as i64 - 960).abs() <= 48, "{timed}");
    assert_eq!(
        only("<region> sample=a.wav off_mode=time").off_mode,
        OffMode::Time
    );
}

#[test]
fn lorand_and_hirand_pick_regions_deterministically() {
    let test = bank(
        "<region> lorand=0 hirand=0.5 sample=kick.wav\n<region> lorand=0.5 hirand=1 sample=hat.wav\n",
    );
    let picks = |bank: &TestBank| {
        let mut synth = bank.bank.synth(RATE);
        synth.prepare_all();
        (0..64)
            .map(|_| {
                synth.reset();
                on(&mut synth, 60, 100);
                synth.sounding()
            })
            .collect::<Vec<usize>>()
    };
    assert!(picks(&test).iter().all(|count| *count == 1));
    let mut synth = test.bank.synth(RATE);
    synth.prepare_all();
    let mut lengths = Vec::new();
    for _ in 0..32 {
        on(&mut synth, 60, 100);
        lengths.push(render(&mut synth, 24_000).left[100].to_bits());
        off(&mut synth, 60);
        render(&mut synth, 24_000);
    }
    lengths.sort_unstable();
    lengths.dedup();
    assert_eq!(lengths.len(), 2);
}

#[test]
fn seq_length_and_seq_position_alternate_regions() {
    let test = bank(
        "<group> seq_length=2\n<region> seq_position=1 sample=kick.wav\n<region> seq_position=2 sample=hat.wav\n",
    );
    let mut synth = test.bank.synth(RATE);
    synth.prepare_all();
    let mut firsts = Vec::new();
    for _ in 0..4 {
        on(&mut synth, 60, 127);
        firsts.push(render(&mut synth, 4410).left[2000].to_bits());
        synth.handle(
            channel(),
            &MidiMessage::Control {
                controller: 120,
                value: 0,
            },
        );
    }
    assert_eq!(firsts[0], firsts[2]);
    assert_eq!(firsts[1], firsts[3]);
    assert_ne!(firsts[0], firsts[1]);
}

#[test]
fn trigger_first_and_legato_follow_held_notes_and_release_regions_are_dropped() {
    let test = bank(
        "<region> trigger=first sample=kick.wav\n<region> trigger=legato sample=hat.wav\n<region> trigger=release sample=snare.wav\n",
    );
    let mut synth = test.bank.synth(RATE);
    synth.prepare_all();
    on(&mut synth, 60, 100);
    assert_eq!(synth.sounding(), 1);
    on(&mut synth, 62, 100);
    assert_eq!(synth.sounding(), 2);
    off(&mut synth, 60);
    off(&mut synth, 62);
    render(&mut synth, SECOND);
    on(&mut synth, 64, 100);
    assert_eq!(synth.sounding(), 1);
    assert_eq!(test.bank.regions("test").len(), 2);
    assert!(
        test.bank
            .ignored("test")
            .iter()
            .any(|entry| entry.reason == IgnoreReason::ReleaseTrigger)
    );
    assert_eq!(
        only("<region> sample=a.wav trigger=legato").trigger,
        Trigger::Legato
    );
}

#[test]
fn keyswitches_are_fixed_by_sw_default_or_the_lowest_sw_last() {
    let lowest = bank("<region> sw_last=24 sample=kick.wav\n<region> sw_last=25 sample=hat.wav\n");
    let mut synth = lowest.bank.synth(RATE);
    synth.prepare_all();
    on(&mut synth, 60, 100);
    assert_eq!(synth.sounding(), 1);
    let chosen = bank(
        "<global> sw_lokey=24 sw_hikey=26 sw_default=25\n<region> sw_last=24 sample=kick.wav\n<region> sw_last=25 sample=hat.wav\n<region> sample=snare.wav\n",
    );
    let mut synth = chosen.bank.synth(RATE);
    synth.prepare_all();
    on(&mut synth, 60, 100);
    assert_eq!(synth.sounding(), 2);
    let first = render(&mut synth, 1000).left;
    let reference = bank("<region> sample=hat.wav\n<region> sample=snare.wav\n");
    let mut hat_only = reference.bank.synth(RATE);
    hat_only.prepare_all();
    on(&mut hat_only, 60, 100);
    assert_eq!(first, render(&mut hat_only, 1000).left);
}

#[test]
fn controller_ranges_use_set_cc_and_the_stream_but_never_rpn_controllers() {
    let test = bank(
        "<control> set_cc100=0 set_cc64=0\n<region> locc100=0 hicc100=63 sample=kick.wav\n<region> locc100=64 hicc100=127 sample=hat.wav\n<region> locc1=10 sample=snare.wav\n",
    );
    let mut synth = test.bank.synth(RATE);
    synth.prepare_all();
    on(&mut synth, 60, 100);
    assert_eq!(synth.sounding(), 1);
    synth.handle(
        channel(),
        &MidiMessage::Rpn {
            parameter: 0,
            value: 12,
        },
    );
    for controller in [6u8, 38, 98, 99, 100, 101] {
        cc(&mut synth, controller, 127);
    }
    synth.handle(
        channel(),
        &MidiMessage::Control {
            controller: 120,
            value: 0,
        },
    );
    on(&mut synth, 60, 100);
    assert_eq!(synth.sounding(), 1);
    synth.handle(
        channel(),
        &MidiMessage::Control {
            controller: 120,
            value: 0,
        },
    );
    cc(&mut synth, 1, 20);
    on(&mut synth, 60, 100);
    assert_eq!(synth.sounding(), 2);
    let control = text("<control> set_cc7=90\n<region> sample=a.wav set_cc8=1");
    assert_eq!(
        control.set_cc,
        vec![InitialController {
            controller: 7,
            value: 90
        }]
    );
    assert!(listed(&control, "set_ccN", IgnoreReason::Unsupported));
}

#[test]
fn rpn_and_nrpn_controllers_never_move_the_controller_state() {
    for controller in [6u8, 38, 98, 99, 100, 101] {
        let test = bank(&format!(
            "<control> set_cc{controller}=0\n<region> hicc{controller}=63 sample=kick.wav\n<region> locc{controller}=64 sample=hat.wav\n"
        ));
        let mut reference = test.bank.synth(RATE);
        reference.prepare_all();
        on(&mut reference, 60, 100);
        let expected = render(&mut reference, 2400).left;
        let mut synth = test.bank.synth(RATE);
        synth.prepare_all();
        synth.handle(
            channel(),
            &MidiMessage::Rpn {
                parameter: 0,
                value: 12,
            },
        );
        cc(&mut synth, controller, 127);
        on(&mut synth, 60, 100);
        assert_eq!(synth.sounding(), 1, "controller {controller}");
        assert_eq!(
            render(&mut synth, 2400).left,
            expected,
            "controller {controller}"
        );
    }
}

#[test]
fn the_controller_state_starts_from_set_cc() {
    let test = bank("<control> set_cc30=100\n<region> locc30=64 sample=kick.wav\n");
    let mut synth = test.bank.synth(RATE);
    synth.prepare_all();
    on(&mut synth, 60, 100);
    assert_eq!(synth.sounding(), 1);
    synth.handle(
        channel(),
        &MidiMessage::Control {
            controller: 120,
            value: 0,
        },
    );
    cc(&mut synth, 30, 10);
    on(&mut synth, 60, 100);
    assert_eq!(synth.sounding(), 0);
}

#[test]
fn headers_inherit_from_the_most_specific_scope() {
    let parsed = text(
        "<global> volume=-12 tune=5\n<master> volume=-6\n<group> volume=-3 pan=20\n<region> volume=2 sample=a.wav\n<region> sample=b.wav\n<group>\n<region> sample=c.wav\n<global>\n<region> sample=d.wav",
    );
    let volumes: Vec<f32> = parsed.regions.iter().map(|region| region.volume).collect();
    assert_eq!(volumes, vec![2.0, -3.0, -6.0, 0.0]);
    assert_eq!(parsed.regions[1].tune, 5);
    assert_eq!(parsed.regions[0].pan, 20.0);
    assert_eq!(parsed.regions[2].pan, 0.0);
}

#[test]
fn values_keep_spaces_and_lines_may_carry_several_headers() {
    let parsed = text(
        "<group> lokey=36 <region> sample=Kick Drum 1.wav hikey=40 volume=-2\r\n<region>Sample=A#5_s6.flac key=c#3\r",
    );
    assert_eq!(parsed.regions.len(), 2);
    assert_eq!(parsed.regions[0].sample, "Kick Drum 1.wav");
    assert_eq!((parsed.regions[0].lokey, parsed.regions[0].hikey), (36, 40));
    assert_eq!(parsed.regions[1].sample, "A#5_s6.flac");
    assert_eq!(parsed.regions[1].lokey, 49);
}

#[test]
fn comments_are_removed_before_anything_else() {
    let parsed = text(
        "// a comment <region> sample=no.wav\n<region> /* volume=-60\n still a comment */ sample=yes.wav // trailing",
    );
    assert_eq!(parsed.regions.len(), 1);
    assert_eq!(parsed.regions[0].sample, "yes.wav");
    assert_eq!(parsed.regions[0].volume, 0.0);
}

#[test]
fn defines_substitute_by_longest_name_inside_names_values_and_paths() {
    let parsed = text(
        "#define $KEY 40\n#define $KEY_KICK 36\n#define $NUM 1\n<control> default_path=kit$NUM/\n<region> key=$KEY_KICK sample=kick$NUM.wav amp_velcurve_$KEY=0.5\n<region> key=$KEY sample=$MISSING.wav\n<region> key=$KEY sample=b.wav tune=$NOPE",
    );
    assert_eq!(parsed.regions.len(), 2);
    assert_eq!(parsed.regions[0].lokey, 36);
    assert_eq!(parsed.regions[0].sample, "kit1/kick1.wav");
    assert_eq!(parsed.regions[0].amp_velcurve[0].velocity, 40);
    assert_eq!(parsed.regions[1].tune, 0);
    assert!(listed(&parsed, "sample", IgnoreReason::UnresolvedVariable));
    assert!(listed(&parsed, "tune", IgnoreReason::UnresolvedVariable));
}

#[test]
fn includes_expand_in_place_with_two_per_line_and_text_files() {
    let mut files = BTreeMap::new();
    files.insert("a.sfz".to_owned(), "<region> sample=a.wav".to_owned());
    files.insert(
        "parts\\b.txt".replace('\\', "/"),
        "<region> sample=b.wav\n#define $V -6".to_owned(),
    );
    let parsed = parse_sfz_text(
        "<global> pan=10\n#include \"a.sfz\" #include \"parts\\b.txt\"\n<region> sample=c.wav volume=$V\n#include \"none.sfz\"",
        &files,
        &SfzLimits::DEFAULT,
    )
    .expect("parses");
    let samples: Vec<&str> = parsed
        .regions
        .iter()
        .map(|region| region.sample.as_str())
        .collect();
    assert_eq!(samples, vec!["a.wav", "b.wav", "c.wav"]);
    assert_eq!(parsed.regions[2].volume, -6.0);
    assert!(parsed.regions.iter().all(|region| region.pan == 10.0));
    assert!(listed(&parsed, "#include", IgnoreReason::MissingInclude));
}

#[test]
fn values_outside_their_range_are_clamped_and_bad_values_ignored() {
    let parsed =
        text("<region> sample=a.wav tune=-20000 volume=loud ampeg_release=400 pan=-300 lokey=h4");
    let region = &parsed.regions[0];
    assert_eq!(region.tune, -9600);
    assert_eq!(region.volume, 0.0);
    assert_eq!(region.ampeg.release, 100.0);
    assert_eq!(region.pan, -100.0);
    assert_eq!(region.lokey, 0);
    assert!(listed(&parsed, "tune", IgnoreReason::Clamped));
    assert!(listed(&parsed, "volume", IgnoreReason::InvalidValue));
    assert!(listed(&parsed, "lokey", IgnoreReason::InvalidValue));
}

#[test]
fn unknown_headers_swallow_their_opcodes_and_generators_drop_regions() {
    let parsed = text(
        "<curve> v000=0 v127=1\n<effect> type=reverb\n<region> sample=*sine\n<region> sample=a.wav lfo02_freq=3 amplitude_oncc105=50\nstray=1",
    );
    assert_eq!(parsed.regions.len(), 1);
    assert!(listed(&parsed, "sample", IgnoreReason::Generator));
    assert!(listed(&parsed, "lfoN_freq", IgnoreReason::Unsupported));
    assert!(listed(
        &parsed,
        "amplitude_onccN",
        IgnoreReason::Unsupported
    ));
    assert!(
        !parsed
            .ignored
            .iter()
            .any(|entry| entry.name.starts_with('v'))
    );
    let early = text("volume=3\n<region> sample=a.wav");
    assert!(listed(&early, "volume", IgnoreReason::OutsideHeader));
}

#[test]
fn note_names_follow_c4_equals_sixty() {
    let cases = [
        ("c-1", 0u8),
        ("c4", 60),
        ("a4", 69),
        ("g9", 127),
        ("C#3", 49),
        ("db3", 49),
        ("b#3", 60),
        ("cb4", 59),
    ];
    for (name, expected) in cases {
        assert_eq!(
            only(&format!("<region> sample=a.wav key={name}")).lokey,
            expected,
            "{name}"
        );
    }
    for bad in ["+c4", "c10", "h2"] {
        let parsed = text(&format!("<region> sample=a.wav key={bad}"));
        assert!(listed(&parsed, "key", IgnoreReason::InvalidValue), "{bad}");
    }
}

#[test]
fn a_region_without_a_sample_is_dropped_and_listed() {
    let parsed = text("<region> key=60\n<region> sample=\n<region> sample=a.wav");
    assert_eq!(parsed.regions.len(), 1);
    assert!(listed(&parsed, "sample", IgnoreReason::MissingSample));
}

#[test]
fn the_ogg_and_flac_samples_play_through_regions() {
    let dir = scratch_dir();
    let root = write_bank(
        dir.path(),
        &[text_file(
            "t.sfz",
            "<region> lokey=0 hikey=59 sample=s.ogg loop_mode=loop_continuous\n<region> lokey=60 sample=p.flac\n",
        )],
        &[
            sample_file("s.ogg", opentab_testkit::banks::sine_ogg()),
            sample_file("p.flac", opentab_testkit::banks::flac16(&samples::pluck())),
        ],
    );
    let bank = SfzBank::load(&root).expect("loads");
    for key in [40u8, 70] {
        let mut synth = bank.synth(RATE);
        synth.prepare_all();
        on(&mut synth, key, 120);
        assert!(energy(&render(&mut synth, 4800).left) > 1.0, "key {key}");
    }
}

#[test]
fn voice_stealing_keeps_the_polyphony_cap() {
    let test = bank(&looped_sine(""));
    let mut synth = test.bank.synth(RATE);
    synth.prepare_all();
    for index in 0..200u32 {
        on(&mut synth, 40 + (index % 60) as u8, 100);
    }
    assert_eq!(synth.sounding(), opentab_audio::SFZ_MAX_VOICES);
}

#[test]
fn waveform_helpers_stay_in_sync() {
    let wave: Waveform = samples::sine_loop();
    assert_eq!((wave.loop_start, wave.loop_end), (200, 600));
}
