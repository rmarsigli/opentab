use rmcp::model::{ClientJsonRpcMessage, ClientRequest, JsonRpcMessage};
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const NOT_INITIALIZED: i64 = -32002;
const BOM: &[u8] = b"\xEF\xBB\xBF";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Forward,
    Skip,
    Reply(String),
}

#[derive(Debug, Default)]
pub struct Gate {
    initialized: bool,
}

#[derive(Serialize)]
pub struct WireError {
    pub code: i64,
    pub message: &'static str,
}

#[derive(Serialize)]
pub struct WireReply {
    pub jsonrpc: &'static str,
    pub id: Value,
    pub error: WireError,
}

impl Gate {
    pub fn judge(&mut self, line: &[u8]) -> Verdict {
        let line = trimmed(line);
        if line.iter().all(u8::is_ascii_whitespace) {
            return Verdict::Skip;
        }
        match serde_json::from_slice::<ClientJsonRpcMessage>(line) {
            Ok(message) => self.admit(&message),
            Err(_) => refuse(line),
        }
    }

    fn admit(&mut self, message: &ClientJsonRpcMessage) -> Verdict {
        if self.initialized {
            return Verdict::Forward;
        }
        match message {
            JsonRpcMessage::Request(request) => match request.request {
                ClientRequest::InitializeRequest(_) => {
                    self.initialized = true;
                    Verdict::Forward
                }
                ClientRequest::PingRequest(_) => Verdict::Forward,
                _ => Verdict::Reply(reply(
                    id_value(serde_json::to_value(&request.id).ok()),
                    NOT_INITIALIZED,
                    "the server is not initialized; send initialize first",
                )),
            },
            _ => Verdict::Skip,
        }
    }
}

fn trimmed(line: &[u8]) -> &[u8] {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    line.strip_prefix(BOM).unwrap_or(line)
}

fn refuse(line: &[u8]) -> Verdict {
    let Ok(value) = serde_json::from_slice::<Value>(line) else {
        return Verdict::Reply(reply(Value::Null, PARSE_ERROR, "Parse error"));
    };
    let is_notification =
        value.get("id").is_none() && value.get("method").is_some_and(Value::is_string);
    if is_notification {
        return Verdict::Skip;
    }
    Verdict::Reply(reply(
        id_value(value.get("id").cloned()),
        INVALID_REQUEST,
        "Invalid Request",
    ))
}

fn id_value(id: Option<Value>) -> Value {
    match id {
        Some(id @ (Value::String(_) | Value::Number(_))) => id,
        _ => Value::Null,
    }
}

fn reply(id: Value, code: i64, message: &'static str) -> String {
    let reply = WireReply {
        jsonrpc: "2.0",
        id,
        error: WireError { code, message },
    };
    serde_json::to_string(&reply).unwrap_or_default()
}

pub async fn pump_in<R, W>(input: R, mut to_server: W, errors: UnboundedSender<String>)
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut input = BufReader::new(input);
    let mut gate = Gate::default();
    let mut line = Vec::new();
    loop {
        line.clear();
        match input.read_until(b'\n', &mut line).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        match gate.judge(&line) {
            Verdict::Forward => {
                let mut framed = trimmed(&line).to_vec();
                framed.push(b'\n');
                if to_server.write_all(&framed).await.is_err() || to_server.flush().await.is_err() {
                    break;
                }
            }
            Verdict::Skip => {}
            Verdict::Reply(text) => {
                if errors.send(text).is_err() {
                    break;
                }
            }
        }
    }
    let _ = to_server.shutdown().await;
}

pub async fn pump_out<R, W>(from_server: R, mut output: W, mut errors: UnboundedReceiver<String>)
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut lines = BufReader::new(from_server).lines();
    let mut server_open = true;
    let mut errors_open = true;
    while server_open || errors_open {
        let next = tokio::select! {
            biased;
            error = errors.recv(), if errors_open => match error {
                Some(text) => Some(text),
                None => {
                    errors_open = false;
                    None
                }
            },
            line = lines.next_line(), if server_open => match line {
                Ok(Some(text)) => Some(text),
                _ => {
                    server_open = false;
                    None
                }
            },
        };
        if let Some(text) = next {
            let written = output.write_all(text.as_bytes()).await;
            let ended = output.write_all(b"\n").await;
            if written.is_err() || ended.is_err() || output.flush().await.is_err() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn line(value: &Value) -> Vec<u8> {
        format!("{value}\n").into_bytes()
    }

    fn initialize() -> Value {
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": {"name": "t", "version": "0"}}})
    }

    fn error_of(verdict: Verdict) -> Value {
        match verdict {
            Verdict::Reply(text) => serde_json::from_str(&text).unwrap(),
            other => panic!("expected a reply, got {other:?}"),
        }
    }

    #[test]
    fn only_ping_and_initialize_pass_before_initialization() {
        let mut gate = Gate::default();
        let ping = json!({"jsonrpc": "2.0", "id": "p", "method": "ping"});
        assert_eq!(gate.judge(&line(&ping)), Verdict::Forward);
        let list = json!({"jsonrpc": "2.0", "id": 7, "method": "tools/list"});
        let refused = error_of(gate.judge(&line(&list)));
        assert_eq!(refused["id"], 7);
        assert_eq!(refused["error"]["code"], NOT_INITIALIZED);
        let note = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        assert_eq!(gate.judge(&line(&note)), Verdict::Skip);
        assert_eq!(gate.judge(&line(&initialize())), Verdict::Forward);
        assert_eq!(gate.judge(&line(&list)), Verdict::Forward);
        assert_eq!(gate.judge(&line(&note)), Verdict::Forward);
    }

    #[test]
    fn unparsable_lines_get_a_parse_error_and_bad_shapes_an_invalid_request() {
        let mut gate = Gate::default();
        gate.judge(&line(&initialize()));
        for garbage in [
            &b"not json\n"[..],
            b"{\"a\":\n",
            b"\x01\x02\n",
            b"\xff\xfe\n",
        ] {
            let error = error_of(gate.judge(garbage));
            assert_eq!(error["error"]["code"], PARSE_ERROR);
            assert_eq!(error["id"], Value::Null);
            assert_eq!(error["jsonrpc"], "2.0");
        }
        for (shape, id) in [
            (json!([]), Value::Null),
            (json!(42), Value::Null),
            (json!({"foo": 1}), Value::Null),
            (json!({"jsonrpc": "2.0", "id": 9, "method": 5}), json!(9)),
            (
                json!({"jsonrpc": "2.0", "id": "x", "method": 5}),
                json!("x"),
            ),
            (
                json!({"jsonrpc": "2.0", "id": {}, "method": 5}),
                Value::Null,
            ),
        ] {
            let verdict = gate.judge(&line(&shape));
            assert!(matches!(verdict, Verdict::Reply(_)), "{shape}: {verdict:?}");
            let error = error_of(verdict);
            assert_eq!(error["error"]["code"], INVALID_REQUEST, "{shape}");
            assert_eq!(error["id"], id, "{shape}");
        }
        let odd_note = json!({"jsonrpc": "2.0", "method": "$/cancelRequest", "params": 3});
        assert_eq!(gate.judge(&line(&odd_note)), Verdict::Skip);
        assert_eq!(gate.judge(b"   \r\n"), Verdict::Skip);
        let mut with_bom = BOM.to_vec();
        with_bom.extend(line(&json!({"jsonrpc": "2.0", "id": 3, "method": "ping"})));
        assert_eq!(gate.judge(&with_bom), Verdict::Forward);
    }
}
