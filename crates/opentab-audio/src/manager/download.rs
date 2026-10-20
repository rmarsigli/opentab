use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::net::IpAddr;
use std::path::Path;
use std::time::Duration;

use sha2::{Digest, Sha256};
use ureq::Agent;
use ureq::http::Uri;

use crate::manager::copy::storage;
use crate::manager::install::{InstallError, InstallProgress, InstallStage};

pub const DOWNLOAD_CONNECT_SECONDS: u64 = 30;
pub const DOWNLOAD_RESPONSE_SECONDS: u64 = 60;
pub const DOWNLOAD_REDIRECTS: u32 = 10;
const CHUNK_BYTES: usize = 64 * 1024;
const USER_AGENT: &str = "OpenTab";
const HTTP_OK: u16 = 200;
const LOCALHOST: &str = "localhost";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Transport {
    Secure,
    Loopback,
}

fn loopback(host: &str) -> bool {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    bare.eq_ignore_ascii_case(LOCALHOST)
        || bare
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn transport(url: &str) -> Result<Transport, InstallError> {
    let uri: Uri = url.parse().map_err(|_| InstallError::Address {
        url: url.to_owned(),
    })?;
    let host = uri.host().ok_or_else(|| InstallError::Address {
        url: url.to_owned(),
    })?;
    match uri.scheme_str() {
        Some("https") => Ok(Transport::Secure),
        Some("http") if loopback(host) => Ok(Transport::Loopback),
        _ => Err(InstallError::Insecure {
            url: url.to_owned(),
        }),
    }
}

pub fn check_download_url(url: &str) -> Result<(), InstallError> {
    transport(url).map(|_| ())
}

fn agent(transport: Transport) -> Agent {
    let mut builder = Agent::config_builder()
        .http_status_as_error(false)
        .user_agent(USER_AGENT)
        .timeout_connect(Some(Duration::from_secs(DOWNLOAD_CONNECT_SECONDS)))
        .timeout_recv_response(Some(Duration::from_secs(DOWNLOAD_RESPONSE_SECONDS)));
    builder = match transport {
        Transport::Secure => builder.https_only(true).max_redirects(DOWNLOAD_REDIRECTS),
        Transport::Loopback => builder.https_only(false).max_redirects(0).proxy(None),
    };
    Agent::new_with_config(builder.build())
}

fn request_error(url: &str, error: ureq::Error) -> InstallError {
    match error {
        ureq::Error::ConnectionFailed
        | ureq::Error::HostNotFound
        | ureq::Error::Timeout(_)
        | ureq::Error::Io(_) => InstallError::Offline {
            message: error.to_string(),
        },
        ureq::Error::RequireHttpsOnly(_) => InstallError::Insecure {
            url: url.to_owned(),
        },
        other => InstallError::Network {
            message: other.to_string(),
        },
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) struct Expected<'a> {
    pub(crate) url: &'a str,
    pub(crate) size: u64,
    pub(crate) sha256: &'a str,
}

pub(crate) fn fetch(
    expected: &Expected<'_>,
    target: &Path,
    progress: &mut dyn FnMut(InstallProgress),
) -> Result<(), InstallError> {
    let transport = transport(expected.url)?;
    let response = agent(transport)
        .get(expected.url)
        .call()
        .map_err(|error| request_error(expected.url, error))?;
    let status = response.status().as_u16();
    if status != HTTP_OK {
        return Err(InstallError::Http { status });
    }
    if let Some(declared) = response.body().content_length() {
        if declared > expected.size {
            return Err(InstallError::TooLarge {
                expected: expected.size,
            });
        }
        if declared < expected.size {
            return Err(InstallError::Truncated {
                expected: expected.size,
                received: declared,
            });
        }
    }
    let mut body = response
        .into_body()
        .into_reader()
        .take(expected.size.saturating_add(1));
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(|error| InstallError::Bank(storage(target, &error)))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; CHUNK_BYTES];
    let mut received = 0u64;
    loop {
        let read = match body.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => {
                return Err(InstallError::Truncated {
                    expected: expected.size,
                    received,
                });
            }
        };
        received = received.saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
        if received > expected.size {
            return Err(InstallError::TooLarge {
                expected: expected.size,
            });
        }
        hasher.update(&buffer[..read]);
        output
            .write_all(&buffer[..read])
            .map_err(|error| InstallError::Bank(storage(target, &error)))?;
        progress(InstallProgress {
            stage: InstallStage::Downloading,
            done: received,
            total: expected.size,
        });
    }
    if received < expected.size {
        return Err(InstallError::Truncated {
            expected: expected.size,
            received,
        });
    }
    output
        .sync_all()
        .map_err(|error| InstallError::Bank(storage(target, &error)))?;
    progress(InstallProgress {
        stage: InstallStage::Verifying,
        done: received,
        total: expected.size,
    });
    let actual = hex(&hasher.finalize());
    if !actual.eq_ignore_ascii_case(expected.sha256) {
        return Err(InstallError::HashMismatch {
            expected: expected.sha256.to_owned(),
            actual,
        });
    }
    Ok(())
}
