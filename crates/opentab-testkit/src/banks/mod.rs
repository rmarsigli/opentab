mod samples;
mod sf2;

pub use samples::SAMPLE_RATE;
pub use sf2::{
    Generator, InstrumentDef, PresetDef, SampleDef, SoundFontDef, Zone, generator, write_sf2,
};

use samples::Waveform;

pub const KEY_RANGE: u16 = 43;
pub const INSTRUMENT: u16 = 41;
pub const SAMPLE_ID: u16 = 53;
pub const SAMPLE_MODES: u16 = 54;
pub const SCALE_TUNING: u16 = 56;
pub const DECAY_VOL_ENV: u16 = 36;
pub const SUSTAIN_VOL_ENV: u16 = 37;
pub const RELEASE_VOL_ENV: u16 = 38;
pub const EXCLUSIVE_CLASS: u16 = 57;

pub const PERCUSSION_BANK: u16 = 128;
pub const VARIATION_BANK: u16 = 8;
pub const PAD_PROGRAM: u16 = 89;

const LOOP_CONTINUOUSLY: i16 = 1;
const MELODIC_RELEASE_TIMECENTS: i16 = -2786;
const BASS_RELEASE_TIMECENTS: i16 = -3600;
const BASS_DECAY_TIMECENTS: i16 = 1200;
const BASS_SUSTAIN_CENTIBELS: i16 = 200;
const DRUM_RELEASE_TIMECENTS: i16 = 0;
const HAT_CLASS: i16 = 1;

const SINE: u16 = 0;
const PLUCK: u16 = 1;
const SAW: u16 = 2;
const KICK: u16 = 3;
const SNARE: u16 = 4;
const HAT: u16 = 5;
const PAD: u16 = 6;

const SINE_INSTRUMENT: u16 = 0;
const PLUCK_INSTRUMENT: u16 = 1;
const BASS_INSTRUMENT: u16 = 2;
const KIT_INSTRUMENT: u16 = 3;
const PAD_INSTRUMENT: u16 = 4;

pub fn tiny_sf2() -> Vec<u8> {
    write_sf2(&tiny_definition(None))
}

pub fn padded_sf2(pad_frames: u32) -> Vec<u8> {
    write_sf2(&tiny_definition(Some(pad_frames)))
}

pub fn tiny_definition(pad_frames: Option<u32>) -> SoundFontDef {
    let mut samples = vec![
        sample("Sine A4", samples::sine_loop(), 69, -4),
        sample("Pluck A3", samples::pluck(), 57, -4),
        sample("Saw E2", samples::saw_bass(), 40, 0),
        sample("Kick", samples::kick(), 36, 0),
        sample("Snare", samples::snare(), 38, 0),
        sample("Hat", samples::hat(), 42, 0),
    ];
    let mut instruments = vec![
        InstrumentDef {
            name: "Sine".to_owned(),
            zones: vec![looped(SINE, MELODIC_RELEASE_TIMECENTS)],
        },
        InstrumentDef {
            name: "Pluck".to_owned(),
            zones: vec![Zone {
                generators: vec![
                    generator(RELEASE_VOL_ENV, MELODIC_RELEASE_TIMECENTS),
                    generator(SAMPLE_ID, PLUCK as i16),
                ],
            }],
        },
        InstrumentDef {
            name: "Saw Bass".to_owned(),
            zones: vec![Zone {
                generators: vec![
                    generator(DECAY_VOL_ENV, BASS_DECAY_TIMECENTS),
                    generator(SUSTAIN_VOL_ENV, BASS_SUSTAIN_CENTIBELS),
                    generator(RELEASE_VOL_ENV, BASS_RELEASE_TIMECENTS),
                    generator(SAMPLE_MODES, LOOP_CONTINUOUSLY),
                    generator(SAMPLE_ID, SAW as i16),
                ],
            }],
        },
        InstrumentDef {
            name: "Kit".to_owned(),
            zones: vec![
                drum_zone(35, 36, KICK, None),
                drum_zone(37, 41, SNARE, None),
                drum_zone(42, 81, HAT, Some(HAT_CLASS)),
            ],
        },
    ];
    let mut presets = vec![
        preset("Sine Lead", 0, 0, SINE_INSTRUMENT),
        preset("Synth Nylon", 0, 24, PLUCK_INSTRUMENT),
        preset("Synth Steel", 0, 25, PLUCK_INSTRUMENT),
        preset("Synth Clean", 0, 27, PLUCK_INSTRUMENT),
        preset("Synth Overdrive", 0, 29, PLUCK_INSTRUMENT),
        preset("Synth Distortion", 0, 30, PLUCK_INSTRUMENT),
        preset("Synth Finger Bass", 0, 33, BASS_INSTRUMENT),
        preset("Synth Pick Bass", 0, 34, BASS_INSTRUMENT),
        preset("Sine Nylon", VARIATION_BANK, 24, SINE_INSTRUMENT),
        preset("Standard Kit", PERCUSSION_BANK, 0, KIT_INSTRUMENT),
    ];
    if let Some(frames) = pad_frames {
        samples.push(sample("Pad A4", samples::long_pad(frames), 69, -4));
        instruments.push(InstrumentDef {
            name: "Pad".to_owned(),
            zones: vec![looped(PAD, MELODIC_RELEASE_TIMECENTS)],
        });
        presets.push(preset("Long Pad", 0, PAD_PROGRAM, PAD_INSTRUMENT));
    }
    SoundFontDef {
        name: "OpenTab Tiny Test Bank".to_owned(),
        samples,
        instruments,
        presets,
    }
}

fn sample(name: &str, waveform: Waveform, root_key: u8, correction: i8) -> SampleDef {
    SampleDef {
        name: name.to_owned(),
        points: waveform.points,
        loop_start: waveform.loop_start,
        loop_end: waveform.loop_end,
        rate: SAMPLE_RATE,
        root_key,
        correction,
    }
}

fn looped(sample_id: u16, release: i16) -> Zone {
    Zone {
        generators: vec![
            generator(RELEASE_VOL_ENV, release),
            generator(SAMPLE_MODES, LOOP_CONTINUOUSLY),
            generator(SAMPLE_ID, sample_id as i16),
        ],
    }
}

fn drum_zone(low: u8, high: u8, sample_id: u16, class: Option<i16>) -> Zone {
    let mut generators = vec![
        Generator {
            operator: KEY_RANGE,
            amount: u16::from_le_bytes([low, high]),
        },
        generator(SCALE_TUNING, 0),
        generator(RELEASE_VOL_ENV, DRUM_RELEASE_TIMECENTS),
    ];
    if let Some(class) = class {
        generators.push(generator(EXCLUSIVE_CLASS, class));
    }
    generators.push(generator(SAMPLE_ID, sample_id as i16));
    Zone { generators }
}

fn preset(name: &str, bank: u16, program: u16, instrument: u16) -> PresetDef {
    PresetDef {
        name: name.to_owned(),
        bank,
        program,
        zones: vec![Zone {
            generators: vec![generator(INSTRUMENT, instrument as i16)],
        }],
    }
}
