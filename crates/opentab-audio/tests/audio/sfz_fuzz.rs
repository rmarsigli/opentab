use std::collections::BTreeMap;

use opentab_audio::{BankError, SfzLimits, decode_audio, parse_sfz_text};
use opentab_testkit::banks::{
    ExtraCodebook, FlacSpec, VorbisSpec, WavSpec, flac16, mono_points, reseal_ogg, samples,
    sine_ogg, stereo_points, wav16, write_flac, write_vorbis, write_wav,
};
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};

const FRAGMENTS: &[&str] = &[
    "<region>",
    "<group>",
    "<global>",
    "<master>",
    "<control>",
    "<curve>",
    "<effect>",
    "<midi>",
    "<sample>",
    "<",
    ">",
    "<regi",
    " ",
    "  ",
    "\n",
    "\r\n",
    "\r",
    "\t",
    "=",
    "==",
    "sample=",
    "Sample=",
    "key=",
    "lokey=",
    "hikey=",
    "lovel=",
    "hivel=",
    "pitch_keycenter=",
    "tune=",
    "transpose=",
    "pitch_keytrack=",
    "volume=",
    "amplitude=",
    "pan=",
    "amp_veltrack=",
    "amp_velcurve_64=",
    "amp_velcurve_300=",
    "ampeg_attack=",
    "ampeg_hold=",
    "ampeg_decay=",
    "ampeg_sustain=",
    "ampeg_release=",
    "loop_mode=",
    "loop_start=",
    "loop_end=",
    "offset=",
    "end=",
    "group=",
    "off_by=",
    "off_mode=",
    "off_time=",
    "lorand=",
    "hirand=",
    "seq_length=",
    "seq_position=",
    "trigger=",
    "sw_last=",
    "sw_default=",
    "sw_lokey=",
    "locc100=",
    "hicc64=",
    "set_cc100=",
    "set_cc999=",
    "default_path=",
    "lfo01_freq=",
    "amplitude_oncc$CC=",
    "label_cc7=",
    "v000=",
    "0",
    "1",
    "-1",
    "127",
    "128",
    "4294967296",
    "-99999",
    "0.5",
    "1e308",
    "NaN",
    "inf",
    "c4",
    "c#-1",
    "g9",
    "b#9",
    "cb-1",
    "h4",
    "+c4",
    "no_loop",
    "one_shot",
    "loop_continuous",
    "loop_sustain",
    "fast",
    "normal",
    "time",
    "attack",
    "release",
    "first",
    "legato",
    "release_key",
    "pluck.wav",
    "Kick Drum 1.wav",
    "..\\Samples\\",
    "../",
    "/etc/x",
    "C:\\x",
    "*sine",
    "$",
    "$CC",
    "$CCC",
    "$UNDEFINED",
    "#define $CC 7",
    "#define $CCC 70",
    "#define $LONG ",
    "#define bad",
    "#include \"f.sfz\"",
    "#include \"g.txt\"",
    "#include",
    "#include \"missing.sfz\"",
    "#pragma",
    "//",
    "/*",
    "*/",
    "é",
    "\u{feff}",
    "\0",
];

fn fragment() -> impl Strategy<Value = String> {
    prop_oneof![
        8 => (0..FRAGMENTS.len()).prop_map(|index| FRAGMENTS[index].to_owned()),
        1 => "\\PC{0,12}",
        1 => vec(any::<u8>(), 0..12).prop_map(|bytes| String::from_utf8_lossy(&bytes).into_owned()),
    ]
}

fn sfz_text() -> impl Strategy<Value = String> {
    vec(fragment(), 0..160).prop_map(|parts| parts.concat())
}

#[derive(Debug, Clone)]
struct FuzzCase {
    root: String,
    first: String,
    second: String,
    tight: bool,
}

fn fuzz_case() -> impl Strategy<Value = FuzzCase> {
    (sfz_text(), sfz_text(), sfz_text(), any::<bool>()).prop_map(|(root, first, second, tight)| {
        FuzzCase {
            root,
            first,
            second,
            tight,
        }
    })
}

const TIGHT: SfzLimits = SfzLimits {
    file_bytes: 2048,
    expanded_bytes: 8192,
    regions: 12,
    include_depth: 2,
    includes: 6,
    defines: 3,
    define_bytes: 64,
    loader_bytes: 16 << 10,
    ..SfzLimits::DEFAULT
};

fn check_text(case: &FuzzCase) -> Result<(), TestCaseError> {
    let mut includes = BTreeMap::new();
    includes.insert("f.sfz".to_owned(), case.first.clone());
    includes.insert("g.txt".to_owned(), case.second.clone());
    let limits = if case.tight {
        TIGHT
    } else {
        SfzLimits::DEFAULT
    };
    match parse_sfz_text(&case.root, &includes, &limits) {
        Ok(parsed) => {
            prop_assert!(parsed.regions.len() as u64 <= limits.regions);
            prop_assert!(parsed.expanded_bytes as u64 <= limits.expanded_bytes);
            for region in &parsed.regions {
                prop_assert!(region.lokey <= 127 && region.hikey <= 127);
                prop_assert!(region.lovel <= 127 && region.hivel <= 127);
                prop_assert!(region.volume.is_finite() && region.volume <= 6.0);
                prop_assert!((0.0..=1.0).contains(&region.lorand));
                prop_assert!((1..=100).contains(&region.seq_length));
                prop_assert!(!region.sample.is_empty() && !region.sample.contains('$'));
                prop_assert!(region.velocity_gain(127).is_finite());
            }
        }
        Err(BankError::Unsafe { .. }) => {}
        Err(other) => prop_assert!(false, "unexpected error {other:?}"),
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn the_sfz_text_parser_never_panics_and_respects_its_limits(case in fuzz_case()) {
        check_text(&case)?;
    }
}

#[test]
#[ignore = "20 000 cases: run on demand"]
fn the_sfz_text_parser_survives_twenty_thousand_cases() {
    let mut runner = TestRunner::new(Config::with_cases(20_000));
    runner
        .run(&fuzz_case(), |case| check_text(&case))
        .expect("the parser survives");
}

fn seeds() -> Vec<Vec<u8>> {
    let pluck = samples::pluck();
    let mut short = samples::kick();
    short.points.truncate(3000);
    let mut stereo_ogg = VorbisSpec::tone(48_000, 6, 7);
    stereo_ogg.channels = 2;
    let mut booked_ogg = VorbisSpec::tone(44_100, 6, 2);
    booked_ogg.extra_codebook = Some(ExtraCodebook {
        entries: 2,
        dimensions: 2,
        lookup_type: 0,
        value_bits: 1,
    });
    vec![
        wav16(&short),
        write_wav(
            &WavSpec {
                rate: 48_000,
                channels: 2,
                bits: 24,
                extensible: true,
            },
            &stereo_points(&short, 2),
        ),
        flac16(&short),
        write_flac(
            &FlacSpec {
                rate: 44_100,
                channels: 2,
                bits: 24,
            },
            &stereo_points(&short, 3),
        ),
        write_wav(&WavSpec::mono16(22_050), &mono_points(&pluck)[..2000]),
        sine_ogg(),
        write_vorbis(&stereo_ogg),
        write_vorbis(&booked_ogg),
    ]
}

#[derive(Debug, Clone)]
enum Mutation {
    Header { at: usize, value: u8 },
    Flip { at: usize, value: u8 },
    Word { at: usize, value: u32 },
    Truncate { at: usize },
    Insert { at: usize, bytes: Vec<u8> },
}

fn mutation() -> impl Strategy<Value = Mutation> {
    prop_oneof![
        3 => (0..HEADER_SPAN, any::<u8>()).prop_map(|(at, value)| Mutation::Header { at, value }),
        4 => (any::<usize>(), any::<u8>()).prop_map(|(at, value)| Mutation::Flip { at, value }),
        3 => (any::<usize>(), prop_oneof![Just(0u32), Just(u32::MAX), Just(0x7FFF_FFFF), any::<u32>()])
            .prop_map(|(at, value)| Mutation::Word { at, value }),
        1 => any::<usize>().prop_map(|at| Mutation::Truncate { at }),
        1 => (any::<usize>(), vec(any::<u8>(), 1..16)).prop_map(|(at, bytes)| Mutation::Insert { at, bytes }),
    ]
}

fn apply(bytes: &mut Vec<u8>, mutation: &Mutation) {
    if bytes.is_empty() {
        return;
    }
    let length = bytes.len();
    match mutation {
        Mutation::Header { at, value } => {
            if let Some(slot) = bytes.get_mut(*at) {
                *slot ^= value;
            }
        }
        Mutation::Flip { at, value } => bytes[at % length] ^= value,
        Mutation::Word { at, value } => {
            let start = at % length;
            for (offset, byte) in value.to_le_bytes().iter().enumerate() {
                if let Some(slot) = bytes.get_mut(start + offset) {
                    *slot = *byte;
                }
            }
        }
        Mutation::Truncate { at } => bytes.truncate(at % length),
        Mutation::Insert { at, bytes: extra } => {
            let start = at % length;
            for (offset, byte) in extra.iter().enumerate() {
                bytes.insert(start + offset, *byte);
            }
        }
    }
}

#[derive(Debug, Clone)]
struct DecodeCase {
    seed: usize,
    mutations: Vec<Mutation>,
    tight: bool,
    reseal: bool,
}

const HEADER_SPAN: usize = 160;

fn decode_case() -> impl Strategy<Value = DecodeCase> {
    (
        0..8usize,
        vec(mutation(), 1..6),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(seed, mutations, tight, reseal)| DecodeCase {
            seed,
            mutations,
            tight,
            reseal,
        })
}

const DECODE_TIGHT: SfzLimits = SfzLimits {
    sample_frames: 20_000,
    decoded_bytes: 96 << 10,
    vorbis_table_values: 1 << 12,
    ..SfzLimits::DEFAULT
};

fn check_decode(seeds: &[Vec<u8>], case: &DecodeCase) -> Result<(), TestCaseError> {
    let mut bytes = seeds[case.seed % seeds.len()].clone();
    for mutation in &case.mutations {
        apply(&mut bytes, mutation);
    }
    if case.reseal {
        reseal_ogg(&mut bytes);
    }
    let limits = if case.tight {
        DECODE_TIGHT
    } else {
        SfzLimits::DEFAULT
    };
    if let Ok(decoded) = decode_audio(&bytes, &limits) {
        prop_assert!(decoded.channels >= 1 && decoded.channels <= limits.sample_channels);
        prop_assert!(decoded.frames as u64 <= limits.sample_frames);
        prop_assert!(decoded.bytes() <= limits.decoded_bytes);
        prop_assert_eq!(
            decoded.data.len(),
            decoded.frames * decoded.channels as usize
        );
        prop_assert!(decoded.data.iter().all(|value| value.is_finite()));
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn decoders_survive_mutated_samples_within_their_caps(case in decode_case()) {
        check_decode(&seeds(), &case)?;
    }
}

#[test]
#[ignore = "20 000 cases: run on demand"]
fn decoders_survive_twenty_thousand_mutated_samples() {
    let seeds = seeds();
    let mut runner = TestRunner::new(Config::with_cases(20_000));
    runner
        .run(&decode_case(), |case| check_decode(&seeds, &case))
        .expect("the decoders survive");
}
