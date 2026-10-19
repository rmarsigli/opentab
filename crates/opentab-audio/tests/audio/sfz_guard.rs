use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use opentab_audio::{
    BankError, IgnoreReason, SampleRate, SfzBank, SfzLimits, Synth, UnsafeBank, parse_sfz_text,
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
            "<region> key=60 sample=../../outside.wav\n<region> key=61 sample=/etc/passwd\n<region> key=62 sample=C:\\outside.wav\n<region> key=63 sample=..\\Samples\\inside.wav\n<region> key=64 sample=../Samples/../../outside.wav\n",
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
    assert_eq!(outside.count, 4);
}

#[cfg(unix)]
#[test]
fn a_symbolic_link_out_of_the_bank_is_dropped() {
    let dir = scratch_dir();
    let secret = dir.path().join("secret");
    fs::create_dir_all(&secret).expect("secret folder");
    fs::write(secret.join("loot.wav"), wav16(&samples::pluck())).expect("outside");
    fs::write(secret.join("loot.sfz"), "<region> sample=loot.wav").expect("outside sfz");
    let root = write_bank(
        dir.path(),
        &[(
            "a.sfz",
            "<region> key=60 sample=link/loot.wav\n#include \"link/loot.sfz\"\n<region> key=61 sample=pluck.wav",
        )],
        &[("pluck.wav", wav16(&samples::pluck()))],
    );
    std::os::unix::fs::symlink(&secret, root.join("link")).expect("link");
    let bank = SfzBank::load(&root).expect("loads");
    let regions = bank.regions("a");
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].lokey, 61);
    let ignored = bank.ignored("a");
    assert!(
        ignored
            .iter()
            .any(|entry| entry.name == "#include" && entry.reason == IgnoreReason::PathOutsideBank)
    );
    assert!(
        ignored
            .iter()
            .any(|entry| entry.name == "sample" && entry.reason == IgnoreReason::PathOutsideBank)
    );
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
}
