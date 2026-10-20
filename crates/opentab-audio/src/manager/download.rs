use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};
use ureq::Agent;
use ureq::config::Config;
use ureq::http::Uri;
use ureq::unversioned::resolver::DefaultResolver;
use ureq::unversioned::transport::{
    Buffers, ConnectionDetails, Connector, DefaultConnector, NextTimeout, Transport, time,
};

use crate::manager::copy::storage;
use crate::manager::install::{InstallError, InstallProgress, InstallStage};

pub const DOWNLOAD_CONNECT_SECONDS: u64 = 30;
pub const DOWNLOAD_RESPONSE_SECONDS: u64 = 60;
pub const DOWNLOAD_STALL_SECONDS: u64 = 30;
pub const DOWNLOAD_DEADLINE_FLOOR_SECONDS: u64 = 60;
pub const DOWNLOAD_MIN_BYTES_PER_SECOND: u64 = 32 * 1024;
pub const DOWNLOAD_REDIRECTS: u32 = 10;
const CHUNK_BYTES: usize = 64 * 1024;
const USER_AGENT: &str = "OpenTab";
const HTTP_OK: u16 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DownloadTimeouts {
    pub connect: Duration,
    pub response: Duration,
    pub stall: Duration,
    pub deadline_floor: Duration,
    pub min_bytes_per_second: u64,
}

impl DownloadTimeouts {
    pub const DEFAULT: DownloadTimeouts = DownloadTimeouts {
        connect: Duration::from_secs(DOWNLOAD_CONNECT_SECONDS),
        response: Duration::from_secs(DOWNLOAD_RESPONSE_SECONDS),
        stall: Duration::from_secs(DOWNLOAD_STALL_SECONDS),
        deadline_floor: Duration::from_secs(DOWNLOAD_DEADLINE_FLOOR_SECONDS),
        min_bytes_per_second: DOWNLOAD_MIN_BYTES_PER_SECOND,
    };

    pub fn deadline(&self, size: u64) -> Duration {
        let seconds = size.div_ceil(self.min_bytes_per_second.max(1));
        self.deadline_floor
            .saturating_add(Duration::from_secs(seconds))
    }
}

impl Default for DownloadTimeouts {
    fn default() -> DownloadTimeouts {
        DownloadTimeouts::DEFAULT
    }
}

#[derive(Debug, Clone, Default)]
pub struct InstallCancel {
    flag: Arc<AtomicBool>,
}

impl InstallCancel {
    pub fn new() -> InstallCancel {
        InstallCancel::default()
    }

    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    pub(crate) fn check(&self) -> Result<(), InstallError> {
        if self.is_cancelled() {
            return Err(InstallError::Cancelled);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Route {
    Secure,
    Loopback,
}

fn loopback(host: &str) -> bool {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    bare.parse::<IpAddr>()
        .is_ok_and(|address| address.is_loopback())
}

fn route(url: &str) -> Result<Route, InstallError> {
    let uri: Uri = url.parse().map_err(|_| InstallError::Address {
        url: url.to_owned(),
    })?;
    let host = uri.host().ok_or_else(|| InstallError::Address {
        url: url.to_owned(),
    })?;
    match uri.scheme_str() {
        Some("https") => Ok(Route::Secure),
        Some("http") if loopback(host) => Ok(Route::Loopback),
        _ => Err(InstallError::Insecure {
            url: url.to_owned(),
        }),
    }
}

pub fn check_download_url(url: &str) -> Result<(), InstallError> {
    route(url).map(|_| ())
}

#[derive(Debug)]
struct StallConnector {
    stall: Duration,
}

#[derive(Debug)]
struct StallTransport {
    inner: Box<dyn Transport>,
    stall: Duration,
}

fn bounded(timeout: NextTimeout, stall: Duration) -> NextTimeout {
    if *timeout.after <= stall {
        return timeout;
    }
    NextTimeout {
        after: time::Duration::Exact(stall),
        reason: timeout.reason,
    }
}

impl Connector<Box<dyn Transport>> for StallConnector {
    type Out = StallTransport;

    fn connect(
        &self,
        _details: &ConnectionDetails<'_>,
        chained: Option<Box<dyn Transport>>,
    ) -> Result<Option<StallTransport>, ureq::Error> {
        Ok(chained.map(|inner| StallTransport {
            inner,
            stall: self.stall,
        }))
    }
}

impl Transport for StallTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.inner.buffers()
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.inner
            .transmit_output(amount, bounded(timeout, self.stall))
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        self.inner.await_input(bounded(timeout, self.stall))
    }

    fn is_open(&mut self) -> bool {
        self.inner.is_open()
    }

    fn is_tls(&self) -> bool {
        self.inner.is_tls()
    }
}

fn agent_config(route: Route, timeouts: &DownloadTimeouts, size: u64) -> Config {
    let builder = Agent::config_builder()
        .http_status_as_error(false)
        .user_agent(USER_AGENT)
        .timeout_connect(Some(timeouts.connect))
        .timeout_recv_response(Some(timeouts.response))
        .timeout_recv_body(Some(timeouts.deadline(size)));
    match route {
        Route::Secure => builder.https_only(true).max_redirects(DOWNLOAD_REDIRECTS),
        Route::Loopback => builder.https_only(false).max_redirects(0).proxy(None),
    }
    .build()
}

fn agent(config: Config, timeouts: &DownloadTimeouts) -> Agent {
    let connector = DefaultConnector::new().chain(StallConnector {
        stall: timeouts.stall,
    });
    Agent::with_parts(config, connector, DefaultResolver::default())
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

fn timed_out(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::TimedOut
        || error
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<ureq::Error>())
            .is_some_and(|inner| matches!(inner, ureq::Error::Timeout(_)))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) struct Expected<'a> {
    pub(crate) url: &'a str,
    pub(crate) size: u64,
    pub(crate) sha256: &'a str,
}

pub(crate) struct Fetch<'a> {
    pub(crate) timeouts: &'a DownloadTimeouts,
    pub(crate) cancel: &'a InstallCancel,
}

pub(crate) fn fetch(
    expected: &Expected<'_>,
    target: &Path,
    control: &Fetch<'_>,
    progress: &mut dyn FnMut(InstallProgress),
) -> Result<(), InstallError> {
    let route = route(expected.url)?;
    control.cancel.check()?;
    let config = agent_config(route, control.timeouts, expected.size);
    let response = agent(config, control.timeouts)
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
        control.cancel.check()?;
        let read = match body.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if timed_out(&error) => {
                return Err(InstallError::Offline {
                    message: error.to_string(),
                });
            }
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
    control.cancel.check()?;
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

#[cfg(test)]
mod tests {
    use opentab_testkit::http::{LoopbackServer, Reply};

    use super::*;

    const PATH: &str = "/plain.sf2";

    #[test]
    fn the_secure_route_allows_https_hops_only_and_the_loopback_route_no_redirect() {
        let timeouts = DownloadTimeouts::DEFAULT;
        let secure = agent_config(Route::Secure, &timeouts, 1);
        assert!(secure.https_only());
        assert_eq!(secure.max_redirects(), DOWNLOAD_REDIRECTS);
        let local = agent_config(Route::Loopback, &timeouts, 1);
        assert!(!local.https_only());
        assert_eq!(local.max_redirects(), 0);
        assert!(local.proxy().is_none());
    }

    #[test]
    fn a_secure_agent_refuses_a_plain_http_hop_before_connecting() {
        let server = LoopbackServer::start(vec![Reply::ok(PATH, b"plain".to_vec())]);
        let url = server.url(PATH);
        let timeouts = DownloadTimeouts::DEFAULT;
        let outcome = agent(agent_config(Route::Secure, &timeouts, 5), &timeouts)
            .get(&url)
            .call()
            .map(|response| response.status().as_u16())
            .map_err(|error| request_error(&url, error));
        assert_eq!(outcome, Err(InstallError::Insecure { url: url.clone() }));
        assert_eq!(server.requests(), Vec::<String>::new());
    }

    #[test]
    fn the_deadline_grows_with_the_size_from_a_floor() {
        let timeouts = DownloadTimeouts::DEFAULT;
        assert_eq!(
            timeouts.deadline(0),
            Duration::from_secs(DOWNLOAD_DEADLINE_FLOOR_SECONDS)
        );
        assert_eq!(
            timeouts.deadline(DOWNLOAD_MIN_BYTES_PER_SECOND * 100 + 1),
            Duration::from_secs(DOWNLOAD_DEADLINE_FLOOR_SECONDS + 101)
        );
        assert_eq!(
            timeouts.deadline(215_614_036),
            Duration::from_secs(DOWNLOAD_DEADLINE_FLOOR_SECONDS + 6_581)
        );
    }

    #[test]
    fn a_stall_bound_shortens_only_longer_waits() {
        let stall = Duration::from_secs(5);
        let long = NextTimeout {
            after: time::Duration::NotHappening,
            reason: ureq::Timeout::RecvBody,
        };
        assert_eq!(*bounded(long, stall).after, stall);
        let short = NextTimeout {
            after: time::Duration::Exact(Duration::from_secs(1)),
            reason: ureq::Timeout::RecvBody,
        };
        assert_eq!(*bounded(short, stall).after, Duration::from_secs(1));
        assert_eq!(bounded(short, stall).reason, ureq::Timeout::RecvBody);
    }
}
