use std::path::Path;

use midly::{Format, Smf, Timing};
use serde_json::{Value, json};

use crate::support::{gp_fixture, path_str, run, run_in, temp};

struct Midi {
    format: Format,
    ticks: u16,
    tracks: usize,
}

fn midi(path: &Path) -> Midi {
    let bytes = std::fs::read(path).expect("read mid");
    let smf = Smf::parse(&bytes).expect("a valid SMF");
    let Timing::Metrical(ticks) = smf.header.timing else {
        panic!("not metrical");
    };
    Midi {
        format: smf.header.format,
        ticks: ticks.as_int(),
        tracks: smf.tracks.len(),
    }
}

fn export(dir: &Path, fixture: &str, profile: Option<&str>, name: &str) -> (Value, Midi) {
    let out = dir.join(name);
    let input = gp_fixture(fixture);
    let mut args = vec![
        "export",
        path_str(&input),
        "--format",
        "mid",
        "--out",
        path_str(&out),
        "--json",
    ];
    if let Some(profile) = profile {
        args.extend(["--profile", profile]);
    }
    let result = run_in(dir, &args);
    assert_eq!(result.code, 0, "{}", result.stderr);
    (result.json(), midi(&out))
}

#[test]
fn the_gp_profile_writes_gp_timing_and_one_track_per_staff() {
    let dir = temp();
    let (body, file) = export(dir.path(), "multi-track.gp", Some("gp"), "gp.mid");
    assert_eq!(body["profile"], json!("gp"));
    assert_eq!(body["ppq"], json!(480));
    assert_eq!(file.ticks, 480);
    assert_eq!(file.format, Format::Parallel);
    assert!(file.tracks >= 5, "{}", file.tracks);
}

#[test]
fn the_musical_profile_is_the_default_and_writes_960_ticks_per_quarter() {
    let dir = temp();
    let (explicit, explicit_file) =
        export(dir.path(), "multi-track.gp", Some("musical"), "musical.mid");
    let (implicit, implicit_file) = export(dir.path(), "multi-track.gp", None, "default.mid");
    assert_eq!(explicit["ppq"], json!(960));
    assert_eq!(explicit_file.ticks, 960);
    assert_eq!(implicit["profile"], json!("musical"));
    assert_eq!(implicit_file.ticks, 960);
    assert_eq!(
        std::fs::read(dir.path().join("musical.mid")).unwrap(),
        std::fs::read(dir.path().join("default.mid")).unwrap()
    );
    assert_eq!(explicit_file.tracks, implicit_file.tracks);
}

#[test]
fn the_two_profiles_write_different_files() {
    let dir = temp();
    export(dir.path(), "tempo-ramp.gp", Some("gp"), "gp.mid");
    export(dir.path(), "tempo-ramp.gp", Some("musical"), "musical.mid");
    assert_ne!(
        std::fs::read(dir.path().join("gp.mid")).unwrap(),
        std::fs::read(dir.path().join("musical.mid")).unwrap()
    );
}

#[test]
fn the_json_result_reports_files_duration_played_bars_and_unplayed_features() {
    let dir = temp();
    let (body, _) = export(dir.path(), "repeats.gp", Some("gp"), "repeats.mid");
    let out = dir.path().join("repeats.mid");
    assert_eq!(body["files"], json!([path_str(&out)]));
    assert_eq!(body["bytes"], json!(std::fs::metadata(&out).unwrap().len()));
    assert_eq!(body["seconds"], json!(32.0));
    assert_eq!(
        body["played_bars"],
        json!([1, 2, 1, 2, 1, 2, 3, 4, 3, 4, 3, 5, 6, 7, 8, 8])
    );
    assert_eq!(body["unplayed"], json!([]));
    assert_eq!(body["warnings"], json!([]));
    let (effects, _) = export(dir.path(), "beat-effects.gp", Some("musical"), "fx.mid");
    let kinds: Vec<&str> = effects["unplayed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["kind"].as_str().unwrap())
        .collect();
    assert!(
        kinds.contains(&"stroke") && kinds.contains(&"slap"),
        "{kinds:?}"
    );
    assert_eq!(effects["unplayed"][0]["track"], json!(1));
    assert_eq!(effects["unplayed"][0]["first_bar"], json!(2));
    let (ramp, _) = export(dir.path(), "tempo-ramp.gp", Some("gp"), "ramp.mid");
    assert_eq!(ramp["unplayed"][0]["kind"], json!("tempo_ramp"));
    assert_eq!(ramp["unplayed"][0]["track"], Value::Null);
}

#[test]
fn the_human_result_names_the_file_and_the_profile() {
    let dir = temp();
    let out = dir.path().join("a.MID");
    let input = gp_fixture("repeats.gp");
    let result = run_in(
        dir.path(),
        &[
            "export",
            path_str(&input),
            "--format",
            "mid",
            "--profile",
            "gp",
            "--out",
            path_str(&out),
        ],
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(result.stdout.starts_with("wrote "), "{}", result.stdout);
    assert!(result.stdout.contains("profile gp"), "{}", result.stdout);
    assert!(out.is_file());
}

#[test]
fn an_existing_file_is_replaced() {
    let dir = temp();
    let out = dir.path().join("a.mid");
    std::fs::write(&out, b"old").unwrap();
    let input = gp_fixture("repeats.gp");
    let result = run_in(
        dir.path(),
        &[
            "export",
            path_str(&input),
            "--format",
            "mid",
            "--out",
            path_str(&out),
        ],
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(&std::fs::read(&out).unwrap()[..4], b"MThd");
}

#[test]
fn a_wrong_output_name_or_directory_is_an_input_error() {
    let dir = temp();
    let input = gp_fixture("repeats.gp");
    let wrong = dir.path().join("a.wav");
    let result = run(&[
        "export",
        path_str(&input),
        "--format",
        "mid",
        "--out",
        path_str(&wrong),
        "--json",
    ]);
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["kind"], json!("unsupported_format"));
    assert!(!wrong.exists());
    let missing = dir.path().join("nowhere").join("a.mid");
    let result = run(&[
        "export",
        path_str(&input),
        "--format",
        "mid",
        "--out",
        path_str(&missing),
        "--json",
    ]);
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["kind"], json!("io"));
}

#[test]
fn a_missing_input_is_an_io_error_and_a_wrong_option_is_a_usage_error() {
    let dir = temp();
    let out = dir.path().join("a.mid");
    let gone = dir.path().join("gone.gp");
    let result = run(&[
        "export",
        path_str(&gone),
        "--format",
        "mid",
        "--out",
        path_str(&out),
        "--json",
    ]);
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["kind"], json!("io"));
    let input = gp_fixture("repeats.gp");
    for extra in [["--format", "wav"], ["--profile", "fast"]] {
        let mut args = vec![
            "export",
            path_str(&input),
            "--format",
            "mid",
            "--out",
            path_str(&out),
            "--json",
        ];
        args.extend(extra);
        let result = run(&args);
        assert_eq!(result.code, 1, "{extra:?}");
        assert_eq!(result.json()["error"]["kind"], json!("usage"));
    }
}
