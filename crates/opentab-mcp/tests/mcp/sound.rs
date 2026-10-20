use std::io::Cursor;
use std::path::Path;

use hound::{SampleFormat, WavReader};
use midly::{Smf, Timing};
use opentab_testkit::banks::tiny_sf2;
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::support::{Server, ToolReply, fixture};

const REPEATS: &str = "repeats.gp";
const RATE: u32 = 48_000;
const BAR_FRAMES: u64 = 2 * RATE as u64;
const TAIL_FRAMES: u64 = 3 * RATE as u64;

struct Wav {
    channels: u16,
    rate: u32,
    bits: u16,
    float: bool,
    frames: u64,
    peak: i32,
}

fn read_wav(bytes: &[u8]) -> Wav {
    let mut reader = WavReader::new(Cursor::new(bytes)).expect("a valid WAV");
    let spec = reader.spec();
    let peak = reader
        .samples::<i16>()
        .map(|sample| i32::from(sample.expect("a sample")).abs())
        .max()
        .unwrap_or(0);
    Wav {
        channels: spec.channels,
        rate: spec.sample_rate,
        bits: spec.bits_per_sample,
        float: spec.sample_format == SampleFormat::Float,
        frames: u64::from(reader.duration()),
        peak,
    }
}

fn started() -> (TempDir, Server) {
    let data = TempDir::new().expect("a temp dir");
    let server = Server::start_in(data.path());
    (data, server)
}

fn rendered(server: &mut Server, arguments: Value) -> (Wav, Value) {
    let reply = server.call("render_audio", arguments);
    assert!(!reply.is_error(), "{}", reply.result);
    assert_eq!(reply.result["content"].as_array().unwrap().len(), 2);
    let wav = read_wav(&reply.wav(0));
    (wav, reply.text(1))
}

fn failed_render(server: &mut Server, arguments: Value) -> Value {
    server.fails("render_audio", arguments)
}

fn message(error: &Value) -> &str {
    error["message"].as_str().expect("an error message")
}

fn numbers(value: &Value) -> Vec<u64> {
    value
        .as_array()
        .expect("an array")
        .iter()
        .map(|item| item.as_u64().expect("a number"))
        .collect()
}

#[test]
fn render_audio_returns_a_stereo_wav_as_audio_content_of_the_exact_length() {
    let (_data, mut server) = started();
    let session = server.open(&fixture(REPEATS));
    let (wav, report) = rendered(&mut server, json!({"session": session, "bars": "8"}));
    assert_eq!((wav.channels, wav.rate, wav.bits), (2, RATE, 16));
    assert!(!wav.float);
    assert_eq!(wav.frames, 2 * BAR_FRAMES + TAIL_FRAMES);
    assert!(wav.peak > 0);
    assert_eq!(report["frames"], json!(wav.frames));
    assert_eq!(report["sample_rate"], json!(RATE));
    assert_eq!(report["seconds"], json!(7.0));
    assert_eq!(report["profile"], "musical");
    assert_eq!(report["speed"], 100);
    assert_eq!(report["bars"], json!({"first": 8, "last": 8}));
    assert_eq!(report["backends"][0]["format"], "built_in");
    assert_eq!(report["backends"][0]["fallback"], Value::Null);
    assert!(report["bytes"].as_u64().unwrap() > wav.frames * 4);
}

#[test]
fn bars_are_source_bars_and_follow_the_repeats() {
    let (_data, mut server) = started();
    let session = server.open(&fixture(REPEATS));
    let (wav, report) = rendered(&mut server, json!({"session": session, "bars": "8"}));
    assert_eq!(numbers(&report["played_bars"]), [8, 8]);
    assert_eq!(wav.frames, 2 * BAR_FRAMES + TAIL_FRAMES);
    let (wav, report) = rendered(&mut server, json!({"session": session, "bars": "3-4"}));
    assert_eq!(numbers(&report["played_bars"]), [3, 4, 3, 4, 3]);
    assert_eq!(wav.frames, 5 * BAR_FRAMES + TAIL_FRAMES);
    let (wav, report) = rendered(&mut server, json!({"session": session, "bars": 1}));
    assert_eq!(numbers(&report["played_bars"]), [1]);
    assert_eq!(wav.frames, BAR_FRAMES + TAIL_FRAMES);
}

#[test]
fn a_whole_score_matches_the_midi_export_length_and_a_cut_range_is_silent_after_its_tail() {
    let (data, mut server) = started();
    let session = server.open(&fixture(REPEATS));
    let (whole, report) = rendered(&mut server, json!({"session": session}));
    let path = data.path().join("whole.mid");
    let exported = server.ok(
        "export_midi",
        json!({"session": session, "path": path.display().to_string()}),
    );
    let music = (exported["seconds"].as_f64().unwrap() * f64::from(RATE)).round() as u64;
    assert_eq!(whole.frames, music + TAIL_FRAMES);
    assert_eq!(
        report["played_bars"].as_array().unwrap().len() as u64 * BAR_FRAMES,
        music
    );
    assert_eq!(report["bars"], Value::Null);

    let reply = server.call("render_audio", json!({"session": session, "bars": "1"}));
    let bytes = reply.wav(0);
    let mut reader = WavReader::new(Cursor::new(bytes)).unwrap();
    let samples: Vec<i16> = reader.samples::<i16>().map(Result::unwrap).collect();
    let last = samples.len() - 2;
    assert_eq!(&samples[last..], [0, 0]);
}

#[test]
fn tracks_and_speed_shape_the_render() {
    let (_data, mut server) = started();
    let session = server.open(&fixture("multi-track.gp"));
    let opened = server.ok("info", json!({"session": session}));
    let count = opened["tracks"].as_array().unwrap().len() as u64;
    assert!(count >= 2);
    let (_, all) = rendered(&mut server, json!({"session": session, "bars": "1"}));
    assert_eq!(numbers(&all["tracks"]), (1..=count).collect::<Vec<_>>());
    let (_, one) = rendered(
        &mut server,
        json!({"session": session, "bars": "1", "tracks": [2, 2]}),
    );
    assert_eq!(numbers(&one["tracks"]), [2]);
    let (_, last) = rendered(
        &mut server,
        json!({"session": session, "bars": "1", "tracks": 1}),
    );
    assert_eq!(numbers(&last["tracks"]), [1]);

    let everything: Vec<u64> = (1..=count).collect();
    let audio_of = |server: &mut Server, tracks: Value| {
        server
            .call(
                "render_audio",
                json!({"session": session, "bars": "1", "tracks": tracks}),
            )
            .wav(0)
    };
    let default = server
        .call("render_audio", json!({"session": session, "bars": "1"}))
        .wav(0);
    assert_eq!(audio_of(&mut server, json!(everything)), default);
    let second = audio_of(&mut server, json!([2]));
    assert_ne!(second, default);
    assert_ne!(second, audio_of(&mut server, json!([1])));
    assert_eq!(second.len(), default.len());

    let (normal, _) = rendered(&mut server, json!({"session": session, "bars": "1"}));
    let (slow, report) = rendered(
        &mut server,
        json!({"session": session, "bars": "1", "speed": 50}),
    );
    assert_eq!(report["speed"], 50);
    assert_eq!(slow.frames - TAIL_FRAMES, 2 * (normal.frames - TAIL_FRAMES));
}

#[test]
fn a_render_over_sixty_seconds_is_refused_and_one_inside_the_cap_is_not() {
    let (_data, mut server) = started();
    let session = server.open(&fixture(REPEATS));
    let error = failed_render(&mut server, json!({"session": session, "speed": 25}));
    assert_eq!(error["kind"], "invalid_input");
    assert!(message(&error).contains("limited to 60 seconds"), "{error}");
    assert!(message(&error).contains("bars"));
    let error = failed_render(
        &mut server,
        json!({"session": session, "bars": "1-8", "speed": 25}),
    );
    assert_eq!(error["kind"], "invalid_input");
    let (wav, _) = rendered(
        &mut server,
        json!({"session": session, "bars": "1", "speed": 25}),
    );
    assert_eq!(wav.frames, 4 * BAR_FRAMES + TAIL_FRAMES);
    assert!(wav.frames <= 60 * u64::from(RATE));
    let again = server.ok("info", json!({"session": session}));
    assert_eq!(again["session"], json!(session));
}

#[test]
fn bad_selections_are_invalid_input_and_the_server_keeps_serving() {
    let (_data, mut server) = started();
    let session = server.open(&fixture(REPEATS));
    for (arguments, phrase) in [
        (json!({"bars": "99"}), "99"),
        (json!({"bars": "1-99"}), "1-99"),
        (json!({"tracks": 99}), "track 99"),
        (json!({"tracks": [0]}), "track 0"),
        (json!({"speed": 24}), "speed"),
        (json!({"speed": 201}), "speed"),
    ] {
        let mut arguments = arguments;
        arguments["session"] = json!(session);
        let error = failed_render(&mut server, arguments.clone());
        assert_eq!(error["kind"], "invalid_input", "{arguments}");
        assert!(message(&error).contains(phrase), "{arguments}: {error}");
    }
    let error = failed_render(&mut server, json!({"session": session, "bars": "5-"}));
    assert_eq!(error["kind"], "usage");
    let error = failed_render(&mut server, json!({"session": session, "bars": "3-1"}));
    assert_eq!(error["kind"], "usage");
    let error = failed_render(&mut server, json!({"session": "session-9"}));
    assert_eq!(error["kind"], "invalid_input");
    let error = failed_render(&mut server, json!({"session": session, "loud": true}));
    assert_eq!(error["kind"], "usage");
    let error = failed_render(&mut server, json!({"session": session, "profile": "loud"}));
    assert_eq!(error["kind"], "usage");
    let (wav, _) = rendered(&mut server, json!({"session": session, "bars": "1"}));
    assert_eq!(wav.frames, BAR_FRAMES + TAIL_FRAMES);
}

#[test]
fn bars_that_are_never_played_are_refused() {
    let (_data, mut server) = started();
    let session = server.open(&fixture("directions-fine-ahead.gp"));
    let error = failed_render(&mut server, json!({"session": session, "bars": "7"}));
    assert_eq!(error["kind"], "invalid_input");
    assert!(message(&error).contains("ever played"), "{error}");
}

#[test]
fn the_profile_changes_the_performance() {
    let (_data, mut server) = started();
    let session = server.open(&fixture("tempo-ramp.gp"));
    let (_, musical) = rendered(
        &mut server,
        json!({"session": session, "profile": "musical"}),
    );
    let (_, gp) = rendered(&mut server, json!({"session": session, "profile": "gp"}));
    assert_eq!(musical["profile"], "musical");
    assert_eq!(gp["profile"], "gp");
    assert_ne!(musical["frames"], gp["frames"]);
}

fn metrical(ppq: &Value) -> Timing {
    let ticks = u16::try_from(ppq.as_u64().expect("a ppq")).expect("a small ppq");
    Timing::Metrical(midly::num::u15::new(ticks))
}

fn parsed(path: &Path) -> (Timing, usize) {
    let bytes = std::fs::read(path).expect("the MIDI file exists");
    let smf = Smf::parse(&bytes).expect("a valid Standard MIDI File");
    (smf.header.timing, smf.tracks.len())
}

#[test]
fn export_midi_writes_a_file_that_reparses_with_the_profile_timing() {
    let (data, mut server) = started();
    let session = server.open(&fixture("tempo-ramp.gp"));
    let musical = data.path().join("musical.mid");
    let gp = data.path().join("gp.mid");
    let default = server.ok(
        "export_midi",
        json!({"session": session, "path": musical.display().to_string()}),
    );
    let explicit = server.ok(
        "export_midi",
        json!({"session": session, "path": gp.display().to_string(), "profile": "gp"}),
    );
    assert_eq!(default["profile"], "musical");
    assert_eq!(explicit["profile"], "gp");
    let (musical_timing, musical_tracks) = parsed(&musical);
    let (gp_timing, gp_tracks) = parsed(&gp);
    assert_eq!(musical_timing, metrical(&default["ppq"]));
    assert_eq!(gp_timing, metrical(&explicit["ppq"]));
    assert_ne!(default["ppq"], explicit["ppq"]);
    assert!(musical_tracks >= 2 && gp_tracks >= 2);
    assert_eq!(
        default["bytes"].as_u64().unwrap(),
        std::fs::metadata(&musical).unwrap().len()
    );
    assert!(default["seconds"].as_f64().unwrap() > 0.0);
    assert!(default["played_bars"].as_array().unwrap().len() > 1);
    assert_eq!(default["exported"], json!(musical.display().to_string()));
}

#[test]
fn export_midi_replaces_a_file_and_refuses_bad_targets() {
    let (data, mut server) = started();
    let session = server.open(&fixture(REPEATS));
    let path = data.path().join("song.mid");
    std::fs::write(&path, b"old").unwrap();
    server.ok(
        "export_midi",
        json!({"session": session, "path": path.display().to_string()}),
    );
    assert!(std::fs::read(&path).unwrap().starts_with(b"MThd"));
    for bad in ["song.wav", "song.midi", "song.gp", ".mid"] {
        let target = data.path().join(bad);
        let error = server.fails(
            "export_midi",
            json!({"session": session, "path": target.display().to_string()}),
        );
        assert_eq!(error["kind"], "unsupported_format", "{bad}");
        assert!(!target.exists(), "{bad}");
    }
    let missing = data.path().join("nowhere").join("song.mid");
    let error = server.fails(
        "export_midi",
        json!({"session": session, "path": missing.display().to_string()}),
    );
    assert_eq!(error["kind"], "io");
    let error = server.fails(
        "export_midi",
        json!({"session": "session-9", "path": path.display().to_string()}),
    );
    assert_eq!(error["kind"], "invalid_input");
    let error = server.fails(
        "export_midi",
        json!({"session": session, "path": path.display().to_string(), "profile": "x"}),
    );
    assert_eq!(error["kind"], "usage");
}

#[test]
fn export_midi_and_render_audio_use_the_edited_score() {
    let (data, mut server) = started();
    let session = server.open(&fixture(REPEATS));
    let before = server.ok("info", json!({"session": session}));
    let bars = before["master_bars"].as_u64().unwrap();
    let (_, original) = rendered(&mut server, json!({"session": session}));
    server.ok(
        "apply",
        json!({"session": session, "commands": [{"op": "remove_master_bar", "index": bars - 1}]}),
    );
    let (_, edited) = rendered(&mut server, json!({"session": session}));
    assert!(edited["frames"].as_u64().unwrap() < original["frames"].as_u64().unwrap());
    let path = data.path().join("edited.mid");
    let exported = server.ok(
        "export_midi",
        json!({"session": session, "path": path.display().to_string()}),
    );
    assert!(
        exported["played_bars"].as_array().unwrap().len()
            < original["played_bars"].as_array().unwrap().len()
    );
}

fn status(server: &mut Server) -> Value {
    server.ok("status", json!({}))
}

#[cfg(not(feature = "live"))]
#[test]
fn play_status_stop_move_through_their_states_with_a_frame_that_never_decreases() {
    let (_data, mut server) = started();
    let session = server.open(&fixture(REPEATS));
    let idle = status(&mut server);
    assert_eq!(idle["state"], "idle");
    assert_eq!(idle["frame"], Value::Null);
    let stopped = server.ok("stop", json!({}));
    assert_eq!(stopped["state"], "idle");

    let playing = server.ok("play", json!({"session": session, "speed": 25}));
    assert_eq!(playing["state"], "playing");
    assert_eq!(playing["session"], json!(session));
    assert_eq!(playing["played_bar"], 1);
    assert_eq!(playing["source_bar"], 1);
    assert_eq!(playing["frame"], 0);
    assert_eq!(playing["speed"], 25);
    assert_eq!(playing["backends"][0]["format"], "built_in");
    let mut frame = 0;
    for _ in 0..6 {
        let now = status(&mut server);
        assert_eq!(now["state"], "playing");
        assert_eq!(now["session"], json!(session));
        let next = now["frame"].as_u64().unwrap();
        assert!(next >= frame, "{now}");
        frame = next;
        assert!(now["played_bar"].as_u64().unwrap() >= 1);
        assert!(now["offset"].is_string());
    }

    let stopped = server.ok("stop", json!({}));
    assert_eq!(stopped["state"], "stopped");
    assert_eq!(stopped["session"], json!(session));
    assert!(stopped["frame"].as_u64().unwrap() >= frame);
    assert_eq!(status(&mut server)["state"], "idle");
    assert_eq!(server.ok("stop", json!({}))["state"], "idle");

    let again = server.ok("play", json!({"session": session, "speed": 25}));
    assert_eq!(again["state"], "playing");
    assert_eq!(again["frame"], 0);
    assert_eq!(status(&mut server)["state"], "playing");
    let finished = server.ok("stop", json!({}));
    assert_eq!(finished["state"], "stopped");
    assert_eq!(status(&mut server)["state"], "idle");
}

#[cfg(not(feature = "live"))]
#[test]
fn play_starts_at_the_source_bar_and_a_second_play_replaces_the_first() {
    let (_data, mut server) = started();
    let session = server.open(&fixture(REPEATS));
    let other = server.open(&fixture("multi-track.gp"));
    let playing = server.ok(
        "play",
        json!({"session": session, "bars": "8", "speed": 25}),
    );
    assert_eq!(playing["state"], "playing");
    assert_eq!(playing["source_bar"], 8);
    assert_eq!(playing["bars"], json!({"first": 8, "last": 8}));
    assert_eq!(status(&mut server)["source_bar"], 8);
    let replaced = server.ok("play", json!({"session": other, "speed": 25}));
    assert_eq!(replaced["session"], json!(other));
    assert_eq!(replaced["source_bar"], 1);
    assert_eq!(status(&mut server)["session"], json!(other));
    let error = server.fails("play", json!({"session": other, "bars": "99"}));
    assert_eq!(error["kind"], "invalid_input");
    assert_eq!(status(&mut server)["state"], "idle");

    let error = server.fails("play", json!({"session": "session-9"}));
    assert_eq!(error["kind"], "invalid_input");
    let error = server.fails("play", json!({"session": session, "bars": "99"}));
    assert_eq!(error["kind"], "invalid_input");
    let error = server.fails("play", json!({"session": session, "speed": 300}));
    assert_eq!(error["kind"], "invalid_input");
    assert_eq!(status(&mut server)["state"], "idle");
}

#[test]
fn stop_and_status_take_no_arguments() {
    let (_data, mut server) = started();
    for tool in ["stop", "status", "soundbanks"] {
        let error = server.fails(tool, json!({"x": 1}));
        assert_eq!(error["kind"], "usage", "{tool}");
    }
}

fn sf2_file(dir: &Path) -> String {
    let path = dir.join("tiny.sf2");
    std::fs::write(&path, tiny_sf2()).unwrap();
    path.display().to_string()
}

fn banks(reply: &ToolReply) -> Value {
    reply.json()
}

#[test]
fn soundbanks_start_empty_with_the_catalogue_listed() {
    let (_data, mut server) = started();
    let listing = server.ok("soundbanks", json!({}));
    assert_eq!(listing["banks"], json!([]));
    assert_eq!(listing["assignments"], json!({}));
    assert_eq!(listing["problems"], json!([]));
    let catalogue = listing["catalogue"].as_array().unwrap();
    assert_eq!(catalogue.len(), 9);
    for entry in catalogue {
        assert_eq!(entry["installed"], false);
        assert!(entry["id"].as_str().unwrap().len() > 2);
        assert!(entry["size"].as_u64().unwrap() > 0);
    }
}

#[test]
fn an_imported_bank_is_listed_assigned_and_used_by_render_audio() {
    let (data, mut server) = started();
    let session = server.open(&fixture(REPEATS));
    let imported = server.ok("import_soundbank", json!({"path": sf2_file(data.path())}));
    let id = imported["imported"]["id"].as_str().unwrap().to_owned();
    assert_eq!(imported["imported"]["format"], "sf2");
    assert_eq!(imported["imported"]["source"], "imported");
    let listing = server.ok("soundbanks", json!({}));
    assert_eq!(listing["banks"][0]["id"], json!(id));

    let assigned = server.ok(
        "assign_soundbank",
        json!({"instrument": "Guitar", "bank": id}),
    );
    assert_eq!(
        assigned["assigned"],
        json!({"instrument": "guitar", "bank": id})
    );
    let listing = server.ok("soundbanks", json!({}));
    assert_eq!(listing["assignments"], json!({"guitar": id}));
    let (_, report) = rendered(&mut server, json!({"session": session, "bars": "1"}));
    let formats: Vec<&str> = report["backends"]
        .as_array()
        .unwrap()
        .iter()
        .map(|backend| backend["format"].as_str().unwrap())
        .collect();
    assert!(formats.contains(&"sf2"), "{report}");
    assert_eq!(report["backends"][0]["bank"], json!(id));

    server.ok(
        "assign_soundbank",
        json!({"instrument": "guitar", "bank": "built-in"}),
    );
    let (_, report) = rendered(&mut server, json!({"session": session, "bars": "1"}));
    assert_eq!(report["backends"][0]["format"], "built_in");
}

#[test]
fn soundbank_misuse_is_a_clear_error_and_changes_nothing() {
    let (data, mut server) = started();
    let junk = data.path().join("junk.sf2");
    std::fs::write(&junk, b"not a bank").unwrap();
    let error = server.fails(
        "import_soundbank",
        json!({"path": junk.display().to_string()}),
    );
    assert_eq!(error["kind"], "sound_bank");
    let error = server.fails(
        "import_soundbank",
        json!({"path": data.path().join("missing.sf2").display().to_string()}),
    );
    assert_eq!(error["kind"], "sound_bank");
    let error = server.fails("install_soundbank", json!({"id": "no-such-bank"}));
    assert_eq!(error["kind"], "sound_bank");
    let error = server.fails("install_soundbank", json!({"id": "Not A Valid Id"}));
    assert_eq!(error["kind"], "sound_bank");
    let error = server.fails(
        "assign_soundbank",
        json!({"instrument": "banjo", "bank": "built-in"}),
    );
    assert_eq!(error["kind"], "invalid_input");
    assert!(message(&error).contains("\"banjo\""));
    let error = server.fails(
        "assign_soundbank",
        json!({"instrument": "guitar", "bank": "not-installed"}),
    );
    assert_eq!(error["kind"], "sound_bank");
    let listing = banks(&server.call("soundbanks", json!({})));
    assert_eq!(listing["banks"], json!([]));
    assert_eq!(listing["assignments"], json!({}));
}
