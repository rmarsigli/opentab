use opentab_command::{Command, MetaField, apply};
use opentab_gp::{read_gp, write_gp};
use opentab_layout::{A4, DEFAULT_STAFF_SPACE, LayoutOptions, Page, layout};
use opentab_model::{BeatId, Duration, ElementId, NoteValue, Score, TimeSignature, from_json};
use opentab_render::{Ratio, pixel_size};
use resvg::tiny_skia::Pixmap;
use serde_json::{Value, json};

use crate::support::{Server, fixture, read_fixture};

const FIXTURE: &str = "multi-voice.gp";

fn quarter() -> Duration {
    Duration::new(NoteValue::Quarter, 0, None).unwrap()
}

fn first_voice(score: &Score) -> opentab_model::VoiceId {
    score.tracks[0].staves[0].bars[0].voices[0].id
}

fn commands(list: &[Command]) -> Value {
    Value::Array(
        list.iter()
            .map(|command| serde_json::to_value(command).unwrap())
            .collect(),
    )
}

fn normalised(score: &Score) -> Score {
    read_gp(&write_gp(score).expect("the score writes")).expect("the score reads back")
}

fn decoded(png: &[u8]) -> Pixmap {
    Pixmap::decode_png(png).expect("render returns a valid PNG")
}

#[test]
fn a_session_opens_renders_edits_undoes_and_exports_a_gp_that_reads_back_equal() {
    let original = read_fixture(FIXTURE);
    let mut server = Server::start();
    let listed = server.request("tools/list", json!({}));
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "open",
            "info",
            "render",
            "apply",
            "undo",
            "redo",
            "export",
            "close",
            "export_midi",
            "render_audio",
            "play",
            "stop",
            "status",
            "soundbanks",
            "import_soundbank",
            "install_soundbank",
            "assign_soundbank"
        ]
    );
    for tool in listed["result"]["tools"].as_array().unwrap() {
        assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
    }

    let opened = server.ok("open", json!({"path": fixture(FIXTURE)}));
    let session = opened["session"].as_str().unwrap().to_owned();
    assert_eq!(opened["title"], json!(original.meta.title));
    assert_eq!(opened["bars"], json!(original.master_bars.len()));
    assert_eq!(opened["tracks"][0]["number"], 1);
    assert_eq!(opened["tracks"][0]["name"], json!(original.tracks[0].name));
    assert_eq!(
        opened["tracks"].as_array().unwrap().len(),
        original.tracks.len()
    );

    let info = server.ok("info", json!({"session": session}));
    assert_eq!(info["session"], json!(session));
    assert_eq!(info["master_bars"], json!(original.master_bars.len()));
    assert_eq!(info["can_undo"], false);
    let voice = first_voice(&original);
    assert_eq!(
        info["ids"]["tracks"][0]["staves"][0]["bars"][0]["voices"][0]["id"],
        json!(voice.0.0)
    );

    let rendered = server.call("render", json!({"session": session}));
    let image = decoded(&rendered.png(0));
    let expected_list = layout(&original, &LayoutOptions::default()).unwrap();
    let expected_size = pixel_size(&expected_list.pages[0], Ratio::new(4, 3)).unwrap();
    assert_eq!(
        (image.width(), image.height()),
        (expected_size.width, expected_size.height)
    );
    assert_eq!((image.width(), image.height()), (1058, 1497));
    assert!(image.pixels().iter().any(|pixel| pixel.red() < 128));
    let report = rendered.text(1);
    assert_eq!(report["pages"], json!(expected_list.pages.len()));
    assert_eq!(report["page"], 1);
    assert_eq!(report["scale"], "4/3");
    assert_eq!(report["width"], 1058);
    assert_eq!(report["height"], 1497);
    for field in [
        "systems",
        "bars",
        "staff_space",
        "crowded",
        "dropped",
        "title_page",
        "unrendered",
    ] {
        assert!(report.get(field).is_some(), "{field}: {report}");
    }

    let insert = Command::InsertBeat {
        voice: Some(voice),
        bar: None,
        index: 0,
        duration: quarter(),
    };
    let first = server.ok(
        "apply",
        json!({"session": session, "commands": commands(std::slice::from_ref(&insert))}),
    );
    assert_eq!(first["applied"], 1);
    let mut expected = original.clone();
    apply(&mut expected, &insert).unwrap();
    let new_beat = expected.tracks[0].staves[0].bars[0].voices[0].beats[0].id;
    assert_eq!(first["allocated"], json!([new_beat.0.0]));

    let beat = BeatId(ElementId(first["allocated"][0].as_u64().unwrap()));
    let second_batch = [
        Command::AddNote {
            beat,
            string: 0,
            fret: 5,
        },
        Command::SetMeta {
            field: MetaField::Title,
            value: "Changed by the agent".to_owned(),
        },
    ];
    let second = server.ok(
        "apply",
        json!({"session": session, "commands": commands(&second_batch)}),
    );
    assert_eq!(second["applied"], 2);
    assert_eq!(second["allocated"].as_array().unwrap().len(), 1);

    let undone = server.ok("undo", json!({"session": session}));
    assert_eq!(undone, json!({"undone": true, "commands": 2}));
    let info = server.ok("info", json!({"session": session}));
    assert_eq!(info["can_undo"], true);
    assert_eq!(info["can_redo"], true);

    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("edited.gp");
    let exported = server.ok(
        "export",
        json!({"session": session, "path": out.display().to_string()}),
    );
    assert_eq!(exported["format"], "gp");
    let reread = read_gp(&std::fs::read(&out).unwrap()).unwrap();
    assert_eq!(reread, normalised(&expected));
    assert_ne!(reread, normalised(&original));
    assert_eq!(reread.meta.title, original.meta.title);
    let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
    assert_eq!(entries.len(), 1);

    let redone = server.ok("redo", json!({"session": session}));
    assert_eq!(redone, json!({"redone": true, "commands": 2}));
    let json_out = dir.path().join("edited.opentab.json");
    let exported = server.ok(
        "export",
        json!({"session": session, "path": json_out.display().to_string()}),
    );
    assert_eq!(exported["format"], "opentab.json");
    let mut both = expected.clone();
    for command in &second_batch {
        apply(&mut both, command).unwrap();
    }
    let from_json_file = from_json(&std::fs::read_to_string(&json_out).unwrap()).unwrap();
    assert_eq!(from_json_file, both);

    let closed = server.ok("close", json!({"session": session}));
    assert_eq!(closed, json!({"closed": session}));
    let after = server.fails("info", json!({"session": session}));
    assert_eq!(after["kind"], "invalid_input");
}

#[test]
fn later_pages_render_once_edits_make_the_score_longer() {
    let mut server = Server::start();
    let session = server.open(&fixture("signatures.gp"));
    let growth: Vec<Command> = (0..200)
        .map(|_| Command::InsertMasterBar {
            index: 0,
            time_signature: TimeSignature::new(4, 4).unwrap(),
        })
        .collect();
    let applied = server.ok(
        "apply",
        json!({"session": session, "commands": commands(&growth)}),
    );
    assert_eq!(applied["applied"], 200);
    let first = server.call("render", json!({"session": session}));
    let pages = first.text(1)["pages"].as_u64().unwrap();
    assert!(pages > 1, "{pages}");
    let last = server.call(
        "render",
        json!({"session": session, "page": pages, "scale": 1}),
    );
    let image = decoded(&last.png(0));
    assert_eq!((image.width(), image.height()), (794, 1123));
    assert_eq!(last.text(1)["page"], json!(pages));
    assert_ne!(last.png(0), first.png(0));
    let one_track = server.call(
        "render",
        json!({"session": session, "track": [1], "staff": "standard", "scale": 1}),
    );
    assert!(one_track.text(1)["pages"].as_u64().unwrap() >= 1);
    assert!(!one_track.png(0).is_empty());
    let range = server.call(
        "render",
        json!({"session": session, "bars": "1-2", "scale": "1/2"}),
    );
    assert_eq!(range.text(1)["pages"], 1);
    assert_eq!(range.text(1)["bars"], 2);
    let image = decoded(&range.png(0));
    let half = pixel_size(&Page::new(A4, DEFAULT_STAFF_SPACE), Ratio::new(1, 2)).unwrap();
    assert_eq!((image.width(), image.height()), (half.width, half.height));
    assert_eq!((half.width, half.height), (397, 561));
}

#[test]
fn several_sessions_stay_independent() {
    let mut server = Server::start();
    let one = server.open(&fixture(FIXTURE));
    let two = server.open(&fixture(FIXTURE));
    assert_ne!(one, two);
    let title = Command::SetMeta {
        field: MetaField::Title,
        value: "Only in one".to_owned(),
    };
    server.ok(
        "apply",
        json!({"session": one, "commands": commands(&[title])}),
    );
    assert_eq!(
        server.ok("info", json!({"session": one}))["title"],
        "Only in one"
    );
    let untouched = server.ok("info", json!({"session": two}));
    assert_eq!(untouched["title"], json!(read_fixture(FIXTURE).meta.title));
    assert_eq!(untouched["can_undo"], false);
}

#[test]
fn exporting_onto_the_source_replaces_it_atomically_and_reads_back() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("song.gp");
    std::fs::copy(crate::support::fixture_path(FIXTURE), &source).unwrap();
    #[cfg(unix)]
    {
        let mut permissions = std::fs::metadata(&source).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&source, permissions).unwrap();
    }
    let mut server = Server::start();
    let session = server.open(&source.display().to_string());
    let title = Command::SetMeta {
        field: MetaField::Title,
        value: "Saved over the source".to_owned(),
    };
    server.ok(
        "apply",
        json!({"session": session, "commands": commands(std::slice::from_ref(&title))}),
    );
    let exported = server.ok(
        "export",
        json!({"session": session, "path": source.display().to_string()}),
    );
    assert_eq!(exported["format"], "gp");
    let mut expected = read_fixture(FIXTURE);
    apply(&mut expected, &title).unwrap();
    let reread = read_gp(&std::fs::read(&source).unwrap()).unwrap();
    assert_eq!(reread, normalised(&expected));
    let names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["song.gp"]);
    let again = server.open(&source.display().to_string());
    assert_eq!(
        server.ok("info", json!({"session": again}))["title"],
        "Saved over the source"
    );
}
