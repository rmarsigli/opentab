pub(crate) const ENGINE_GAIN_Q16: i64 = 242_000;
pub(crate) const VELOCITY_CURVE_OFFSET: i64 = 127;
pub(crate) const STRING_MAKEUP_Q16: i64 = 131_072;
pub(crate) const EXCITATION_PEAK_Q23: i64 = 1 << 22;
pub(crate) const STRING_RELEASE_MS: u32 = 60;
pub(crate) const CHOKE_RELEASE_MS: u32 = 30;
pub(crate) const DRUM_LONGEST_MS: u32 = 2000;
pub(crate) const KIT_LEVEL_Q16: i64 = 30_000;
pub(crate) const TONE_LEVEL_Q16: i64 = 22_000;
pub(crate) const HIGH_NOTE_REFERENCE_HZ: u32 = 330;
pub(crate) const HIGH_NOTE_BOOST_Q16: i64 = 10_000;
pub(crate) const HIGH_NOTE_MIN_BLEND_PERCENT: i64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StringFamily {
    pub(crate) damping_q16: i64,
    pub(crate) blend_q16: i64,
    pub(crate) pluck_q16: i64,
    pub(crate) noise_q16: i64,
    pub(crate) position_divisor: u32,
    pub(crate) drive_q16: i64,
    pub(crate) level_q16: i64,
}

const NYLON: StringFamily = StringFamily {
    damping_q16: 64_620,
    blend_q16: 32_768,
    pluck_q16: 22_000,
    noise_q16: 8_000,
    position_divisor: 4,
    drive_q16: 0,
    level_q16: 65_536,
};
const STEEL: StringFamily = StringFamily {
    damping_q16: 64_880,
    blend_q16: 22_000,
    pluck_q16: 46_000,
    noise_q16: 16_000,
    position_divisor: 6,
    level_q16: 58_000,
    ..NYLON
};
const JAZZ: StringFamily = StringFamily {
    damping_q16: 64_772,
    pluck_q16: 26_000,
    level_q16: 62_000,
    ..NYLON
};
const CLEAN: StringFamily = StringFamily {
    damping_q16: 64_962,
    blend_q16: 24_000,
    pluck_q16: 38_000,
    noise_q16: 12_000,
    position_divisor: 6,
    level_q16: 60_000,
    ..NYLON
};
const MUTED: StringFamily = StringFamily {
    damping_q16: 60_017,
    pluck_q16: 26_000,
    level_q16: 70_000,
    ..NYLON
};
const OVERDRIVE: StringFamily = StringFamily {
    damping_q16: 65_152,
    blend_q16: 20_000,
    pluck_q16: 40_000,
    noise_q16: 12_000,
    position_divisor: 6,
    drive_q16: 4 << 16,
    level_q16: 14_000,
};
const DISTORTION: StringFamily = StringFamily {
    damping_q16: 65_208,
    blend_q16: 18_000,
    pluck_q16: 44_000,
    drive_q16: 9 << 16,
    level_q16: 11_000,
    ..OVERDRIVE
};
const HARMONICS: StringFamily = StringFamily {
    damping_q16: 64_772,
    blend_q16: 10_000,
    pluck_q16: 60_000,
    noise_q16: 20_000,
    position_divisor: 12,
    level_q16: 50_000,
    ..NYLON
};
const BASS: StringFamily = StringFamily {
    damping_q16: 63_911,
    pluck_q16: 20_000,
    noise_q16: 6_000,
    level_q16: 80_000,
    ..NYLON
};
const SLAP: StringFamily = StringFamily {
    blend_q16: 20_000,
    pluck_q16: 44_000,
    noise_q16: 16_000,
    position_divisor: 8,
    level_q16: 70_000,
    ..BASS
};
const SYNTH_BASS: StringFamily = StringFamily {
    damping_q16: 64_400,
    blend_q16: 14_000,
    pluck_q16: 50_000,
    noise_q16: 4_000,
    position_divisor: 3,
    drive_q16: 2 << 16,
    level_q16: 30_000,
};
const SITAR: StringFamily = StringFamily {
    damping_q16: 64_880,
    blend_q16: 12_000,
    pluck_q16: 52_000,
    noise_q16: 20_000,
    position_divisor: 8,
    level_q16: 52_000,
    ..NYLON
};
const BANJO: StringFamily = StringFamily {
    damping_q16: 64_225,
    blend_q16: 16_000,
    pluck_q16: 56_000,
    level_q16: 56_000,
    ..SITAR
};
const KOTO: StringFamily = StringFamily {
    damping_q16: 64_620,
    blend_q16: 20_000,
    pluck_q16: 40_000,
    noise_q16: 12_000,
    position_divisor: 6,
    level_q16: 60_000,
    ..NYLON
};

pub(crate) fn string_family(program: u8) -> Option<StringFamily> {
    let family = match program {
        24 => NYLON,
        25 => STEEL,
        26 => JAZZ,
        27 => CLEAN,
        28 => MUTED,
        29 => OVERDRIVE,
        30 => DISTORTION,
        31 => HARMONICS,
        32..=35 => BASS,
        36 | 37 => SLAP,
        38 | 39 => SYNTH_BASS,
        104 => SITAR,
        105 | 106 => BANJO,
        107 => KOTO,
        _ => return None,
    };
    Some(family)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ToneFamily {
    pub(crate) ratio_numerator: u32,
    pub(crate) ratio_denominator: u32,
    pub(crate) index_low_q16: i64,
    pub(crate) index_high_q16: i64,
    pub(crate) attack_ms: u32,
    pub(crate) decay_ms: u32,
    pub(crate) sustain_percent: i64,
    pub(crate) release_ms: u32,
    pub(crate) level_q16: i64,
}

const PIANO: ToneFamily = ToneFamily {
    ratio_numerator: 1,
    ratio_denominator: 1,
    index_low_q16: 20_000,
    index_high_q16: 131_072,
    attack_ms: 3,
    decay_ms: 350,
    sustain_percent: 0,
    release_ms: 150,
    level_q16: TONE_LEVEL_Q16,
};
const CHROMATIC: ToneFamily = ToneFamily {
    ratio_numerator: 7,
    ratio_denominator: 2,
    index_low_q16: 32_768,
    index_high_q16: 196_608,
    attack_ms: 1,
    decay_ms: 250,
    ..PIANO
};
const ORGAN: ToneFamily = ToneFamily {
    index_low_q16: 65_536,
    index_high_q16: 65_536,
    attack_ms: 8,
    decay_ms: 100,
    sustain_percent: 90,
    release_ms: 60,
    ..PIANO
};
const PLUCKED_FALLBACK: ToneFamily = ToneFamily {
    decay_ms: 300,
    release_ms: 120,
    ..PIANO
};
const STRINGS: ToneFamily = ToneFamily {
    index_low_q16: 65_536,
    index_high_q16: 104_858,
    attack_ms: 90,
    decay_ms: 400,
    sustain_percent: 80,
    release_ms: 200,
    ..PIANO
};
const ENSEMBLE: ToneFamily = ToneFamily {
    attack_ms: 120,
    ..STRINGS
};
const BRASS: ToneFamily = ToneFamily {
    index_low_q16: 52_429,
    index_high_q16: 196_608,
    attack_ms: 35,
    decay_ms: 200,
    sustain_percent: 75,
    release_ms: 100,
    ..PIANO
};
const REED: ToneFamily = ToneFamily {
    ratio_numerator: 2,
    index_low_q16: 78_643,
    index_high_q16: 131_072,
    attack_ms: 30,
    decay_ms: 200,
    sustain_percent: 80,
    release_ms: 90,
    ..PIANO
};
const PIPE: ToneFamily = ToneFamily {
    index_low_q16: 13_107,
    index_high_q16: 39_322,
    attack_ms: 40,
    decay_ms: 200,
    sustain_percent: 85,
    release_ms: 90,
    ..PIANO
};
const LEAD: ToneFamily = ToneFamily {
    index_low_q16: 163_840,
    index_high_q16: 262_144,
    attack_ms: 4,
    decay_ms: 300,
    sustain_percent: 85,
    release_ms: 80,
    ..PIANO
};
const PAD: ToneFamily = ToneFamily {
    ratio_denominator: 2,
    index_low_q16: 65_536,
    index_high_q16: 98_304,
    attack_ms: 250,
    decay_ms: 500,
    sustain_percent: 90,
    release_ms: 200,
    ..PIANO
};
const SYNTH_EFFECTS: ToneFamily = ToneFamily {
    ratio_numerator: 3,
    ratio_denominator: 2,
    index_low_q16: 65_536,
    index_high_q16: 163_840,
    attack_ms: 50,
    decay_ms: 300,
    sustain_percent: 70,
    release_ms: 200,
    ..PIANO
};
const ETHNIC: ToneFamily = ToneFamily {
    index_low_q16: 65_536,
    index_high_q16: 163_840,
    attack_ms: 20,
    decay_ms: 300,
    sustain_percent: 70,
    release_ms: 100,
    ..PIANO
};
const PERCUSSIVE: ToneFamily = ToneFamily {
    ratio_numerator: 7,
    ratio_denominator: 5,
    index_low_q16: 65_536,
    index_high_q16: 262_144,
    attack_ms: 1,
    decay_ms: 120,
    release_ms: 100,
    ..PIANO
};
const SOUND_EFFECTS: ToneFamily = ToneFamily {
    ratio_numerator: 11,
    ratio_denominator: 3,
    index_low_q16: 131_072,
    index_high_q16: 393_216,
    attack_ms: 10,
    decay_ms: 300,
    sustain_percent: 50,
    ..PIANO
};

const TONE_FAMILIES: [ToneFamily; 16] = [
    PIANO,
    CHROMATIC,
    ORGAN,
    PLUCKED_FALLBACK,
    PLUCKED_FALLBACK,
    STRINGS,
    ENSEMBLE,
    BRASS,
    REED,
    PIPE,
    LEAD,
    PAD,
    SYNTH_EFFECTS,
    ETHNIC,
    PERCUSSIVE,
    SOUND_EFFECTS,
];
const PROGRAMS_PER_FAMILY: u8 = 8;

pub(crate) fn tone_family(program: u8) -> ToneFamily {
    TONE_FAMILIES
        .get(usize::from(program / PROGRAMS_PER_FAMILY))
        .copied()
        .unwrap_or(PIANO)
}

const fn longest_release() -> u32 {
    let mut longest = STRING_RELEASE_MS;
    let mut index = 0;
    while index < TONE_FAMILIES.len() {
        if TONE_FAMILIES[index].release_ms > longest {
            longest = TONE_FAMILIES[index].release_ms;
        }
        index += 1;
    }
    longest
}

pub(crate) const LONGEST_MELODIC_RELEASE_MS: u32 = longest_release();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ToneRecipe {
    pub(crate) start_hz: u32,
    pub(crate) end_hz: u32,
    pub(crate) sweep_ms: u32,
    pub(crate) decay_ms: u32,
    pub(crate) level_q16: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NoiseRecipe {
    pub(crate) top_q16: i64,
    pub(crate) bottom_q16: i64,
    pub(crate) decay_ms: u32,
    pub(crate) level_q16: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DrumRecipe {
    pub(crate) tone: Option<ToneRecipe>,
    pub(crate) noise: Option<NoiseRecipe>,
}

pub(crate) const fn tone(
    start_hz: u32,
    end_hz: u32,
    sweep_ms: u32,
    decay_ms: u32,
    level_q16: i64,
) -> ToneRecipe {
    ToneRecipe {
        start_hz,
        end_hz,
        sweep_ms,
        decay_ms,
        level_q16,
    }
}

const fn noise(top_q16: i64, bottom_q16: i64, decay_ms: u32, level_q16: i64) -> NoiseRecipe {
    NoiseRecipe {
        top_q16,
        bottom_q16,
        decay_ms,
        level_q16,
    }
}

pub(crate) const UNMAPPED_SWEEP_MS: u32 = 10;
pub(crate) const UNMAPPED_DECAY_MS: u32 = 50;
pub(crate) const UNMAPPED_LEVEL_Q16: i64 = 30_000;

fn tom(end_hz: u32) -> DrumRecipe {
    DrumRecipe {
        tone: Some(tone(end_hz * 3 / 2, end_hz, 40, 140, 70_000)),
        noise: Some(noise(30_000, 2_000, 20, 8_000)),
    }
}

fn hand(hertz: u32, decay_ms: u32) -> DrumRecipe {
    DrumRecipe {
        tone: Some(tone(hertz * 5 / 4, hertz, 8, decay_ms, 50_000)),
        noise: Some(noise(40_000, 8_000, 8, 8_000)),
    }
}

pub(crate) fn drum_recipe(key: u8) -> DrumRecipe {
    match key {
        35 | 36 => DrumRecipe {
            tone: Some(tone(170, 50, 22, 110, 90_000)),
            noise: Some(noise(40_000, 6_000, 4, 12_000)),
        },
        37 => DrumRecipe {
            tone: Some(tone(900, 700, 10, 12, 30_000)),
            noise: Some(noise(50_000, 12_000, 10, 30_000)),
        },
        38 | 40 => DrumRecipe {
            tone: Some(tone(240, 185, 20, 40, 40_000)),
            noise: Some(noise(42_000, 9_000, 75, 52_000)),
        },
        39 => DrumRecipe {
            tone: None,
            noise: Some(noise(36_000, 10_000, 45, 56_000)),
        },
        41 => tom(82),
        43 => tom(98),
        45 => tom(110),
        47 => tom(131),
        48 => tom(147),
        50 => tom(165),
        42 | 44 => DrumRecipe {
            tone: None,
            noise: Some(noise(62_000, 40_000, 22, 30_000)),
        },
        46 => DrumRecipe {
            tone: None,
            noise: Some(noise(62_000, 38_000, 150, 26_000)),
        },
        49 | 52 | 55 | 57 => DrumRecipe {
            tone: None,
            noise: Some(noise(60_000, 26_000, 190, 30_000)),
        },
        51 | 53 | 59 => DrumRecipe {
            tone: Some(tone(3_200, 3_100, 50, 120, 6_000)),
            noise: Some(noise(58_000, 34_000, 120, 16_000)),
        },
        54 | 69 | 70 => DrumRecipe {
            tone: None,
            noise: Some(noise(60_000, 42_000, 35, 24_000)),
        },
        56 => DrumRecipe {
            tone: Some(tone(560, 540, 20, 60, 40_000)),
            noise: None,
        },
        60 => hand(400, 60),
        61 => hand(300, 70),
        62 => hand(330, 40),
        63 => hand(260, 80),
        64 => hand(200, 90),
        65 => hand(320, 90),
        66 => hand(220, 100),
        67 => hand(650, 50),
        68 => hand(470, 50),
        75 => hand(2_500, 20),
        76 => hand(1_300, 25),
        77 => hand(900, 25),
        80 => hand(4_400, 40),
        81 => hand(4_400, 180),
        _ => DrumRecipe {
            tone: None,
            noise: Some(noise(48_000, 16_000, 40, 30_000)),
        },
    }
}
