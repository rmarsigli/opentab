use serde_json::{Value, json};

use crate::support::{Server, fixture};

const FIXTURE: &str = "multi-track.gp";

fn still_serves(server: &mut Server) {
    let session = server.open(&fixture(FIXTURE));
    let rendered = server.call("render", json!({"session": session, "scale": "1/4"}));
    assert!(!rendered.png(0).is_empty());
    server.ok("close", json!({"session": session}));
}

fn message(error: &Value) -> &str {
    error["message"].as_str().expect("an error message")
}

#[test]
fn an_unknown_session_is_an_invalid_input_error() {
    let mut server = Server::start();
    for tool in ["info", "undo", "redo", "close"] {
        let error = server.fails(tool, json!({"session": "session-42"}));
        assert_eq!(error["kind"], "invalid_input", "{tool}");
        assert!(message(&error).contains("unknown session \"session-42\""));
    }
    let error = server.fails(
        "export",
        json!({"session": "nope", "path": "/nonexistent/x.gp"}),
    );
    assert_eq!(error["kind"], "invalid_input");
    let error = server.fails("apply", json!({"session": "", "commands": []}));
    assert_eq!(error["kind"], "invalid_input");
    still_serves(&mut server);
}

#[test]
fn a_render_before_any_open_is_refused() {
    let mut server = Server::start();
    let error = server.fails("render", json!({"session": "session-1"}));
    assert_eq!(error["kind"], "invalid_input");
    assert!(message(&error).contains("open a score first"));
    still_serves(&mut server);
}

#[test]
fn malformed_commands_are_refused_and_nothing_is_applied() {
    let mut server = Server::start();
    let session = server.open(&fixture(FIXTURE));
    let error = server.fails(
        "apply",
        json!({"session": session, "commands": [
            {"op": "set_meta", "field": "title", "value": "kept?"},
            {"op": "no_such_op"}
        ]}),
    );
    assert_eq!(error["kind"], "invalid_command");
    assert_eq!(error["index"], 1);
    assert!(message(&error).starts_with("commands[1]: "));
    for commands in [
        json!([1]),
        json!(["set_meta"]),
        json!([{"op": "set_meta"}]),
        json!([{}]),
    ] {
        let error = server.fails("apply", json!({"session": session, "commands": commands}));
        assert_eq!(error["kind"], "invalid_command", "{commands}");
        assert_eq!(error["index"], 0);
    }
    for commands in [json!("set_meta"), json!({"op": "set_meta"}), json!(null)] {
        let error = server.fails("apply", json!({"session": session, "commands": commands}));
        assert_eq!(error["kind"], "usage", "{commands}");
    }
    let error = server.fails(
        "apply",
        json!({"session": session, "commands": [
            {"op": "set_meta", "field": "title", "value": "kept?"},
            {"op": "remove_note", "note": 424242}
        ]}),
    );
    assert_eq!(error["kind"], "command_failed");
    assert_eq!(error["sub_kind"], "not_found");
    assert_eq!(error["index"], 1);
    let info = server.ok("info", json!({"session": session}));
    assert_eq!(info["title"], "Several tracks");
    assert_eq!(info["can_undo"], false);
    let applied = server.ok(
        "apply",
        json!({"session": session, "commands": [{"op": "set_meta", "field": "title", "value": "Now"}]}),
    );
    assert_eq!(applied, json!({"applied": 1, "allocated": []}));
    still_serves(&mut server);
}

#[test]
fn page_indexes_outside_the_score_are_refused() {
    let mut server = Server::start();
    let session = server.open(&fixture(FIXTURE));
    for page in [json!(2), json!(0), json!(u32::MAX)] {
        let error = server.fails("render", json!({"session": session, "page": page}));
        assert_eq!(error["kind"], "invalid_input", "{page}");
        assert!(
            message(&error).ends_with("the score has 1 page(s), numbered from 1"),
            "{error}"
        );
    }
    for page in [
        json!(4_294_967_296_u64),
        json!(-1),
        json!(1.5),
        json!("1"),
        json!(1e300),
    ] {
        let error = server.fails("render", json!({"session": session, "page": page}));
        assert_eq!(error["kind"], "usage", "{page}");
    }
    let error = server.fails("render", json!({"session": session, "track": 5}));
    assert_eq!(error["kind"], "invalid_input");
    assert!(message(&error).starts_with("track 5: the score has 4 track(s)"));
    let error = server.fails("render", json!({"session": session, "bars": "1-3"}));
    assert_eq!(error["kind"], "invalid_input");
    let error = server.fails("render", json!({"session": session, "bars": "3-1"}));
    assert_eq!(error["kind"], "usage");
    still_serves(&mut server);
}

#[test]
fn a_scale_over_the_pixel_cap_is_refused_before_any_image_is_made() {
    let mut server = Server::start();
    let session = server.open(&fixture(FIXTURE));
    for scale in [json!(9), json!("9"), json!("17/2")] {
        let error = server.fails("render", json!({"session": session, "scale": scale}));
        assert_eq!(error["kind"], "invalid_input", "{scale}");
        assert!(
            message(&error).contains("at most 60000000 pixels"),
            "{error}"
        );
    }
    for scale in [json!("100000"), json!(1e6)] {
        let error = server.fails("render", json!({"session": session, "scale": scale}));
        assert_eq!(error["kind"], "invalid_input", "{scale}");
        assert!(
            message(&error).contains("each side must be between 1 and 16384"),
            "{error}"
        );
    }
    for scale in [
        json!(0),
        json!(-2),
        json!("0"),
        json!("1e3"),
        json!("big"),
        json!(true),
    ] {
        let error = server.fails("render", json!({"session": session, "scale": scale}));
        assert_eq!(error["kind"], "usage", "{scale}");
    }
    still_serves(&mut server);
}

#[test]
fn an_unknown_tool_is_a_json_rpc_invalid_params_error() {
    let mut server = Server::start();
    for name in [json!("delete_everything"), json!(""), json!("Open")] {
        let reply = server.request("tools/call", json!({"name": name, "arguments": {}}));
        assert_eq!(reply["error"]["code"], -32602, "{reply}");
        assert_eq!(reply["error"]["data"]["kind"], "usage");
    }
    let long = "x".repeat(10_000);
    let reply = server.request("tools/call", json!({"name": long, "arguments": {}}));
    assert!(reply["error"]["message"].as_str().unwrap().len() < 300);
    still_serves(&mut server);
}

fn replies_before_a_ping(server: &mut Server, garbage: &[u8]) -> Vec<Value> {
    server.send_bytes(garbage);
    server.send_line(&json!({"jsonrpc": "2.0", "id": "after", "method": "ping"}).to_string());
    let mut before = Vec::new();
    loop {
        let message = server.next_line();
        if message["id"] == "after" {
            assert_eq!(message["result"], json!({}), "{message}");
            return before;
        }
        before.push(message);
    }
}

fn one_error(server: &mut Server, garbage: &[u8]) -> Value {
    let replies = replies_before_a_ping(server, garbage);
    let shown = String::from_utf8_lossy(garbage)
        .chars()
        .take(80)
        .collect::<String>();
    assert_eq!(replies.len(), 1, "{shown}: {replies:?}");
    replies[0].clone()
}

#[test]
fn garbage_on_the_wire_gets_a_json_rpc_error_and_the_server_keeps_serving() {
    let mut server = Server::start();
    let nested = format!("{}{}", "[".repeat(200), "]".repeat(200));
    let nested_in_a_request = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"tools/call\",\"params\":{{\"name\":\"apply\",\"arguments\":{{\"commands\":{}{}}}}}}}",
        "[".repeat(500),
        "]".repeat(500)
    );
    let not_json: [&[u8]; 9] = [
        b"this is not json",
        b"{\"unterminated\":",
        b"\x01\x02",
        b"}{",
        b"\"just a string",
        b"\xff\xfe\xfd",
        b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\x80\"}",
        nested.as_bytes(),
        nested_in_a_request.as_bytes(),
    ];
    for garbage in not_json {
        let reply = one_error(&mut server, garbage);
        assert_eq!(reply["error"]["code"], -32700, "{reply}");
        assert_eq!(reply["id"], Value::Null);
    }
    for (garbage, id) in [
        (json!({"foo": 1}), Value::Null),
        (json!(42), Value::Null),
        (
            json!({"jsonrpc": "2.0", "id": 900, "method": 5}),
            json!(900),
        ),
        (
            json!({"jsonrpc": "1.0", "id": 901, "method": "ping"}),
            json!(901),
        ),
    ] {
        let reply = one_error(&mut server, garbage.to_string().as_bytes());
        assert_eq!(reply["error"]["code"], -32600, "{garbage}");
        assert_eq!(reply["id"], id, "{garbage}");
    }
    for batch in [
        json!([]),
        json!([{"jsonrpc": "2.0", "id": 1, "method": "ping"}]),
    ] {
        let reply = one_error(&mut server, batch.to_string().as_bytes());
        assert_eq!(reply["error"]["code"], -32600, "{batch}");
        assert!(
            reply["error"]["message"]
                .as_str()
                .unwrap()
                .contains("batches are not supported")
        );
    }
    for id in [Value::Null, json!(1.5), json!(true), json!({"a": 1})] {
        let request = json!({"jsonrpc": "2.0", "id": id, "method": "tools/list"});
        let reply = one_error(&mut server, request.to_string().as_bytes());
        assert_eq!(reply["error"]["code"], -32600, "{id}");
        assert_eq!(reply["id"], Value::Null, "{id}");
    }
    let huge_id = b"{\"jsonrpc\":\"2.0\",\"id\":99999999999999999999,\"method\":\"tools/list\"}";
    let reply = one_error(&mut server, huge_id);
    assert_eq!(reply["error"]["code"], -32600);
    assert_eq!(reply["id"], Value::Null);
    let reply = server.request("no/such/method", json!({}));
    assert_eq!(reply["error"]["code"], -32601, "{reply}");
    for params in [
        json!(5),
        json!({"name": 5}),
        json!({"name": "open", "arguments": [1, 2]}),
    ] {
        let reply = server.request("tools/call", params.clone());
        assert_eq!(reply["error"]["code"], -32602, "{params}: {reply}");
    }
    let reply = server.request("tools/call", json!({"name": "open"}));
    assert_eq!(reply["result"]["isError"], true, "{reply}");
    assert_eq!(server.call("open", json!({})).error()["kind"], "usage");
    still_serves(&mut server);
}

#[test]
fn a_line_longer_than_64_mib_is_refused_and_the_server_keeps_serving() {
    let mut server = Server::start();
    let mut long =
        b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\",\"params\":{\"x\":\"".to_vec();
    long.resize(64 * 1024 * 1024 + 16, b'x');
    let reply = one_error(&mut server, &long);
    assert_eq!(reply["error"]["code"], -32600, "{reply}");
    assert_eq!(reply["id"], Value::Null);
    assert!(
        reply["error"]["message"]
            .as_str()
            .unwrap()
            .contains("64 MiB")
    );
    still_serves(&mut server);
}

#[test]
fn requests_before_initialize_are_refused_until_the_handshake() {
    let mut server = Server::spawn();
    let early = server.request(
        "tools/call",
        json!({"name": "open", "arguments": {"path": fixture(FIXTURE)}}),
    );
    assert_eq!(early["error"]["code"], -32002, "{early}");
    let bad_initialize = server.request("initialize", json!(5));
    assert_eq!(bad_initialize["error"]["code"], -32602, "{bad_initialize}");
    server.send_line(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string());
    let replies = replies_before_a_ping(&mut server, b"garbage before the handshake");
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0]["error"]["code"], -32700);
    server.initialize();
    still_serves(&mut server);
}

#[test]
fn the_server_exits_cleanly_when_its_input_closes() {
    let mut server = Server::start();
    still_serves(&mut server);
    assert!(server.finish().success());
}

#[test]
fn open_failures_carry_the_cli_error_kinds() {
    let mut server = Server::start();
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.gp");
    let error = server.fails("open", json!({"path": missing.display().to_string()}));
    assert_eq!(error["kind"], "io");
    let error = server.fails("open", json!({"path": "song.txt"}));
    assert_eq!(error["kind"], "unsupported_format");
    let broken_gp = dir.path().join("broken.gp");
    std::fs::write(&broken_gp, b"not a zip").unwrap();
    let error = server.fails("open", json!({"path": broken_gp.display().to_string()}));
    assert_eq!(error["kind"], "invalid_input");
    for special in [
        dir.path().join("folder.gp"),
        dir.path().join("fifo.gp"),
        dir.path().join("zero.gp"),
    ] {
        let name = special.file_name().unwrap().to_string_lossy().into_owned();
        match name.as_str() {
            "folder.gp" => std::fs::create_dir(&special).unwrap(),
            "fifo.gp" => assert!(
                std::process::Command::new("mkfifo")
                    .arg(&special)
                    .status()
                    .unwrap()
                    .success()
            ),
            _ => std::os::unix::fs::symlink("/dev/zero", &special).unwrap(),
        }
        let error = server.fails("open", json!({"path": special.display().to_string()}));
        assert_eq!(error["kind"], "io", "{name}");
        assert!(message(&error).ends_with("not a regular file"), "{error}");
    }
    let broken_json = dir.path().join("broken.opentab.json");
    std::fs::write(&broken_json, b"{}").unwrap();
    let error = server.fails("open", json!({"path": broken_json.display().to_string()}));
    assert_eq!(error["kind"], "invalid_score");
    still_serves(&mut server);
}

#[test]
fn sessions_are_bounded_and_a_closed_slot_can_be_reused() {
    let mut server = Server::start();
    let sessions: Vec<String> = (0..16).map(|_| server.open(&fixture(FIXTURE))).collect();
    let error = server.fails("open", json!({"path": fixture(FIXTURE)}));
    assert_eq!(error["kind"], "invalid_input");
    assert!(message(&error).contains("at most 16 sessions"));
    server.ok("close", json!({"session": sessions[0]}));
    let reopened = server.open(&fixture(FIXTURE));
    assert!(!sessions.contains(&reopened));
    let error = server.fails("info", json!({"session": sessions[0]}));
    assert_eq!(error["kind"], "invalid_input");
}

#[test]
fn export_refuses_other_suffixes_and_writes_only_the_given_path() {
    let mut server = Server::start();
    let session = server.open(&fixture(FIXTURE));
    let dir = tempfile::tempdir().unwrap();
    for name in ["out.txt", "out.json", "out", ".gp", "out.gpx"] {
        let path = dir.path().join(name);
        let error = server.fails(
            "export",
            json!({"session": session, "path": path.display().to_string()}),
        );
        assert_eq!(error["kind"], "unsupported_format", "{name}");
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    let missing_parent = dir.path().join("no/such/dir/out.gp");
    let error = server.fails(
        "export",
        json!({"session": session, "path": missing_parent.display().to_string()}),
    );
    assert_eq!(error["kind"], "io");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    let target = dir.path().join("Out.GP");
    server.ok(
        "export",
        json!({"session": session, "path": target.display().to_string()}),
    );
    let names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["Out.GP"]);
    still_serves(&mut server);
}

#[test]
fn untrusted_strings_in_errors_are_length_bounded() {
    let mut server = Server::start();
    let long = "s".repeat(100_000);
    let error = server.fails("info", json!({"session": long}));
    assert!(message(&error).len() < 300, "{}", message(&error).len());
    let error = server.fails("open", json!({"path": format!("{long}.gp")}));
    assert!(message(&error).chars().count() <= 1_001);
    let session = server.open(&fixture(FIXTURE));
    let error = server.fails(
        "apply",
        json!({"session": session, "commands": [{"op": long}]}),
    );
    assert!(message(&error).chars().count() <= 1_001);
    still_serves(&mut server);
}
