use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::thread::JoinHandle;
use std::time::Duration;

const STOP_PATH: &str = "/__opentab_stop";
const REQUEST_LIMIT: usize = 16 * 1024;
const LOOPBACK: &str = "127.0.0.1:0";
const SAFETY_SECONDS: u64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Declared {
    Exact,
    Absent,
    Bytes(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub path: String,
    pub status: u16,
    pub body: Vec<u8>,
    pub declared: Declared,
    pub cut: Option<usize>,
}

impl Reply {
    pub fn ok(path: &str, body: Vec<u8>) -> Reply {
        Reply {
            path: path.to_owned(),
            status: 200,
            body,
            declared: Declared::Exact,
            cut: None,
        }
    }
}

pub struct LoopbackServer {
    address: SocketAddr,
    thread: Option<JoinHandle<Vec<String>>>,
}

fn read_request(stream: &mut TcpStream) -> Option<String> {
    let mut request = Vec::new();
    let mut byte = [0u8; 1];
    while !request.ends_with(b"\r\n\r\n") {
        match stream.read(&mut byte) {
            Ok(1) => request.push(byte[0]),
            _ => return None,
        }
        if request.len() > REQUEST_LIMIT {
            return None;
        }
    }
    let text = String::from_utf8_lossy(&request).into_owned();
    let line = text.lines().next()?;
    line.split(' ').nth(1).map(str::to_owned)
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
    head.push_str("\r\n");
    let sent = reply.cut.unwrap_or(reply.body.len()).min(reply.body.len());
    let _ = stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(&reply.body[..sent]))
        .and_then(|()| stream.flush());
}

fn serve(listener: TcpListener, replies: Vec<Reply>) -> Vec<String> {
    let mut seen = Vec::new();
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else {
            continue;
        };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(SAFETY_SECONDS)));
        let Some(path) = read_request(&mut stream) else {
            continue;
        };
        if path == STOP_PATH {
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            break;
        }
        answer(&mut stream, replies.iter().find(|reply| reply.path == path));
        let _ = stream.shutdown(Shutdown::Both);
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

pub fn closed_loopback_url(path: &str) -> String {
    let listener = match TcpListener::bind(LOOPBACK) {
        Ok(listener) => listener,
        Err(error) => panic!("cannot reserve a loopback port: {error}"),
    };
    let address = match listener.local_addr() {
        Ok(address) => address,
        Err(error) => panic!("the reserved port has no address: {error}"),
    };
    drop(listener);
    format!("http://{address}{path}")
}
