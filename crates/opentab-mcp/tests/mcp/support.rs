use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use opentab_gp::read_gp;
use opentab_model::Score;
use serde_json::{Value, json};

pub const PROTOCOL: &str = "2025-06-18";
const HANG_GUARD: Duration = Duration::from_secs(300);

pub struct Server {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    next_id: u64,
}

pub struct ToolReply {
    pub result: Value,
}

impl Server {
    pub fn spawn() -> Server {
        Server::spawn_with(None)
    }

    pub fn spawn_with(data_dir: Option<&Path>) -> Server {
        let mut command = Command::new(env!("CARGO_BIN_EXE_opentab-mcp"));
        if let Some(dir) = data_dir {
            command.env("OPENTAB_DATA_DIR", dir);
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the server starts");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        let (sender, lines) = channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Server {
            child: Some(child),
            stdin: Some(stdin),
            lines,
            next_id: 0,
        }
    }

    pub fn start() -> Server {
        let mut server = Server::spawn();
        server.initialize();
        server
    }

    pub fn start_in(data_dir: &Path) -> Server {
        let mut server = Server::spawn_with(Some(data_dir));
        server.initialize();
        server
    }

    pub fn initialize(&mut self) {
        let init = self.request(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL,
                "capabilities": {},
                "clientInfo": {"name": "opentab-test", "version": "0"}
            }),
        );
        assert_eq!(
            init["result"]["serverInfo"]["name"], "opentab-mcp",
            "{init}"
        );
        self.send_line(
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string(),
        );
    }

    pub fn finish(mut self) -> ExitStatus {
        drop(self.stdin.take());
        self.child
            .take()
            .expect("a running server")
            .wait()
            .expect("the server exits")
    }

    pub fn send_line(&mut self, line: &str) {
        self.send_bytes(line.as_bytes());
    }

    pub fn send_bytes(&mut self, line: &[u8]) {
        let stdin = self.stdin.as_mut().expect("stdin is open");
        stdin.write_all(line).expect("the server reads stdin");
        stdin.write_all(b"\n").expect("the server reads stdin");
        stdin.flush().expect("flush");
    }

    pub fn next_line(&mut self) -> Value {
        let line = self
            .lines
            .recv_timeout(HANG_GUARD)
            .expect("the server answers and is still running");
        serde_json::from_str(&line).expect("every line the server writes is JSON")
    }

    pub fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send_line(
            &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string(),
        );
        self.response(id)
    }

    pub fn response(&mut self, id: u64) -> Value {
        loop {
            let message = self.next_line();
            if message.get("id") == Some(&json!(id)) {
                return message;
            }
            assert!(
                message.get("method").is_some(),
                "an unexpected response arrived before the answer to {id}: {message}"
            );
        }
    }

    pub fn call(&mut self, tool: &str, arguments: Value) -> ToolReply {
        let message = self.request("tools/call", json!({"name": tool, "arguments": arguments}));
        assert!(message.get("error").is_none(), "{tool}: {message}");
        ToolReply {
            result: message["result"].clone(),
        }
    }

    pub fn ok(&mut self, tool: &str, arguments: Value) -> Value {
        self.call(tool, arguments).json()
    }

    pub fn fails(&mut self, tool: &str, arguments: Value) -> Value {
        self.call(tool, arguments).error()
    }

    pub fn open(&mut self, path: &str) -> String {
        let opened = self.ok("open", json!({"path": path}));
        opened["session"].as_str().expect("a session id").to_owned()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl ToolReply {
    pub fn is_error(&self) -> bool {
        self.result["isError"] == json!(true)
    }

    pub fn text(&self, index: usize) -> Value {
        let part = &self.result["content"][index];
        assert_eq!(part["type"], "text", "{}", self.result);
        serde_json::from_str(part["text"].as_str().expect("text")).expect("the text part is JSON")
    }

    pub fn json(&self) -> Value {
        assert!(!self.is_error(), "{}", self.result);
        self.text(0)
    }

    pub fn error(&self) -> Value {
        assert!(self.is_error(), "expected a tool error: {}", self.result);
        let envelope = self.text(0);
        envelope["error"].clone()
    }

    pub fn wav(&self, index: usize) -> Vec<u8> {
        let part = &self.result["content"][index];
        assert_eq!(part["type"], "audio", "{}", self.result);
        assert_eq!(part["mimeType"], "audio/wav");
        STANDARD
            .decode(part["data"].as_str().expect("base64 data"))
            .expect("valid base64")
    }

    pub fn png(&self, index: usize) -> Vec<u8> {
        let part = &self.result["content"][index];
        assert_eq!(part["type"], "image", "{}", self.result);
        assert_eq!(part["mimeType"], "image/png");
        STANDARD
            .decode(part["data"].as_str().expect("base64 data"))
            .expect("valid base64")
    }
}

pub fn fixture(name: &str) -> String {
    fixture_path(name).display().to_string()
}

pub fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/gp")
        .join(name)
}

pub fn read_fixture(name: &str) -> Score {
    read_gp(&std::fs::read(fixture_path(name)).expect("fixture")).expect("fixture reads")
}
