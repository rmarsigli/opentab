use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::support::{gp_fixture, path_str, run_in, temp};

const RATE: u32 = 48_000;
const BAR_FRAMES: u64 = 2 * RATE as u64;
const TAIL_FRAMES: u64 = 3 * RATE as u64;

struct Wav {
    channels: u16,
    rate: u32,
    bits: u16,
    frames: u32,
    peak: i32,
    samples: Vec<i16>,
}

fn read_wav(path: &Path) -> Wav {
    let mut reader = hound::WavReader::open(path).expect("a valid wav");
    let spec = reader.spec();
    let samples: Vec<i16> = reader
        .samples::<i16>()
        .map(|sample| sample.expect("a sample"))
        .collect();
    let peak = samples
        .iter()
        .map(|sample| i32::from(*sample).abs())
        .max()
        .unwrap_or(0);
    Wav {
        channels: spec.channels,
        rate: spec.sample_rate,
        bits: spec.bits_per_sample,
        frames: reader.duration(),
        peak,
        samples,
    }
}

struct Rendered {
    body: Value,
    path: PathBuf,
}

impl Rendered {
    fn wav(&self) -> Wav {
        read_wav(&self.path)
    }

    fn frames(&self) -> u64 {
        self.body["frames"].as_u64().expect("frames")
    }

    fn played(&self) -> Value {
        self.body["played_bars"].clone()
    }
}

fn audio(data: &Path, fixture: &str, name: &str, extra: &[&str]) -> Rendered {
    let out = data.join(name);
    let input = gp_fixture(fixture);
    let mut args = vec!["audio", path_str(&input), "--out", path_str(&out), "--json"];
    args.extend(extra);
    let result = run_in(data, &args);
    assert_eq!(result.code, 0, "{extra:?}: {}", result.stderr);
    Rendered {
        body: result.json(),
        path: out,
    }
}

fn failure(data: &Path, fixture: &str, extra: &[&str]) -> (i32, Value) {
    let out = data.join("failed.wav");
    let input = gp_fixture(fixture);
    let mut args = vec!["audio", path_str(&input), "--out", path_str(&out), "--json"];
    args.extend(extra);
    let result = run_in(data, &args);
    assert!(!out.exists(), "{extra:?}");
    (result.code, result.json())
}

fn import_tiny_sf2(data: &Path) -> String {
    let file = data.join("tiny.sf2");
    std::fs::write(&file, opentab_testkit::banks::tiny_sf2()).expect("write bank");
    let result = run_in(data, &["soundbank", "import", path_str(&file), "--json"]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    result.json()["imported"]["id"]
        .as_str()
        .expect("an id")
        .to_owned()
}

fn import_tiny_sfz(data: &Path) -> String {
    let root = opentab_testkit::banks::tiny_sfz(&data.join("sfz-source"));
    let result = run_in(data, &["soundbank", "import", path_str(&root), "--json"]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    result.json()["imported"]["id"]
        .as_str()
        .expect("an id")
        .to_owned()
}

#[test]
fn audio_writes_a_stereo_48_khz_wav_that_the_json_describes() {
    let dir = temp();
    let rendered = audio(dir.path(), "repeats.gp", "song.wav", &[]);
    let wav = rendered.wav();
    assert_eq!((wav.channels, wav.rate, wav.bits), (2, RATE, 16));
    assert_eq!(u64::from(wav.frames), rendered.frames());
    assert!(wav.peak > 1000, "{}", wav.peak);
    assert_eq!(rendered.body["files"], json!([path_str(&rendered.path)]));
    assert_eq!(rendered.body["sample_rate"], json!(RATE));
    assert_eq!(rendered.body["profile"], json!("musical"));
    assert_eq!(rendered.body["speed"], json!(100));
    assert_eq!(rendered.body["tracks"], json!([1]));
    assert_eq!(rendered.body["bars"], Value::Null);
    assert_eq!(rendered.body["unplayed"], json!([]));
    assert_eq!(rendered.body["warnings"], json!([]));
    assert_eq!(
        rendered.played(),
        json!([1, 2, 1, 2, 1, 2, 3, 4, 3, 4, 3, 5, 6, 7, 8, 8])
    );
    let seconds = rendered.body["seconds"].as_f64().unwrap();
    assert!((seconds - 35.0).abs() < 0.001, "{seconds}");
    assert_eq!(
        rendered.body["backends"],
        json!([{"track": 1, "format": "built_in", "bank": "built-in", "fallback": null}])
    );
}

#[test]
fn the_same_command_writes_the_same_bytes() {
    let dir = temp();
    let first = audio(dir.path(), "multi-track.gp", "a.wav", &["--bars", "1"]);
    let second = audio(dir.path(), "multi-track.gp", "b.wav", &["--bars", "1"]);
    assert_eq!(
        std::fs::read(&first.path).unwrap(),
        std::fs::read(&second.path).unwrap()
    );
}

#[test]
fn bars_are_source_bars_counted_from_one_and_follow_the_repeats() {
    let dir = temp();
    let single = audio(dir.path(), "repeats.gp", "one.wav", &["--bars", "2"]);
    assert_eq!(single.played(), json!([2]));
    assert_eq!(single.frames(), BAR_FRAMES + TAIL_FRAMES);
    assert_eq!(single.body["bars"], json!({"first": 2, "last": 2}));
    let first = audio(dir.path(), "repeats.gp", "first.wav", &["--bars", "1"]);
    assert_eq!(first.played(), json!([1]));
    let inner = audio(dir.path(), "repeats.gp", "inner.wav", &["--bars", "3-4"]);
    assert_eq!(inner.played(), json!([3, 4, 3, 4, 3]));
    assert_eq!(inner.frames(), 5 * BAR_FRAMES + TAIL_FRAMES);
    let after = audio(dir.path(), "repeats.gp", "after.wav", &["--bars", "5-6"]);
    assert_eq!(after.played(), json!([5, 6]));
    assert_eq!(after.frames(), 2 * BAR_FRAMES + TAIL_FRAMES);
    let last = audio(dir.path(), "repeats.gp", "last.wav", &["--bars", "8"]);
    assert_eq!(last.played(), json!([8, 8]));
}

#[test]
fn a_bar_range_stops_at_the_frame_where_its_last_bar_ends() {
    let dir = temp();
    let one = audio(dir.path(), "repeats.gp", "one.wav", &["--bars", "5"]);
    let two = audio(dir.path(), "repeats.gp", "two.wav", &["--bars", "5-6"]);
    assert_eq!(one.played(), json!([5]));
    assert_eq!(two.played(), json!([5, 6]));
    let boundary = 2 * BAR_FRAMES as usize;
    let first = one.wav();
    let second = two.wav();
    assert_eq!(first.samples.len(), boundary + 2 * TAIL_FRAMES as usize);
    assert_eq!(
        second.samples.len(),
        2 * boundary + 2 * TAIL_FRAMES as usize
    );
    assert_eq!(first.samples[..boundary], second.samples[..boundary]);
    assert!(first.samples[..boundary].iter().any(|sample| *sample != 0));
    let after = &second.samples[boundary..2 * boundary];
    assert!(after.iter().any(|sample| *sample != 0));
}

#[test]
fn bars_outside_the_score_or_never_played_are_input_errors() {
    let dir = temp();
    let (code, body) = failure(dir.path(), "repeats.gp", &["--bars", "9-12"]);
    assert_eq!(code, 2);
    assert_eq!(body["error"]["kind"], json!("invalid_input"));
    assert_eq!(
        body["error"]["message"],
        json!("--bars 9-12: the score has 8 bar(s), numbered from 1")
    );
    let (code, body) = failure(dir.path(), "directions-fine-ahead.gp", &["--bars", "7"]);
    assert_eq!(code, 2);
    assert_eq!(body["error"]["kind"], json!("invalid_input"));
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("--bars 7: none of these bars is ever played"),
        "{body}"
    );
    let (code, body) = failure(dir.path(), "repeats.gp", &["--bars", "0"]);
    assert_eq!(code, 1);
    assert_eq!(body["error"]["kind"], json!("usage"));
    let (code, body) = failure(dir.path(), "repeats.gp", &["--bars", "4-2"]);
    assert_eq!(code, 1);
    assert_eq!(body["error"]["kind"], json!("usage"));
}

#[test]
fn speed_scales_the_duration_and_stays_between_25_and_200() {
    let dir = temp();
    let normal = audio(dir.path(), "repeats.gp", "n.wav", &["--bars", "2"]);
    let slow = audio(
        dir.path(),
        "repeats.gp",
        "s.wav",
        &["--bars", "2", "--speed", "50"],
    );
    let fast = audio(
        dir.path(),
        "repeats.gp",
        "f.wav",
        &["--bars", "2", "--speed", "200"],
    );
    assert_eq!(normal.frames(), BAR_FRAMES + TAIL_FRAMES);
    assert_eq!(slow.frames(), 2 * BAR_FRAMES + TAIL_FRAMES);
    assert_eq!(fast.frames(), BAR_FRAMES / 2 + TAIL_FRAMES);
    assert_eq!(slow.body["speed"], json!(50));
    for edge in ["25", "200"] {
        audio(
            dir.path(),
            "repeats.gp",
            "edge.wav",
            &["--bars", "2", "--speed", edge],
        );
    }
    for bad in ["24", "201", "0", "fast"] {
        let (code, body) = failure(dir.path(), "repeats.gp", &["--speed", bad]);
        assert_eq!(code, 1, "{bad}");
        assert_eq!(body["error"]["kind"], json!("usage"));
    }
}

#[test]
fn a_count_in_adds_one_bar_before_the_music() {
    let dir = temp();
    let plain = audio(dir.path(), "repeats.gp", "p.wav", &["--bars", "2"]);
    let counted = audio(
        dir.path(),
        "repeats.gp",
        "c.wav",
        &["--bars", "2", "--count-in"],
    );
    assert_eq!(counted.frames(), plain.frames() + BAR_FRAMES);
    assert_eq!(counted.played(), json!([2]));
    let lead = &counted.wav().samples[..2 * 1000];
    assert!(lead.iter().any(|sample| *sample != 0));
}

#[test]
fn the_metronome_adds_clicks_without_changing_the_length() {
    let dir = temp();
    let plain = audio(dir.path(), "repeats.gp", "p.wav", &["--bars", "2"]);
    let clicked = audio(
        dir.path(),
        "repeats.gp",
        "c.wav",
        &["--bars", "2", "--metronome"],
    );
    assert_eq!(clicked.frames(), plain.frames());
    assert_ne!(plain.wav().samples, clicked.wav().samples);
}

#[test]
fn tracks_are_picked_by_number_from_one() {
    let dir = temp();
    let all = audio(dir.path(), "multi-track.gp", "all.wav", &[]);
    let second = audio(dir.path(), "multi-track.gp", "two.wav", &["--track", "2"]);
    let pair = audio(
        dir.path(),
        "multi-track.gp",
        "pair.wav",
        &["--track", "3", "--track", "1", "--track", "3"],
    );
    assert_eq!(all.body["tracks"], json!([1, 2, 3, 4]));
    assert_eq!(second.body["tracks"], json!([2]));
    assert_eq!(pair.body["tracks"], json!([1, 3]));
    assert_ne!(all.wav().samples, second.wav().samples);
    assert_ne!(second.wav().samples, pair.wav().samples);
    assert_eq!(all.body["backends"].as_array().unwrap().len(), 4);
    let (code, body) = failure(dir.path(), "multi-track.gp", &["--track", "5"]);
    assert_eq!(code, 2);
    assert_eq!(
        body["error"]["message"],
        json!("--track 5: the score has 4 track(s), numbered from 1")
    );
    let (code, _) = failure(dir.path(), "multi-track.gp", &["--track", "0"]);
    assert_eq!(code, 1);
}

#[test]
fn the_profile_changes_the_performance() {
    let dir = temp();
    let gp = audio(dir.path(), "tempo-ramp.gp", "gp.wav", &["--profile", "gp"]);
    let musical = audio(
        dir.path(),
        "tempo-ramp.gp",
        "m.wav",
        &["--profile", "musical"],
    );
    let default = audio(dir.path(), "tempo-ramp.gp", "d.wav", &[]);
    assert_eq!(gp.body["profile"], json!("gp"));
    assert_eq!(musical.body["profile"], json!("musical"));
    assert_ne!(gp.frames(), musical.frames());
    assert_eq!(default.frames(), musical.frames());
    assert_eq!(default.wav().samples, musical.wav().samples);
    assert_eq!(gp.body["unplayed"][0]["kind"], json!("tempo_ramp"));
    let (code, body) = failure(dir.path(), "tempo-ramp.gp", &["--profile", "fast"]);
    assert_eq!(code, 1);
    assert_eq!(body["error"]["kind"], json!("usage"));
}

#[test]
fn an_assigned_sf2_bank_sounds_and_is_reported_per_track() {
    let dir = temp();
    let plain = audio(dir.path(), "multi-track.gp", "plain.wav", &[]);
    let id = import_tiny_sf2(dir.path());
    let assigned = run_in(
        dir.path(),
        &["soundbank", "assign", "guitar", &id, "--json"],
    );
    assert_eq!(assigned.code, 0, "{}", assigned.stderr);
    let banked = audio(dir.path(), "multi-track.gp", "banked.wav", &[]);
    let backends = banked.body["backends"].as_array().unwrap();
    assert_eq!(backends.len(), 4);
    assert_eq!(backends[0]["bank"], json!(id));
    assert_eq!(backends[0]["format"], json!("sf2"));
    assert_eq!(backends[0]["fallback"], Value::Null);
    assert_ne!(plain.wav().samples, banked.wav().samples);
    assert_eq!(banked.frames(), plain.frames());
}

#[test]
fn soundbank_replaces_the_bank_of_every_track() {
    let dir = temp();
    let id = import_tiny_sf2(dir.path());
    let forced = audio(
        dir.path(),
        "multi-track.gp",
        "forced.wav",
        &["--soundbank", &id],
    );
    for backend in forced.body["backends"].as_array().unwrap() {
        assert_eq!(backend["bank"], json!(id));
        assert_eq!(backend["fallback"], Value::Null);
    }
    let plain = audio(dir.path(), "multi-track.gp", "plain.wav", &[]);
    let built_in = audio(
        dir.path(),
        "multi-track.gp",
        "built-in.wav",
        &["--soundbank", "built-in"],
    );
    assert_eq!(plain.wav().samples, built_in.wav().samples);
    assert_ne!(plain.wav().samples, forced.wav().samples);
    let (code, body) = failure(dir.path(), "multi-track.gp", &["--soundbank", "Not_A_Name"]);
    assert_eq!(code, 1);
    assert_eq!(body["error"]["kind"], json!("usage"));
}

#[test]
fn an_sfz_bank_sounds_through_soundbank() {
    let dir = temp();
    let id = import_tiny_sfz(dir.path());
    let forced = audio(
        dir.path(),
        "repeats.gp",
        "forced.wav",
        &["--soundbank", &id, "--bars", "1"],
    );
    let backend = &forced.body["backends"][0];
    assert_eq!(backend["bank"], json!(id));
    assert_eq!(backend["format"], json!("sfz"));
    assert!(forced.wav().peak > 0);
}

#[test]
fn the_json_reports_the_fallback_when_an_assigned_bank_is_removed() {
    let dir = temp();
    let id = import_tiny_sf2(dir.path());
    let assigned = run_in(dir.path(), &["soundbank", "assign", "guitar", &id]);
    assert_eq!(assigned.code, 0, "{}", assigned.stderr);
    let removed = run_in(dir.path(), &["soundbank", "remove", &id]);
    assert_eq!(removed.code, 0, "{}", removed.stderr);
    let plain = audio(dir.path(), "repeats.gp", "after.wav", &["--bars", "1"]);
    let backend = &plain.body["backends"][0];
    assert_eq!(backend["bank"], json!("built-in"));
    assert_eq!(backend["format"], json!("built_in"));
    let fallback = backend["fallback"].as_str().expect("a fallback");
    assert!(fallback.contains(&id), "{fallback}");
    assert!(fallback.contains("not installed"), "{fallback}");
    assert!(plain.wav().peak > 1000);
    let input = gp_fixture("repeats.gp");
    let out = dir.path().join("human.wav");
    let human = run_in(
        dir.path(),
        &[
            "audio",
            path_str(&input),
            "--out",
            path_str(&out),
            "--bars",
            "1",
        ],
    );
    assert_eq!(human.code, 0, "{}", human.stderr);
    assert!(human.stdout.contains("fell back"), "{}", human.stdout);
    assert!(human.stdout.contains(&id), "{}", human.stdout);
}

#[test]
fn an_unknown_soundbank_id_falls_back_and_says_so() {
    let dir = temp();
    let missing = audio(
        dir.path(),
        "repeats.gp",
        "missing.wav",
        &["--soundbank", "no-such-bank", "--bars", "1"],
    );
    let backend = &missing.body["backends"][0];
    assert_eq!(backend["bank"], json!("built-in"));
    assert!(
        backend["fallback"]
            .as_str()
            .unwrap()
            .contains("no-such-bank")
    );
}

#[test]
fn unplayed_features_are_reported_with_track_and_bar_numbers() {
    let dir = temp();
    let effects = audio(dir.path(), "beat-effects.gp", "fx.wav", &[]);
    let unplayed = effects.body["unplayed"].as_array().unwrap();
    let stroke = unplayed
        .iter()
        .find(|item| item["kind"] == json!("stroke"))
        .expect("a stroke");
    assert_eq!(stroke["track"], json!(1));
    assert_eq!(stroke["first_bar"], json!(2));
    assert_eq!(stroke["count"], json!(4));
}

#[test]
fn a_wrong_output_name_a_missing_directory_or_a_missing_score_is_an_input_error() {
    let dir = temp();
    let input = gp_fixture("repeats.gp");
    let wrong = dir.path().join("song.mid");
    let result = run_in(
        dir.path(),
        &[
            "audio",
            path_str(&input),
            "--out",
            path_str(&wrong),
            "--json",
        ],
    );
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["kind"], json!("unsupported_format"));
    let missing = dir.path().join("nowhere").join("song.wav");
    let result = run_in(
        dir.path(),
        &[
            "audio",
            path_str(&input),
            "--out",
            path_str(&missing),
            "--json",
        ],
    );
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["kind"], json!("io"));
    let gone = dir.path().join("gone.gp");
    let out = dir.path().join("song.wav");
    let result = run_in(
        dir.path(),
        &["audio", path_str(&gone), "--out", path_str(&out), "--json"],
    );
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["kind"], json!("io"));
    assert!(!out.exists());
}

#[test]
fn an_existing_wav_is_replaced() {
    let dir = temp();
    let out = dir.path().join("song.WAV");
    std::fs::write(&out, b"old").unwrap();
    let input = gp_fixture("repeats.gp");
    let result = run_in(
        dir.path(),
        &[
            "audio",
            path_str(&input),
            "--out",
            path_str(&out),
            "--bars",
            "1",
        ],
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(&std::fs::read(&out).unwrap()[..4], b"RIFF");
    assert!(result.stdout.starts_with("wrote "), "{}", result.stdout);
    assert!(
        result.stdout.contains("track 1 Guitar: built-in"),
        "{}",
        result.stdout
    );
}

#[test]
fn a_render_longer_than_the_limit_is_refused_before_it_starts() {
    let dir = temp();
    let score = dir.path().join("long.opentab.json");
    let created = run_in(dir.path(), &["new", "--out", path_str(&score)]);
    assert_eq!(created.code, 0, "{}", created.stderr);
    let mut lines = vec![json!({"op": "set_tempo", "master_bar": 0, "bpm": 1}).to_string()];
    for index in 1..8 {
        lines.push(
            json!({"op": "insert_master_bar", "index": index, "time_signature": {"numerator": 4, "denominator": 4}})
                .to_string(),
        );
    }
    let commands = dir.path().join("edits.jsonl");
    std::fs::write(&commands, lines.join("\n")).unwrap();
    let applied = run_in(
        dir.path(),
        &[
            "apply",
            path_str(&score),
            path_str(&commands),
            "--out",
            path_str(&score),
        ],
    );
    assert_eq!(applied.code, 0, "{}", applied.stderr);
    let out = dir.path().join("long.wav");
    let result = run_in(
        dir.path(),
        &["audio", path_str(&score), "--out", path_str(&out), "--json"],
    );
    assert_eq!(result.code, 2);
    let error = &result.json()["error"];
    assert_eq!(error["kind"], json!("invalid_input"));
    assert!(error["message"].as_str().unwrap().contains("20 minutes"));
    assert!(!out.exists());
}

#[test]
fn the_help_documents_what_bars_means() {
    let dir = temp();
    let result = run_in(dir.path(), &["audio", "--help"]);
    assert_eq!(result.code, 0);
    assert!(result.stdout.contains("SOURCE bars"), "{}", result.stdout);
    assert!(
        result
            .stdout
            .contains("first contiguous run of played bars"),
        "{}",
        result.stdout
    );
}
