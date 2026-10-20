use std::path::{Path, PathBuf};

use opentab_testkit::banks::tiny_sf2;
use opentab_testkit::http::{LoopbackServer, Reply};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

use crate::support::{Server, fixture};

const PATH: &str = "/bank.sf2";
const ID: &str = "hosted-sf2";

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn record(url: &str, size: usize, claimed: &[u8]) -> Value {
    json!({
        "id": ID,
        "name": "Hosted test bank",
        "format": "sf2",
        "archive": "raw",
        "url": url,
        "sha256": hex(claimed),
        "size": size,
        "licence_name": "CC0-1.0",
        "licence_url": "https://example.org/licence",
        "licence_text": "No rights reserved.",
        "covers": "tests"
    })
}

fn catalogue(dir: &Path, records: &[Value]) -> PathBuf {
    let path = dir.join("catalogue.json");
    std::fs::write(&path, Value::Array(records.to_vec()).to_string()).expect("write");
    path
}

fn message(error: &Value) -> &str {
    error["message"].as_str().expect("a message")
}

#[test]
fn install_soundbank_downloads_from_the_test_catalogue_and_the_bank_is_used() {
    let data = TempDir::new().expect("a temp dir");
    let bytes = tiny_sf2();
    let host = LoopbackServer::start(vec![Reply::ok(PATH, bytes.clone())]);
    let file = catalogue(data.path(), &[record(&host.url(PATH), bytes.len(), &bytes)]);
    let mut server = Server::start_with_catalogue(data.path(), &file);
    let listing = server.ok("soundbanks", json!({}));
    assert_eq!(listing["catalogue"].as_array().unwrap().len(), 1);
    assert_eq!(listing["catalogue"][0]["id"], ID);
    assert_eq!(listing["catalogue"][0]["installed"], false);

    let installed = server.ok("install_soundbank", json!({"id": ID}));
    assert_eq!(installed["installed"]["id"], ID);
    assert_eq!(installed["installed"]["source"], "catalogue");
    assert_eq!(installed["installed"]["format"], "sf2");
    let listing = server.ok("soundbanks", json!({}));
    assert_eq!(listing["banks"][0]["id"], ID);
    assert_eq!(listing["catalogue"][0]["installed"], true);

    server.ok(
        "assign_soundbank",
        json!({"instrument": "guitar", "bank": ID}),
    );
    let session = server.open(&fixture("repeats.gp"));
    let reply = server.call("render_audio", json!({"session": session, "bars": "1"}));
    let report = reply.text(1);
    assert_eq!(report["backends"][0]["format"], "sf2", "{report}");
    assert_eq!(report["backends"][0]["bank"], ID);
    assert_eq!(host.requests(), vec![PATH.to_owned()]);
}

#[test]
fn a_download_that_does_not_match_its_hash_installs_nothing() {
    let data = TempDir::new().expect("a temp dir");
    let bytes = tiny_sf2();
    let host = LoopbackServer::start(vec![Reply::ok(PATH, bytes.clone())]);
    let file = catalogue(
        data.path(),
        &[record(&host.url(PATH), bytes.len(), b"something else")],
    );
    let mut server = Server::start_with_catalogue(data.path(), &file);
    let error = server.fails("install_soundbank", json!({"id": ID}));
    assert_eq!(error["kind"], "sound_bank");
    assert!(message(&error).contains("checksum"), "{error}");
    assert!(error.get("sub_kind").is_none(), "{error}");
    let listing = server.ok("soundbanks", json!({}));
    assert_eq!(listing["banks"], json!([]));
    assert_eq!(listing["catalogue"][0]["installed"], false);
    assert_eq!(
        std::fs::read_dir(data.path().join("staging"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn an_address_that_is_not_https_or_loopback_is_refused_by_the_policy() {
    let data = TempDir::new().expect("a temp dir");
    let bytes = tiny_sf2();
    let file = catalogue(
        data.path(),
        &[record("http://example.org/bank.sf2", bytes.len(), &bytes)],
    );
    let mut server = Server::start_with_catalogue(data.path(), &file);
    let error = server.fails("install_soundbank", json!({"id": ID}));
    assert_eq!(error["kind"], "sound_bank");
    assert_eq!(error["sub_kind"], "insecure_address");
    assert!(message(&error).contains("(https)"), "{error}");
    assert_eq!(server.ok("soundbanks", json!({}))["banks"], json!([]));
}

#[test]
fn a_broken_or_missing_test_catalogue_is_a_clear_error() {
    let data = TempDir::new().expect("a temp dir");
    let broken = data.path().join("broken.json");
    std::fs::write(&broken, b"{\"not\": \"a list\"}").unwrap();
    let mut server = Server::start_with_catalogue(data.path(), &broken);
    let error = server.fails("soundbanks", json!({}));
    assert_eq!(error["kind"], "invalid_input");
    assert!(message(&error).contains("not a catalogue file"), "{error}");
    let gone = data.path().join("gone.json");
    let mut server = Server::start_with_catalogue(data.path(), &gone);
    let error = server.fails("install_soundbank", json!({"id": ID}));
    assert_eq!(error["kind"], "io");
    assert!(message(&error).contains("gone.json"), "{error}");
}
