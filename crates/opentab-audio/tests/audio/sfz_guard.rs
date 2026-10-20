use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use opentab_audio::{
    BankError, IgnoreReason, SampleRate, SfzBank, SfzLimits, Synth, UnsafeBank, measure_sfz_text,
    parse_sfz_text,
};
use opentab_playback::{Channel, MidiMessage};
use opentab_testkit::banks::{SIDECAR_FILE, WavSpec, samples, wav16, write_wav};

use crate::sfz::{energy, render, write_bank};
use crate::support::scratch_dir;

fn refusal<T: std::fmt::Debug>(result: Result<T, BankError>) -> UnsafeBank {
    match result {
        Err(BankError::Unsafe { reason }) => reason,
        other => panic!("expected an unsafe refusal, got {other:?}"),
    }
}

fn text_with(
    sfz: &str,
    files: &[(&str, &str)],
    limits: &SfzLimits,
) -> Result<opentab_audio::SfzText, BankError> {
    let includes: BTreeMap<String, String> = files
        .iter()
        .map(|(name, text)| ((*name).to_owned(), (*text).to_owned()))
        .collect();
    parse_sfz_text(sfz, &includes, limits)
}

fn load(root: &Path, limits: &SfzLimits) -> Result<SfzBank, BankError> {
    SfzBank::load_with_limits(root, limits)
}

fn pluck_bank(dir: &Path, sfz: &[(&str, &str)]) -> std::path::PathBuf {
    write_bank(dir, sfz, &[("pluck.wav", wav16(&samples::pluck()))])
}

#[test]
fn an_instrument_file_beyond_the_size_cap_is_refused() {
    let limits = SfzLimits {
        file_bytes: 64,
        ..SfzLimits::DEFAULT
    };
    let long = format!("<region> sample=pluck.wav\n// {}", "x".repeat(100));
    assert_eq!(
        refusal(text_with(&long, &[], &limits)),
        UnsafeBank::SfzFileTooLarge { limit_kib: 0 }
    );
    assert_eq!(
        refusal(text_with(
            "#include \"big.sfz\"",
            &[("big.sfz", &long)],
            &limits
        )),
        UnsafeBank::SfzFileTooLarge { limit_kib: 0 }
    );
    let dir = scratch_dir();
    let root = pluck_bank(dir.path(), &[("a.sfz", &long)]);
    assert_eq!(
        refusal(load(&root, &limits)),
        UnsafeBank::SfzFileTooLarge { limit_kib: 0 }
    );
}

#[test]
fn expansion_beyond_the_text_cap_is_refused() {
    let limits = SfzLimits {
        expanded_bytes: 4096,
        ..SfzLimits::DEFAULT
    };
    let fragment = "<region> sample=pluck.wav\n".repeat(20);
    let includes = "#include \"f.sfz\"\n".repeat(20);
    assert_eq!(
        refusal(text_with(&includes, &[("f.sfz", &fragment)], &limits)),
        UnsafeBank::ExpandedTextTooLarge { limit_mib: 0 }
    );
    let bomb = format!(
        "#define $A {}\n<region> sample=pluck.wav volume={}",
        "9".repeat(200),
        "$A".repeat(100)
    );
    assert_eq!(
        refusal(text_with(&bomb, &[], &limits)),
        UnsafeBank::ExpandedTextTooLarge { limit_mib: 0 }
    );
}

#[test]
fn the_whole_bank_text_is_capped() {
    let limits = SfzLimits {
        bank_text_bytes: 100,
        ..SfzLimits::DEFAULT
    };
    let dir = scratch_dir();
    let region = "<region> sample=pluck.wav\n";
    let root = pluck_bank(
        dir.path(),
        &[("a.sfz", region), ("b.sfz", region), ("c.sfz", region)],
    );
    assert_eq!(
        refusal(load(&root, &limits)),
        UnsafeBank::BankTextTooLarge { limit_mib: 0 }
    );
    assert!(load(&root, &SfzLimits::DEFAULT).is_ok());
}

#[test]
fn too_many_regions_are_refused() {
    let limits = SfzLimits {
        regions: 2,
        ..SfzLimits::DEFAULT
    };
    let three = "<region> sample=a.wav\n".repeat(3);
    assert_eq!(
        refusal(text_with(&three, &[], &limits)),
        UnsafeBank::TooManyRegions { limit: 2 }
    );
    assert!(text_with(&"<region> sample=a.wav\n".repeat(2), &[], &limits).is_ok());
    let dir = scratch_dir();
    let root = pluck_bank(dir.path(), &[("a.sfz", &three)]);
    assert_eq!(
        refusal(load(&root, &limits)),
        UnsafeBank::TooManyRegions { limit: 2 }
    );
}

#[test]
fn includes_nested_too_deep_are_refused() {
    let limits = SfzLimits {
        include_depth: 2,
        ..SfzLimits::DEFAULT
    };
    let files = [
        ("1.sfz", "#include \"2.sfz\""),
        ("2.sfz", "#include \"3.sfz\""),
        ("3.sfz", "<region> sample=a.wav"),
    ];
    assert_eq!(
        refusal(text_with("#include \"1.sfz\"", &files, &limits)),
        UnsafeBank::IncludeTooDeep { limit: 2 }
    );
    assert!(text_with("#include \"2.sfz\"", &files, &limits).is_ok());
}

#[test]
fn too_many_includes_are_refused() {
    let limits = SfzLimits {
        includes: 4,
        ..SfzLimits::DEFAULT
    };
    let files = [("f.sfz", "<region> sample=a.wav")];
    assert_eq!(
        refusal(text_with(
            &"#include \"f.sfz\"\n".repeat(5),
            &files,
            &limits
        )),
        UnsafeBank::TooManyIncludes { limit: 4 }
    );
    assert!(text_with(&"#include \"f.sfz\"\n".repeat(4), &files, &limits).is_ok());
}

#[test]
fn include_cycles_are_refused_in_memory_and_on_disk() {
    let files = [
        ("a.sfz", "#include \"b.sfz\""),
        ("b.sfz", "#include \"a.sfz\""),
    ];
    assert_eq!(
        refusal(text_with("#include \"a.sfz\"", &files, &SfzLimits::DEFAULT)),
        UnsafeBank::IncludeCycle
    );
    let dir = scratch_dir();
    let root = pluck_bank(
        dir.path(),
        &[
            (
                "main.sfz",
                "#include \"parts/x.sfz\"\n<region> sample=pluck.wav",
            ),
            ("parts/x.sfz", "#include \"main.sfz\""),
        ],
    );
    assert_eq!(
        refusal(load(&root, &SfzLimits::DEFAULT)),
        UnsafeBank::IncludeCycle
    );
}

#[test]
fn too_many_defines_are_refused() {
    let limits = SfzLimits {
        defines: 2,
        ..SfzLimits::DEFAULT
    };
    let text = "#define $A 1\n#define $B 2\n#define $C 3\n<region> sample=a.wav";
    assert_eq!(
        refusal(text_with(text, &[], &limits)),
        UnsafeBank::TooManyDefines { limit: 2 }
    );
    assert!(text_with("#define $A 1\n#define $B 2", &[], &limits).is_ok());
}

#[test]
fn define_text_beyond_its_cap_is_refused() {
    let limits = SfzLimits {
        define_bytes: 1024,
        ..SfzLimits::DEFAULT
    };
    let text = format!("#define $A {}", "1".repeat(2000));
    assert_eq!(
        refusal(text_with(&text, &[], &limits)),
        UnsafeBank::DefineTextTooLarge { limit_kib: 1 }
    );
}

#[test]
fn a_folder_with_too_many_entries_is_refused() {
    let limits = SfzLimits {
        folder_entries: 3,
        ..SfzLimits::DEFAULT
    };
    let dir = scratch_dir();
    let region = "<region> sample=pluck.wav";
    let root = pluck_bank(
        dir.path(),
        &[("a.sfz", region), ("b.sfz", region), ("c.sfz", region)],
    );
    assert_eq!(
        refusal(load(&root, &limits)),
        UnsafeBank::TooManyFiles { limit: 3 }
    );
}

#[test]
fn folders_nested_too_deep_are_refused() {
    let limits = SfzLimits {
        folder_depth: 2,
        ..SfzLimits::DEFAULT
    };
    let dir = scratch_dir();
    let root = pluck_bank(dir.path(), &[("a.sfz", "<region> sample=pluck.wav")]);
    fs::create_dir_all(root.join("1/2/3")).expect("folders");
    assert_eq!(
        refusal(load(&root, &limits)),
        UnsafeBank::FolderTooDeep { limit: 2 }
    );
    fs::remove_dir(root.join("1/2/3")).expect("folder removed");
    assert!(load(&root, &limits).is_ok());
}

#[test]
fn too_many_instrument_files_are_refused() {
    let limits = SfzLimits {
        sfz_files: 1,
        ..SfzLimits::DEFAULT
    };
    let dir = scratch_dir();
    let region = "<region> sample=pluck.wav";
    let root = pluck_bank(dir.path(), &[("a.sfz", region), ("b.sfz", region)]);
    assert_eq!(
        refusal(load(&root, &limits)),
        UnsafeBank::TooManyInstruments { limit: 1 }
    );
}

#[test]
fn a_sidecar_beyond_its_cap_is_refused() {
    let limits = SfzLimits {
        sidecar_bytes: 1024,
        ..SfzLimits::DEFAULT
    };
    let dir = scratch_dir();
    let root = pluck_bank(dir.path(), &[("a.sfz", "<region> sample=pluck.wav")]);
    fs::write(root.join(SIDECAR_FILE), " ".repeat(2048)).expect("sidecar");
    assert_eq!(
        refusal(load(&root, &limits)),
        UnsafeBank::SidecarTooLarge { limit_kib: 1 }
    );
}

#[test]
fn retained_regions_beyond_the_loader_budget_are_refused() {
    let limits = SfzLimits {
        loader_bytes: 4096,
        ..SfzLimits::DEFAULT
    };
    let text = "<group> amp_velcurve_1=0.1 amp_velcurve_2=0.2 amp_velcurve_3=0.3\n".to_owned()
        + &"<region> sample=pluck.wav\n".repeat(40);
    assert_eq!(
        refusal(text_with(&text, &[], &limits)),
        UnsafeBank::LoaderBudget
    );
    let dir = scratch_dir();
    let root = pluck_bank(dir.path(), &[("a.sfz", &text)]);
    assert_eq!(refusal(load(&root, &limits)), UnsafeBank::LoaderBudget);
    assert!(load(&root, &SfzLimits::DEFAULT).is_ok());
}

#[test]
fn sample_paths_that_leave_the_bank_are_dropped_and_listed() {
    let dir = scratch_dir();
    fs::write(dir.path().join("outside.wav"), wav16(&samples::pluck())).expect("outside");
    let root = write_bank(
        dir.path(),
        &[(
            "Programs/a.sfz",
            "<region> key=60 sample=../../outside.wav\n<region> key=61 sample=/etc/passwd\n<region> key=62 sample=C:\\outside.wav\n<region> key=65 sample=\\\\srv\\share\\outside.wav\n<region> key=63 sample=..\\Samples\\inside.wav\n<region> key=64 sample=../Samples/../../outside.wav\n",
        )],
        &[("Samples/inside.wav", wav16(&samples::pluck()))],
    );
    let bank = SfzBank::load(&root).expect("loads");
    let regions = bank.regions("Programs/a");
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].lokey, 63);
    assert_eq!(bank.sample_count(), 1);
    let outside = bank
        .ignored("Programs/a")
        .into_iter()
        .find(|entry| entry.reason == IgnoreReason::PathOutsideBank)
        .expect("listed");
    assert_eq!(outside.count, 5);
}

#[cfg(unix)]
fn symlinked_bank(target: &Path, link: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = scratch_dir();
    let root = pluck_bank(dir.path(), &[("a.sfz", "<region> key=61 sample=pluck.wav")]);
    std::os::unix::fs::symlink(target, root.join(link)).expect("link");
    (dir, root)
}

#[cfg(unix)]
#[test]
fn a_symbolic_link_anywhere_in_the_bank_refuses_it() {
    let dir = scratch_dir();
    let secret = dir.path().join("secret");
    fs::create_dir_all(&secret).expect("secret folder");
    fs::write(secret.join("loot.wav"), wav16(&samples::pluck())).expect("outside");
    let loot = secret.join("loot.wav");
    for (target, link) in [
        (Path::new("."), "a"),
        (Path::new("pluck.wav"), "copy.wav"),
        (secret.as_path(), "link"),
        (loot.as_path(), "loot.wav"),
        (loot.as_path(), SIDECAR_FILE),
        (Path::new("missing"), "dangling"),
    ] {
        let (_dir, root) = symlinked_bank(target, link);
        assert_eq!(
            refusal(load(&root, &SfzLimits::DEFAULT)),
            UnsafeBank::BankSymlink,
            "{link}"
        );
    }
    let dir = scratch_dir();
    let root = pluck_bank(
        dir.path(),
        &[("Deep/a.sfz", "<region> sample=../pluck.wav")],
    );
    fs::create_dir_all(root.join("Deep/Er")).expect("folder");
    std::os::unix::fs::symlink(root.join("Deep"), root.join("Deep/Er/loop")).expect("link");
    assert_eq!(
        refusal(load(&root, &SfzLimits::DEFAULT)),
        UnsafeBank::BankSymlink
    );
}

#[test]
fn case_insensitive_lookups_stop_at_the_listing_cap() {
    let dir = scratch_dir();
    let region = "<region> sample=PLUCK.WAV";
    let root = write_bank(
        dir.path(),
        &[("a.sfz", region)],
        &[
            ("pluck.wav", wav16(&samples::pluck())),
            ("b.wav", Vec::new()),
            ("c.wav", Vec::new()),
            ("d.wav", Vec::new()),
        ],
    );
    let limits = SfzLimits {
        listed_entries: 3,
        ..SfzLimits::DEFAULT
    };
    assert_eq!(
        refusal(load(&root, &limits)),
        UnsafeBank::ListedEntries { limit: 3 }
    );
    let bank = load(&root, &SfzLimits::DEFAULT).expect("loads");
    assert_eq!(bank.regions("a").len(), 1);
}

#[test]
fn one_folder_reached_through_many_spellings_is_listed_once() {
    let dir = scratch_dir();
    let text = "<region> sample=Samples/ONE.wav\n<region> sample=./Samples/../Samples/TWO.wav\n<region> sample=Programs/../Samples/THREE.wav\n<region> sample=samples/one.wav";
    let root = write_bank(
        dir.path(),
        &[("a.sfz", text), ("Programs/keep.txt", "")],
        &[
            ("Samples/one.wav", wav16(&samples::pluck())),
            ("Samples/two.wav", wav16(&samples::pluck())),
            ("Samples/three.wav", wav16(&samples::pluck())),
        ],
    );
    let bank = load(&root, &SfzLimits::DEFAULT).expect("loads");
    assert_eq!(bank.regions("a").len(), 4);
    assert_eq!(bank.sample_count(), 3);
    assert_eq!(bank.listed_entries(), 3 + 3);
}

#[test]
fn a_hostile_sample_is_refused_at_play_time_silenced_and_reported_once() {
    let dir = scratch_dir();
    let six = write_wav(
        &WavSpec {
            rate: 44_100,
            channels: 6,
            bits: 16,
            extensible: false,
        },
        &[1000; 600],
    );
    let root = write_bank(
        dir.path(),
        &[("a.sfz", "<region> sample=six.wav")],
        &[("six.wav", six)],
    );
    let bank = SfzBank::load(&root).expect("loads");
    let mut synth = bank.synth(SampleRate::STUDIO);
    let channel = Channel::new(0).expect("channel");
    for _ in 0..3 {
        synth.handle(
            channel,
            &MidiMessage::NoteOn {
                key: 60,
                velocity: 100,
            },
        );
    }
    assert_eq!(energy(&render(&mut synth, 4800).0), 0.0);
    let faults = bank.faults();
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert!(faults[0].message.contains("6 channels"));
    assert_eq!(bank.decode_count(), 1);
    assert_eq!(
        bank.preload("a"),
        Err(BankError::Unsafe {
            reason: UnsafeBank::SampleChannels { channels: 6 }
        })
    );
}

fn letters(index: usize) -> String {
    let mut name = String::from("zz");
    let mut rest = index;
    for _ in 0..3 {
        name.push(char::from(b'a' + (rest % 26) as u8));
        rest /= 26;
    }
    name
}

#[test]
fn listings_of_ignored_opcodes_count_against_the_loader_budget() {
    let limits = SfzLimits {
        loader_bytes: 16 << 10,
        ..SfzLimits::DEFAULT
    };
    let opcodes: Vec<String> = (0..400)
        .map(|index| format!("{}=1", letters(index)))
        .collect();
    let text = format!("<region> sample=pluck.wav {}", opcodes.join(" "));
    let dir = scratch_dir();
    let root = pluck_bank(dir.path(), &[("a.sfz", &text)]);
    assert_eq!(refusal(load(&root, &limits)), UnsafeBank::LoaderBudget);
    let bank = load(&root, &SfzLimits::DEFAULT).expect("loads");
    assert_eq!(bank.ignored("a").len(), 400);
    assert_eq!(
        refusal(text_with(&text, &[], &limits)),
        UnsafeBank::LoaderBudget
    );
}

const STEP_FACTOR: u64 = 4;

fn input_bytes(root: &str, files: &[(&str, &str)]) -> u64 {
    (root.len() + files.iter().map(|(_, text)| text.len()).sum::<usize>()) as u64
}

#[test]
fn comment_stripping_is_linear_in_its_input() {
    for text in [
        "//\n".repeat(20_000),
        "/*/".repeat(20_000),
        "/* */".repeat(15_000),
    ] {
        let run = measure_sfz_text(&text, &BTreeMap::new(), &SfzLimits::DEFAULT);
        assert!(run.result.is_ok());
        assert!(
            run.steps <= STEP_FACTOR * text.len() as u64,
            "{} steps for {} bytes",
            run.steps,
            text.len()
        );
    }
}

#[test]
fn define_lookups_stay_linear_because_names_are_capped() {
    let defines: String = (1..=200)
        .map(|length| format!("#define ${} 1\n", "N".repeat(length)))
        .collect();
    let uses = format!(
        "<region> sample=a.wav\n{}\n",
        format!("${} ", "M".repeat(200)).repeat(100)
    );
    let text = defines + &uses;
    let run = measure_sfz_text(&text, &BTreeMap::new(), &SfzLimits::DEFAULT);
    let parsed = run.result.expect("parses");
    assert!(
        parsed
            .ignored
            .iter()
            .any(|entry| entry.name == "#define" && entry.reason == IgnoreReason::UnknownDirective)
    );
    assert!(
        run.steps <= STEP_FACTOR * 4 * text.len() as u64,
        "{} steps for {} bytes",
        run.steps,
        text.len()
    );
}

#[test]
fn text_read_through_includes_is_capped() {
    let limits = SfzLimits {
        read_bytes: 16 << 10,
        ..SfzLimits::DEFAULT
    };
    let blank = "\n".repeat(2048);
    let many = "#include \"z.txt\"\n".repeat(50);
    let files = [("m.txt", many.as_str()), ("z.txt", blank.as_str())];
    let run = measure_sfz_text("#include \"m.txt\"\n", &files_map(&files), &limits);
    assert_eq!(
        run.result.map(|text| text.regions.len()),
        Err(BankError::Unsafe {
            reason: UnsafeBank::TextReadTooLarge { limit_mib: 0 }
        })
    );
    assert!(
        run.steps <= STEP_FACTOR * limits.read_bytes,
        "{} steps",
        run.steps
    );
    let parsed = text_with("#include \"m.txt\"\n", &files, &SfzLimits::DEFAULT).expect("parses");
    assert_eq!(
        parsed.read_bytes,
        input_bytes("#include \"m.txt\"\n", &files) + 49 * 2048 + 51 * 5
    );
    let dir = scratch_dir();
    let root = pluck_bank(
        dir.path(),
        &[
            ("root.sfz", "#include \"inc/m.txt\"\n"),
            ("inc/m.txt", &many.replace("z.txt", "inc/z.txt")),
            ("inc/z.txt", &blank),
        ],
    );
    assert_eq!(
        refusal(load(&root, &limits)),
        UnsafeBank::TextReadTooLarge { limit_mib: 0 }
    );
}

fn files_map(files: &[(&str, &str)]) -> BTreeMap<String, String> {
    files
        .iter()
        .map(|(name, text)| ((*name).to_owned(), (*text).to_owned()))
        .collect()
}

#[test]
fn a_line_longer_than_its_cap_is_refused() {
    let limits = SfzLimits {
        line_bytes: 1024,
        ..SfzLimits::DEFAULT
    };
    let long = format!("<region> sample=a.wav {}", "x=1 ".repeat(300));
    assert_eq!(
        refusal(text_with(&long, &[], &limits)),
        UnsafeBank::LineTooLong { limit_kib: 1 }
    );
    let short = "<region> sample=a.wav\n".repeat(300);
    assert!(text_with(&short, &[], &limits).is_ok());
}

#[test]
fn resolved_sample_paths_count_against_the_loader_budget() {
    let regions: String = (0..10)
        .map(|index| format!("<region> sample={index}{}.wav\n", "s".repeat(240)))
        .collect();
    let limit = (1..256u64)
        .map(|kib| kib << 10)
        .find(|bytes| {
            let limits = SfzLimits {
                loader_bytes: *bytes,
                ..SfzLimits::DEFAULT
            };
            text_with(&regions, &[], &limits).is_ok()
        })
        .expect("some budget parses the text");
    let limits = SfzLimits {
        loader_bytes: limit,
        ..SfzLimits::DEFAULT
    };
    let dir = scratch_dir();
    let root = pluck_bank(dir.path(), &[("a.sfz", &regions)]);
    assert_eq!(refusal(load(&root, &limits)), UnsafeBank::LoaderBudget);
}

fn bank_with_files(dir: &Path, sfz: &str, files: usize) -> std::path::PathBuf {
    let root = pluck_bank(dir, &[("a.sfz", sfz)]);
    for index in 0..files {
        fs::write(root.join(format!("f{index:05}")), b"").expect("entry");
    }
    root
}

#[test]
fn case_misses_over_a_large_folder_cost_one_listing_and_cached_lookups() {
    let dir = scratch_dir();
    let distinct: String = (0..500)
        .map(|index| format!("#include \"MISS{index}.TXT\"\n"))
        .collect();
    let repeated = "#include \"../M\"\n".repeat(2000);
    let root = bank_with_files(dir.path(), &(distinct + &repeated), 2000);
    let bank = load(&root, &SfzLimits::DEFAULT).expect("loads");
    assert!(
        bank.path_lookups() <= 2 * 502,
        "{} lookups",
        bank.path_lookups()
    );
    assert!(
        bank.listed_entries() <= 2010,
        "{} listed",
        bank.listed_entries()
    );
    let folder_bytes = 2010 * 8;
    let lookup_bytes = 2 * 502 * 2 * 16;
    assert!(
        bank.lookup_steps() <= STEP_FACTOR * (folder_bytes + lookup_bytes),
        "{} steps",
        bank.lookup_steps()
    );
}

#[test]
fn distinct_path_lookups_are_capped() {
    let limits = SfzLimits {
        path_lookups: 50,
        ..SfzLimits::DEFAULT
    };
    let dir = scratch_dir();
    let regions: String = (0..100)
        .map(|index| format!("<region> sample=s{index}.wav\n"))
        .collect();
    let root = pluck_bank(dir.path(), &[("a.sfz", &regions)]);
    assert_eq!(
        refusal(load(&root, &limits)),
        UnsafeBank::PathLookups { limit: 50 }
    );
    assert!(load(&root, &SfzLimits::DEFAULT).is_ok());
}

#[test]
fn an_include_path_expanding_past_its_cap_is_refused_with_bounded_work() {
    let text = format!("#define $A {}\n#include \"$A$A$A$A\"\n", "x".repeat(600));
    let run = measure_sfz_text(&text, &BTreeMap::new(), &SfzLimits::DEFAULT);
    assert_eq!(
        run.result.map(|parsed| parsed.regions.len()),
        Err(BankError::Unsafe {
            reason: UnsafeBank::PathTooLong { limit: 1024 }
        })
    );
    assert!(
        run.steps <= STEP_FACTOR * (1024 + text.len() as u64),
        "{} steps",
        run.steps
    );
    let dir = scratch_dir();
    let root = pluck_bank(dir.path(), &[("a.sfz", &text)]);
    assert_eq!(
        refusal(load(&root, &SfzLimits::DEFAULT)),
        UnsafeBank::PathTooLong { limit: 1024 }
    );
}

#[test]
fn a_sample_path_past_its_cap_is_refused() {
    let text = format!(
        "<control> default_path={}/\n<region> sample={}.wav\n",
        "d".repeat(600),
        "s".repeat(600)
    );
    assert_eq!(
        refusal(text_with(&text, &[], &SfzLimits::DEFAULT)),
        UnsafeBank::PathTooLong { limit: 1024 }
    );
}

#[test]
fn sample_path_text_counts_against_the_bank_text_cap() {
    let text = format!(
        "<control> default_path={}/\n{}",
        "d".repeat(500),
        "<region> sample=x.wav\n".repeat(100)
    );
    let limits = SfzLimits {
        bank_text_bytes: 30 << 10,
        ..SfzLimits::DEFAULT
    };
    let dir = scratch_dir();
    let root = pluck_bank(dir.path(), &[("a.sfz", &text)]);
    assert_eq!(
        refusal(load(&root, &limits)),
        UnsafeBank::BankTextTooLarge { limit_mib: 0 }
    );
    assert!(load(&root, &SfzLimits::DEFAULT).is_ok());
}

#[cfg(unix)]
#[test]
fn an_unreadable_folder_refuses_the_bank() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch_dir();
    let root = pluck_bank(dir.path(), &[("a.sfz", "<region> sample=pluck.wav")]);
    let hidden = root.join("hidden");
    fs::create_dir_all(&hidden).expect("folder");
    fs::set_permissions(&hidden, fs::Permissions::from_mode(0o311)).expect("mode");
    let readable = fs::read_dir(&hidden).is_ok();
    let result = load(&root, &SfzLimits::DEFAULT).map(|bank| bank.sample_count());
    fs::set_permissions(&hidden, fs::Permissions::from_mode(0o755)).expect("mode restored");
    if !readable {
        assert_eq!(
            result,
            Err(BankError::Unsafe {
                reason: UnsafeBank::UnreadableFolder
            })
        );
    }
}

fn include_list_bank(dir: &Path, roots: usize, lines: usize) -> std::path::PathBuf {
    let list = "#include \"e.txt\"\n".repeat(lines);
    let mut files: Vec<(String, String)> = vec![
        ("e.txt".to_owned(), String::new()),
        ("inc.txt".to_owned(), list),
    ];
    for index in 0..roots {
        files.push((
            format!("r{index:04}.sfz"),
            "#include \"inc.txt\"\n<region> sample=pluck.wav\n".to_owned(),
        ));
    }
    let named: Vec<(&str, &str)> = files
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    pluck_bank(dir, &named)
}

#[test]
fn include_occurrences_across_the_bank_are_capped() {
    let dir = scratch_dir();
    let root = include_list_bank(dir.path(), 20, 50);
    let limits = SfzLimits {
        bank_includes: 1000,
        ..SfzLimits::DEFAULT
    };
    assert_eq!(
        refusal(load(&root, &limits)),
        UnsafeBank::BankIncludes { limit: 1000 }
    );
    let bank = load(&root, &SfzLimits::DEFAULT).expect("loads");
    assert_eq!(bank.instrument_names().len(), 20);
}

#[test]
fn the_path_lookup_cache_counts_against_the_loader_budget_while_expanding() {
    let spellings: String = (0..200)
        .map(|index| format!("#include \"{}e.txt\"\n", "./".repeat(index + 1)))
        .collect();
    let looped = spellings + "#include \"a.sfz\"\n";
    let dir = scratch_dir();
    let root = pluck_bank(dir.path(), &[("a.sfz", &looped), ("e.txt", "")]);
    let limits = SfzLimits {
        loader_bytes: 32 << 10,
        ..SfzLimits::DEFAULT
    };
    assert_eq!(refusal(load(&root, &limits)), UnsafeBank::LoaderBudget);
    assert_eq!(
        refusal(load(&root, &SfzLimits::DEFAULT)),
        UnsafeBank::IncludeCycle
    );
}

fn distinct_directives(count: usize) -> String {
    (0..count)
        .map(|index| format!("#{}\n", letters(index)))
        .collect()
}

fn distinct_opcodes(count: usize) -> String {
    (0..count)
        .map(|index| format!("{}=1", letters(index)))
        .collect::<Vec<String>>()
        .join(" ")
}

#[test]
fn directive_listings_are_charged_against_the_loader_budget_while_expanding() {
    let limits = SfzLimits {
        loader_bytes: 16 << 10,
        ..SfzLimits::DEFAULT
    };
    let looped = distinct_directives(400) + "#include \"a.sfz\"\n";
    let dir = scratch_dir();
    let root = pluck_bank(dir.path(), &[("a.sfz", &looped)]);
    assert_eq!(refusal(load(&root, &limits)), UnsafeBank::LoaderBudget);
    assert_eq!(
        refusal(load(&root, &SfzLimits::DEFAULT)),
        UnsafeBank::IncludeCycle
    );
    let memory = distinct_directives(400) + "#include \"root.sfz\"\n";
    let files = [("root.sfz", memory.as_str())];
    assert_eq!(
        refusal(text_with(&memory, &files, &limits)),
        UnsafeBank::LoaderBudget
    );
    assert_eq!(
        refusal(text_with(&memory, &files, &SfzLimits::DEFAULT)),
        UnsafeBank::IncludeCycle
    );
}

#[test]
fn distinct_ignored_names_are_capped() {
    let limits = SfzLimits {
        ignored_names: 100,
        ..SfzLimits::DEFAULT
    };
    let over = format!("<region> sample=pluck.wav {}", distinct_opcodes(101));
    assert_eq!(
        refusal(text_with(&over, &[], &limits)),
        UnsafeBank::IgnoredNames { limit: 100 }
    );
    let at = format!("<region> sample=pluck.wav {}", distinct_opcodes(100));
    assert_eq!(
        text_with(&at, &[], &limits).expect("parses").ignored.len(),
        100
    );
    let dir = scratch_dir();
    let looped = distinct_directives(101) + "#include \"b.sfz\"\n";
    let root = pluck_bank(dir.path(), &[("a.sfz", &over), ("b.sfz", &looped)]);
    assert_eq!(
        refusal(load(&root, &limits)),
        UnsafeBank::IgnoredNames { limit: 100 }
    );
    let directives_only = scratch_dir();
    let root = pluck_bank(directives_only.path(), &[("b.sfz", &looped)]);
    assert_eq!(
        refusal(load(&root, &limits)),
        UnsafeBank::IgnoredNames { limit: 100 }
    );
    assert_eq!(
        refusal(load(&root, &SfzLimits::DEFAULT)),
        UnsafeBank::IncludeCycle
    );
}

#[test]
fn unknown_header_names_are_charged_and_stop_the_parse() {
    let limits = SfzLimits {
        loader_bytes: 16 << 10,
        ..SfzLimits::DEFAULT
    };
    let headers: String = (0..400)
        .map(|index| format!("<{}>\n", letters(index)))
        .collect();
    let text = headers + &"<group>\n".repeat(2000);
    let run = measure_sfz_text(&text, &BTreeMap::new(), &limits);
    assert_eq!(
        run.result.err(),
        Some(BankError::Unsafe {
            reason: UnsafeBank::LoaderBudget
        })
    );
    assert!(
        run.steps <= STEP_FACTOR * text.len() as u64,
        "{} steps for {} bytes",
        run.steps,
        text.len()
    );
    assert!(text_with(&text, &[], &SfzLimits::DEFAULT).is_ok());
}

const SLOT_STEPS: u64 = 3 * 128;

fn full_group() -> String {
    let mut group = String::from("<group>");
    for number in 0..128 {
        group.push_str(&format!(
            " amp_velcurve_{number}=0.5 locc{number}=1 hicc{number}=120"
        ));
    }
    group + "\n"
}

#[test]
fn regions_without_samples_count_against_the_region_cap() {
    let limits = SfzLimits {
        regions: 10,
        ..SfzLimits::DEFAULT
    };
    let over = format!("{}{}", full_group(), "<region>\n".repeat(11));
    assert_eq!(
        refusal(text_with(&over, &[], &limits)),
        UnsafeBank::TooManyRegions { limit: 10 }
    );
    let at = format!("{}{}", full_group(), "<region>\n".repeat(10));
    assert!(
        text_with(&at, &[], &limits)
            .expect("parses")
            .regions
            .is_empty()
    );
    let dir = scratch_dir();
    let root = pluck_bank(dir.path(), &[("a.sfz", &over)]);
    assert_eq!(
        refusal(load(&root, &limits)),
        UnsafeBank::TooManyRegions { limit: 10 }
    );
}

#[test]
fn inheriting_costs_a_fixed_number_of_slots_per_region() {
    let measure = |regions: usize| {
        let text = format!("{}{}", full_group(), "<region>\n".repeat(regions));
        let run = measure_sfz_text(&text, &BTreeMap::new(), &SfzLimits::DEFAULT);
        assert!(run.result.is_ok());
        (run.steps, text.len() as u64)
    };
    let (small, _) = measure(100);
    let (large, length) = measure(200);
    let extra = large - small;
    assert!(
        extra >= 100 * 5 * SLOT_STEPS,
        "{extra} steps for 100 regions"
    );
    assert!(
        extra <= 100 * 8 * SLOT_STEPS,
        "{extra} steps for 100 regions"
    );
    assert!(
        large <= STEP_FACTOR * length + 200 * 8 * SLOT_STEPS,
        "{large} steps for {length} bytes"
    );
}

#[test]
fn listings_merged_after_parsing_stay_capped() {
    let limits = SfzLimits {
        ignored_names: 100,
        ..SfzLimits::DEFAULT
    };
    let both = format!(
        "{}<region> sample=pluck.wav {}\n",
        distinct_directives(60),
        distinct_opcodes(60)
    );
    assert_eq!(
        refusal(text_with(&both, &[], &limits)),
        UnsafeBank::IgnoredNames { limit: 100 }
    );
    assert!(text_with(&both, &[], &SfzLimits::DEFAULT).is_ok());
    let one = SfzLimits {
        ignored_names: 1,
        ..SfzLimits::DEFAULT
    };
    let missing = "zzaaa=1\n<region> sample=pluck.wav\n<region> sample=gone.wav\n";
    let dir = scratch_dir();
    let root = pluck_bank(dir.path(), &[("a.sfz", missing)]);
    assert_eq!(
        refusal(load(&root, &one)),
        UnsafeBank::IgnoredNames { limit: 1 }
    );
    assert_eq!(
        load(&root, &SfzLimits::DEFAULT)
            .expect("loads")
            .ignored("a")
            .len(),
        2
    );
}

#[test]
fn regions_and_listings_share_one_parse_budget() {
    let limits = SfzLimits {
        loader_bytes: 16 << 10,
        ..SfzLimits::DEFAULT
    };
    let region_cost =
        std::mem::size_of::<opentab_audio::SfzRegion>() + std::mem::size_of::<usize>() + 9;
    let regions = "<region> sample=pluck.wav\n".repeat((8 << 10) / region_cost);
    let listed = format!("<group> {}\n", distinct_opcodes(100));
    assert!(text_with(&regions, &[], &limits).is_ok());
    assert!(text_with(&listed, &[], &limits).is_ok());
    assert_eq!(
        refusal(text_with(&(regions + &listed), &[], &limits)),
        UnsafeBank::LoaderBudget
    );
}

fn inherited_generator(value_bytes: usize, uses: usize, region_lines: usize) -> String {
    format!(
        "#define $a {}\n#define $r {}\n<group> sample=*{}\n{}\n",
        "a".repeat(value_bytes),
        "<region>".repeat(8000),
        "$a".repeat(uses),
        "$r".repeat(region_lines)
    )
}

#[test]
fn sample_values_past_the_path_cap_are_refused_before_they_are_stored() {
    let over = UnsafeBank::PathTooLong { limit: 1024 };
    for text in [
        format!("<region> sample={}\n", "s".repeat(1025)),
        format!("<group> sample=*{}\n<region>\n", "g".repeat(1024)),
        format!("<global> sample=$zz{}\n<region>\n", "v".repeat(1022)),
        format!("#define $a {}\n<master> sample=$a$a\n", "d".repeat(600)),
        inherited_generator(1024, 2, 1),
    ] {
        assert_eq!(refusal(text_with(&text, &[], &SfzLimits::DEFAULT)), over);
    }
    let at = format!("<region> sample={}\n", "s".repeat(1024));
    assert_eq!(
        text_with(&at, &[], &SfzLimits::DEFAULT)
            .expect("parses")
            .regions
            .len(),
        1
    );
    let generator = format!("<group> sample=*{}\n<region>\n", "g".repeat(1023));
    let parsed = text_with(&generator, &[], &SfzLimits::DEFAULT).expect("parses");
    assert!(parsed.regions.is_empty());
    assert!(
        parsed
            .ignored
            .iter()
            .any(|entry| entry.reason == IgnoreReason::Generator)
    );
    let dir = scratch_dir();
    let root = pluck_bank(
        dir.path(),
        &[
            ("a.sfz", "<region> sample=pluck.wav\n"),
            ("b.sfz", &generator),
        ],
    );
    assert!(load(&root, &SfzLimits::DEFAULT).is_ok());
    let over_generator = format!("<group> sample=*{}\n<region>\n", "g".repeat(1024));
    let root = pluck_bank(dir.path(), &[("b.sfz", &over_generator)]);
    assert_eq!(refusal(load(&root, &SfzLimits::DEFAULT)), over);
}

#[test]
fn the_inherited_generator_bomb_is_refused_with_bounded_work() {
    let text = inherited_generator(1024, 240, 12);
    let expanded = (text.len() + 240 * 1024 + 12 * 8000 * 8) as u64;
    let wide = SfzLimits {
        line_bytes: 1 << 20,
        ..SfzLimits::DEFAULT
    };
    for (limits, reason) in [
        (
            SfzLimits::DEFAULT,
            UnsafeBank::LineTooLong { limit_kib: 64 },
        ),
        (wide, UnsafeBank::PathTooLong { limit: 1024 }),
    ] {
        let run = measure_sfz_text(&text, &BTreeMap::new(), &limits);
        assert_eq!(
            run.result.map(|parsed| parsed.regions.len()),
            Err(BankError::Unsafe { reason })
        );
        assert!(
            run.steps <= STEP_FACTOR * expanded,
            "{} steps for {expanded} expanded bytes",
            run.steps
        );
    }
}

#[test]
fn expanded_lines_past_the_line_cap_are_refused() {
    let limits = SfzLimits {
        line_bytes: 1024,
        ..SfzLimits::DEFAULT
    };
    let define = format!("#define $r {}\n", "<region>".repeat(100));
    assert!(text_with(&format!("{define}$r\n$r\n"), &[], &limits).is_ok());
    assert_eq!(
        refusal(text_with(&format!("{define}$r$r\n"), &[], &limits)),
        UnsafeBank::LineTooLong { limit_kib: 1 }
    );
    let fragment = "<region>".repeat(70);
    let include = format!("{} #include \"f.sfz\"\n", "<region>".repeat(70));
    assert_eq!(
        refusal(text_with(&include, &[("f.sfz", &fragment)], &limits)),
        UnsafeBank::LineTooLong { limit_kib: 1 }
    );
}

#[test]
fn one_name_under_many_scopes_and_reasons_counts_once_against_the_name_cap() {
    let limits = SfzLimits {
        ignored_names: 1,
        ..SfzLimits::DEFAULT
    };
    let one =
        "<region> sample=pluck.wav zzq=1 zzq=$v\n<group> zzq=1\n<global> zzq=1\n<master> zzq=1\n";
    let parsed = text_with(one, &[], &limits).expect("one name fits a cap of one");
    assert_eq!(parsed.ignored.len(), 5);
    assert!(parsed.ignored.iter().all(|entry| entry.name == "zzq"));
    let two = format!("{one}<region> sample=pluck.wav yyq=1\n");
    assert_eq!(
        refusal(text_with(&two, &[], &limits)),
        UnsafeBank::IgnoredNames { limit: 1 }
    );
}
