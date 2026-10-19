use rmcp::model::{ClientJsonRpcMessage, ClientRequest, JsonRpcMessage};
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const INVALID_PARAMS: i64 = -32602;
pub const NOT_INITIALIZED: i64 = -32002;
pub const MAX_LINE_BYTES: usize = 64 * 1024 * 1024;
pub const READ_BUFFER_BYTES: usize = 1 << 16;
const BOM: &[u8] = b"\xEF\xBB\xBF";
const KNOWN_REQUESTS: [&str; 15] = [
    "initialize",
    "ping",
    "server/discover",
    "completion/complete",
    "logging/setLevel",
    "prompts/get",
    "prompts/list",
    "resources/list",
    "resources/templates/list",
    "resources/read",
    "resources/subscribe",
    "resources/unsubscribe",
    "subscriptions/listen",
    "tools/call",
    "tools/list",
];
const BATCH_REFUSED: &str =
    "Invalid Request: batches are not supported (MCP 2025-06-18 has no JSON-RPC batching)";
const BAD_ID: &str = "Invalid Request: the id must be a string or an integer";
const TOO_LONG: &str = "Invalid Request: the message is longer than 64 MiB";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Forward,
    Skip,
    Reply(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineRead {
    Line,
    TooLong,
    End,
}

#[derive(Debug, Default)]
pub struct Gate {
    initialized: bool,
}

#[derive(Serialize)]
pub struct WireError {
    pub code: i64,
    pub message: String,
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
        let Ok(value) = serde_json::from_slice::<Value>(line) else {
            return Verdict::Reply(reply(Value::Null, PARSE_ERROR, "Parse error"));
        };
        if value.is_array() {
            return Verdict::Reply(reply(Value::Null, INVALID_REQUEST, BATCH_REFUSED));
        }
        if value.get("id").is_some_and(|id| !valid_id(id)) {
            return Verdict::Reply(reply(Value::Null, INVALID_REQUEST, BAD_ID));
        }
        let method = value.get("method").and_then(Value::as_str);
        let id = value.get("id").cloned().unwrap_or(Value::Null);
        if value.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Verdict::Reply(reply(id, INVALID_REQUEST, "Invalid Request"));
        }
        match serde_json::from_slice::<ClientJsonRpcMessage>(line) {
            Ok(JsonRpcMessage::Request(request))
                if matches!(request.request, ClientRequest::CustomRequest(_))
                    && method.is_some_and(known) =>
            {
                Verdict::Reply(invalid_params(id, method))
            }
            Ok(message) => self.admit(&message, id),
            Err(_) if value.get("id").is_none() && method.is_some() => Verdict::Skip,
            Err(_) if method.is_some_and(known) => Verdict::Reply(invalid_params(id, method)),
            Err(_) => Verdict::Reply(reply(id, INVALID_REQUEST, "Invalid Request")),
        }
    }

    fn admit(&mut self, message: &ClientJsonRpcMessage, id: Value) -> Verdict {
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
                    id,
                    NOT_INITIALIZED,
                    "the server is not initialized; send initialize first",
                )),
            },
            _ => Verdict::Skip,
        }
    }
}

fn known(method: &str) -> bool {
    KNOWN_REQUESTS.contains(&method)
}

fn valid_id(id: &Value) -> bool {
    match id {
        Value::String(_) => true,
        Value::Number(number) => number.is_i64(),
        _ => false,
    }
}

fn invalid_params(id: Value, method: Option<&str>) -> String {
    let message = format!(
        "Invalid params for {}",
        crate::error::echo(method.unwrap_or_default())
    );
    reply(id, INVALID_PARAMS, message)
}

fn trimmed(line: &[u8]) -> &[u8] {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    line.strip_prefix(BOM).unwrap_or(line)
}

fn reply(id: Value, code: i64, message: impl Into<String>) -> String {
    let reply = WireReply {
        jsonrpc: "2.0",
        id,
        error: WireError {
            code,
            message: message.into(),
        },
    };
    serde_json::to_string(&reply).unwrap_or_default()
}

pub async fn read_line<R>(
    input: &mut R,
    line: &mut Vec<u8>,
    limit: usize,
) -> std::io::Result<LineRead>
where
    R: AsyncBufRead + Unpin,
{
    line.clear();
    let mut too_long = false;
    loop {
        let available = input.fill_buf().await?;
        if available.is_empty() {
            return Ok(match (too_long, line.is_empty()) {
                (true, _) => LineRead::TooLong,
                (false, true) => LineRead::End,
                (false, false) => LineRead::Line,
            });
        }
        let (chunk, ended) = match available.iter().position(|byte| *byte == b'\n') {
            Some(index) => (&available[..=index], true),
            None => (available, false),
        };
        let taken = chunk.len();
        let content = line.len() + taken - usize::from(ended);
        if !too_long && content <= limit {
            line.extend_from_slice(chunk);
        } else {
            too_long = true;
            line.clear();
        }
        input.consume(taken);
        if ended {
            return Ok(if too_long {
                LineRead::TooLong
            } else {
                LineRead::Line
            });
        }
    }
}

pub async fn pump_in<R, W>(input: R, mut to_server: W, errors: UnboundedSender<String>)
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut input = BufReader::with_capacity(READ_BUFFER_BYTES, input);
    let mut gate = Gate::default();
    let mut line = Vec::new();
    loop {
        let verdict = match read_line(&mut input, &mut line, MAX_LINE_BYTES).await {
            Ok(LineRead::Line) => gate.judge(&line),
            Ok(LineRead::TooLong) => Verdict::Reply(reply(Value::Null, INVALID_REQUEST, TOO_LONG)),
            Ok(LineRead::End) | Err(_) => break,
        };
        match verdict {
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
    use std::io;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use serde_json::json;
    use tokio::sync::mpsc::unbounded_channel;

    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Refuse {
        Everything,
        Text,
        Newline,
    }

    struct Flaky {
        refuse: Refuse,
        attempts: Vec<Vec<u8>>,
    }

    impl Flaky {
        fn new(refuse: Refuse) -> Flaky {
            Flaky {
                refuse,
                attempts: Vec::new(),
            }
        }
    }

    impl AsyncWrite for Flaky {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _context: &mut Context<'_>,
            bytes: &[u8],
        ) -> Poll<io::Result<usize>> {
            self.attempts.push(bytes.to_vec());
            let refused = match self.refuse {
                Refuse::Everything => true,
                Refuse::Text => bytes != b"\n",
                Refuse::Newline => bytes == b"\n",
            };
            Poll::Ready(if refused {
                Err(io::Error::other("refused"))
            } else {
                Ok(bytes.len())
            })
        }

        fn poll_flush(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    fn run<F: Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(future)
    }

    fn pumped_in<W: AsyncWrite + Unpin>(input: &[u8], to_server: W) -> Vec<String> {
        let (errors, mut replies) = unbounded_channel();
        run(pump_in(input, to_server, errors));
        let mut seen = Vec::new();
        while let Ok(text) = replies.try_recv() {
            seen.push(text);
        }
        seen
    }

    fn pumped_out<W: AsyncWrite + Unpin>(server: &[u8], errors: &[&str], output: W) {
        let (sender, receiver) = unbounded_channel();
        for text in errors {
            sender.send((*text).to_string()).unwrap();
        }
        drop(sender);
        run(pump_out(server, output, receiver));
    }

    #[test]
    fn a_line_just_under_the_limit_is_forwarded_whole() {
        let mut call =
            br#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"open","arguments":{"path":""#
                .to_vec();
        let tail = br#""}}}"#;
        call.resize(MAX_LINE_BYTES - 1 - tail.len(), b'x');
        call.extend_from_slice(tail);
        assert_eq!(call.len(), MAX_LINE_BYTES - 1);
        let mut input = line(&initialize());
        input.extend_from_slice(&call);
        input.push(b'\n');
        let mut forwarded = Vec::new();
        assert_eq!(pumped_in(&input, &mut forwarded), Vec::<String>::new());
        assert!(
            forwarded == input,
            "{} of {} bytes",
            forwarded.len(),
            input.len()
        );
    }

    #[test]
    fn reading_stops_once_the_server_side_refuses_a_line() {
        let mut input = line(&initialize());
        input.extend_from_slice(b"not json\n");
        let mut server = Flaky::new(Refuse::Everything);
        assert_eq!(pumped_in(&input, &mut server), Vec::<String>::new());
        assert_eq!(server.attempts, [line(&initialize())]);
    }

    #[test]
    fn server_lines_still_flow_after_the_error_channel_closes() {
        let mut output = Vec::new();
        pumped_out(b"one\ntwo\n", &["early"], &mut output);
        assert_eq!(output, b"early\none\ntwo\n");
    }

    #[test]
    fn writing_stops_when_a_line_or_its_newline_is_refused() {
        for refuse in [Refuse::Text, Refuse::Newline] {
            let mut output = Flaky::new(refuse);
            pumped_out(b"one\ntwo\n", &[], &mut output);
            assert_eq!(
                output.attempts,
                [b"one".to_vec(), b"\n".to_vec()],
                "{refuse:?}"
            );
        }
    }

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

    #[test]
    fn ids_that_are_not_strings_or_integers_get_an_invalid_request_with_a_null_id() {
        let mut gate = Gate::default();
        gate.judge(&line(&initialize()));
        for id in [
            Value::Null,
            json!(1.5),
            json!(99_999_999_999_999_999_999.0),
            json!(true),
            json!({}),
            json!([1]),
        ] {
            let request = json!({"jsonrpc": "2.0", "id": id, "method": "ping"});
            let error = error_of(gate.judge(&line(&request)));
            assert_eq!(error["error"]["code"], INVALID_REQUEST, "{id}");
            assert_eq!(error["error"]["message"], BAD_ID);
            assert_eq!(error["id"], Value::Null);
        }
        let big = b"{\"jsonrpc\":\"2.0\",\"id\":99999999999999999999,\"method\":\"ping\"}\n";
        let error = error_of(gate.judge(big));
        assert_eq!(error["error"]["code"], INVALID_REQUEST);
        assert_eq!(error["id"], Value::Null);
        for id in [json!(-3), json!(0), json!(i64::MAX), json!("abc")] {
            let request = json!({"jsonrpc": "2.0", "id": id, "method": "ping"});
            assert_eq!(gate.judge(&line(&request)), Verdict::Forward, "{id}");
        }
    }

    #[test]
    fn known_methods_with_bad_params_get_invalid_params_with_their_id() {
        let mut gate = Gate::default();
        let bad_initialize =
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": 5});
        let error = error_of(gate.judge(&line(&bad_initialize)));
        assert_eq!(error["error"]["code"], INVALID_PARAMS);
        gate.judge(&line(&initialize()));
        for params in [
            None,
            Some(json!(5)),
            Some(json!({"name": 5})),
            Some(json!({"name": "open", "arguments": [1, 2]})),
        ] {
            let mut request = json!({"jsonrpc": "2.0", "id": "c", "method": "tools/call"});
            if let Some(params) = params.clone() {
                request["params"] = params;
            }
            let error = error_of(gate.judge(&line(&request)));
            assert_eq!(error["error"]["code"], INVALID_PARAMS, "{params:?}");
            assert_eq!(error["id"], "c");
            assert_eq!(
                error["error"]["message"],
                "Invalid params for \"tools/call\""
            );
        }
        let unknown = json!({"jsonrpc": "2.0", "id": 4, "method": "no/such/method"});
        assert_eq!(gate.judge(&line(&unknown)), Verdict::Forward);
        let call = json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "open", "arguments": {}}});
        assert_eq!(gate.judge(&line(&call)), Verdict::Forward);
    }

    #[test]
    fn batches_are_refused_with_a_message_naming_the_protocol() {
        let mut gate = Gate::default();
        gate.judge(&line(&initialize()));
        for batch in [
            json!([]),
            json!([{"jsonrpc": "2.0", "id": 1, "method": "ping"}]),
        ] {
            let error = error_of(gate.judge(&line(&batch)));
            assert_eq!(error["error"]["code"], INVALID_REQUEST);
            assert_eq!(error["error"]["message"], BATCH_REFUSED);
            assert_eq!(error["id"], Value::Null);
        }
    }

    fn lines(input: &[u8], capacity: usize, limit: usize) -> Vec<(LineRead, Vec<u8>)> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        runtime.block_on(async {
            let mut reader = BufReader::with_capacity(capacity, input);
            let mut line = Vec::new();
            let mut seen = Vec::new();
            loop {
                let read = read_line(&mut reader, &mut line, limit).await.unwrap();
                seen.push((read, line.clone()));
                if read == LineRead::End {
                    return seen;
                }
            }
        })
    }

    #[test]
    fn lines_over_the_limit_are_dropped_without_being_kept_and_reading_goes_on() {
        let input = b"abcd\nabcdefghij\nxy\nabcde";
        for capacity in [1, 3, 64] {
            let seen = lines(input, capacity, 4);
            assert_eq!(
                seen,
                vec![
                    (LineRead::Line, b"abcd\n".to_vec()),
                    (LineRead::TooLong, Vec::new()),
                    (LineRead::Line, b"xy\n".to_vec()),
                    (LineRead::TooLong, Vec::new()),
                    (LineRead::End, Vec::new()),
                ],
                "{capacity}"
            );
        }
        let seen = lines(b"abcdefgh", 2, 4);
        assert_eq!(
            seen,
            vec![(LineRead::TooLong, Vec::new()), (LineRead::End, Vec::new())]
        );
        assert_eq!(lines(b"", 8, 4), vec![(LineRead::End, Vec::new())]);
        assert_eq!(
            lines(b"abcd", 3, 4),
            vec![
                (LineRead::Line, b"abcd".to_vec()),
                (LineRead::End, Vec::new())
            ]
        );
    }
}
