use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::thread::JoinHandle;
use std::time::Duration;

const STOP_PATH: &str = "/__opentab_stop";
const REQUEST_LIMIT: usize = 16 * 1024;
const LOOPBACK: &str = "127.0.0.1:0";
const SAFETY_SECONDS: u64 = 30;
const DRAIN_BYTES: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Declared {
    Exact,
    Absent,
    Bytes(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    Close,
    Hold,
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pace {
    pub chunk: usize,
    pub pause_millis: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub path: String,
    pub status: u16,
    pub body: Vec<u8>,
    pub declared: Declared,
    pub cut: Option<usize>,
    pub ending: Ending,
    pub pace: Option<Pace>,
    pub headers: Vec<Header>,
}

impl Reply {
    pub fn ok(path: &str, body: Vec<u8>) -> Reply {
        Reply {
            path: path.to_owned(),
            status: 200,
            body,
            declared: Declared::Exact,
            cut: None,
            ending: Ending::Close,
            pace: None,
            headers: Vec::new(),
        }
    }
}

pub struct LoopbackServer {
    address: SocketAddr,
    thread: Option<JoinHandle<Vec<String>>>,
}

fn read_until(stream: &mut TcpStream, request: &mut Vec<u8>, end: &[u8]) -> bool {
    let mut byte = [0u8; 1];
    while !request.ends_with(end) {
        match stream.read(&mut byte) {
            Ok(1) => request.push(byte[0]),
            _ => return false,
        }
        if request.len() > REQUEST_LIMIT {
            return false;
        }
    }
    true
}

fn path_of(request: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(request).into_owned();
    let line = text.lines().next()?;
    line.split(' ').nth(1).map(str::to_owned)
}

fn send_body(stream: &mut TcpStream, body: &[u8], pace: Option<Pace>) -> std::io::Result<()> {
    let Some(pace) = pace else {
        return stream.write_all(body);
    };
    for piece in body.chunks(pace.chunk.max(1)) {
        stream.write_all(piece)?;
        stream.flush()?;
        std::thread::sleep(Duration::from_millis(pace.pause_millis));
    }
    Ok(())
}

fn hold(stream: &mut TcpStream) {
    let mut sink = [0u8; DRAIN_BYTES];
    while let Ok(read) = stream.read(&mut sink) {
        if read == 0 {
            break;
        }
    }
}

fn answer(stream: &mut TcpStream, reply: Option<&Reply>) {
    let Some(reply) = reply else {
        let _ = stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        return;
    };
    let mut head = format!("HTTP/1.1 {} Reply\r\nConnection: close\r\n", reply.status);
    match reply.declared {
        Declared::Exact => head.push_str(&format!("Content-Length: {}\r\n", reply.body.len())),
        Declared::Bytes(bytes) => head.push_str(&format!("Content-Length: {bytes}\r\n")),
        Declared::Absent => {}
    }
    for header in &reply.headers {
        head.push_str(&format!("{}: {}\r\n", header.name, header.value));
    }
    head.push_str("\r\n");
    let sent = reply.cut.unwrap_or(reply.body.len()).min(reply.body.len());
    let written = stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.flush())
        .and_then(|()| send_body(stream, &reply.body[..sent], reply.pace))
        .and_then(|()| stream.flush());
    if written.is_ok() && reply.ending == Ending::Hold {
        hold(stream);
    }
}

fn serve(listener: TcpListener, replies: Vec<Reply>) -> Vec<String> {
    let mut seen = Vec::new();
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else {
            continue;
        };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(SAFETY_SECONDS)));
        let mut request = Vec::new();
        if !read_until(&mut stream, &mut request, b"\r\n") {
            continue;
        }
        let Some(path) = path_of(&request) else {
            continue;
        };
        let reply = replies.iter().find(|reply| reply.path == path);
        let resets = reply.is_some_and(|reply| reply.ending == Ending::Reset);
        if !resets && !read_until(&mut stream, &mut request, b"\r\n\r\n") {
            continue;
        }
        if path == STOP_PATH {
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            break;
        }
        answer(&mut stream, reply);
        if !resets {
            let _ = stream.shutdown(Shutdown::Both);
        }
        drop(stream);
        seen.push(path);
    }
    seen
}

impl LoopbackServer {
    pub fn start(replies: Vec<Reply>) -> LoopbackServer {
        let listener = match TcpListener::bind(LOOPBACK) {
            Ok(listener) => listener,
            Err(error) => panic!("cannot open the loopback test server: {error}"),
        };
        let address = match listener.local_addr() {
            Ok(address) => address,
            Err(error) => panic!("the loopback test server has no address: {error}"),
        };
        let thread = std::thread::spawn(move || serve(listener, replies));
        LoopbackServer {
            address,
            thread: Some(thread),
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }

    pub fn port(&self) -> u16 {
        self.address.port()
    }

    fn halt(&mut self) -> Vec<String> {
        let Some(thread) = self.thread.take() else {
            return Vec::new();
        };
        if let Ok(mut stream) = TcpStream::connect(self.address) {
            let request = format!("GET {STOP_PATH} HTTP/1.1\r\nHost: {}\r\n\r\n", self.address);
            let _ = stream.write_all(request.as_bytes());
            let mut sink = Vec::new();
            let _ = stream.read_to_end(&mut sink);
        }
        thread.join().unwrap_or_default()
    }

    pub fn requests(mut self) -> Vec<String> {
        self.halt()
    }
}

impl Drop for LoopbackServer {
    fn drop(&mut self) {
        self.halt();
    }
}

pub struct ClosedPort {
    reservation: TcpListener,
}

impl ClosedPort {
    pub fn reserve() -> ClosedPort {
        match TcpListener::bind(LOOPBACK) {
            Ok(reservation) => ClosedPort { reservation },
            Err(error) => panic!("cannot reserve a loopback port: {error}"),
        }
    }

    pub fn url(&self, path: &str) -> String {
        let port = match self.reservation.local_addr() {
            Ok(address) => address.port(),
            Err(error) => panic!("the reserved port has no address: {error}"),
        };
        format!("http://[::1]:{port}{path}")
    }
}
