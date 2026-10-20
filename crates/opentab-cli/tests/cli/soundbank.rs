use std::fs;
use std::path::{Path, PathBuf};

use opentab_testkit::archives::{ZipItem, zip_archive};
use opentab_testkit::banks::{tiny_sf2, tiny_sfz};
use opentab_testkit::http::{LoopbackServer, Reply};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::support::{path_str, run_in, run_with_catalogue, temp};

const PATH: &str = "/bank.bin";

struct Hosted {
    id: &'static str,
    format: &'static str,
    archive: &'static str,
    bytes: Vec<u8>,
}

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn record(hosted: &Hosted, url: &str, claimed: &[u8]) -> Value {
    json!({
        "id": hosted.id,
        "name": format!("Test {}", hosted.id),
        "format": hosted.format,
        "archive": hosted.archive,
        "url": url,
        "sha256": hex(claimed),
        "size": hosted.bytes.len(),
        "licence_name": "CC0-1.0",
        "licence_url": "https://example.org/licence",
        "licence_text": "No rights reserved.",
        "attribution": null,
        "covers": "tests"
    })
}

fn catalogue_file(dir: &Path, records: Vec<Value>) -> PathBuf {
    let path = dir.join("catalogue.json");
    fs::write(&path, Value::Array(records).to_string()).expect("write catalogue");
    path
}

fn sf2_hosted() -> Hosted {
    Hosted {
        id: "hosted-sf2",
        format: "sf2",
        archive: "raw",
        bytes: tiny_sf2(),
    }
}

fn zipped_sfz(dir: &Path) -> Vec<u8> {
    let root = tiny_sfz(&dir.join("zip-source"));
    let mut items = Vec::new();
    collect(&root, &root, &mut items);
    zip_archive(&items)
}

fn collect(root: &Path, folder: &Path, items: &mut Vec<ZipItem>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(folder)
        .expect("read dir")
        .map(|entry| entry.expect("entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(root, &path, items);
        } else {
            let name = path
                .strip_prefix(root)
                .expect("inside the root")
                .to_string_lossy()
                .replace('\\', "/");
            items.push(ZipItem::File {
                name,
                data: fs::read(&path).expect("read file"),
            });
        }
    }
}

fn soundbank(dir: &Path, args: &[&str]) -> crate::support::Run {
    let mut full = vec!["soundbank"];
    full.extend(args);
    run_in(dir, &full)
}

#[test]
fn list_on_a_fresh_data_folder_is_empty_and_creates_the_folder() {
    let dir = temp();
    let data = dir.path().join("fresh");
    let result = soundbank(&data, &["list", "--json"]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        result.json(),
        json!({"banks": [], "assignments": {}, "problems": []})
    );
    assert!(data.join("banks").is_dir());
    let human = soundbank(&data, &["list"]);
    assert!(
        human.stdout.contains("no sound banks installed"),
        "{}",
        human.stdout
    );
}

#[test]
fn the_data_folder_comes_from_the_environment_variable() {
    let dir = temp();
    let file = dir.path().join("Warm Piano.sf2");
    fs::write(&file, tiny_sf2()).unwrap();
    let data = dir.path().join("data");
    let result = soundbank(&data, &["import", path_str(&file), "--json"]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    let path = PathBuf::from(result.json()["imported"]["path"].as_str().expect("a path"));
    assert!(
        path.starts_with(fs::canonicalize(&data).unwrap()),
        "{path:?}"
    );
    assert!(path.is_file());
    assert!(data.join("banks.json").is_file());
}

#[test]
fn an_sf2_file_imports_and_lists() {
    let dir = temp();
    let file = dir.path().join("Warm Piano.sf2");
    fs::write(&file, tiny_sf2()).unwrap();
    let result = soundbank(dir.path(), &["import", path_str(&file), "--json"]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    let imported = &result.json()["imported"];
    assert_eq!(imported["id"], json!("warm-piano"));
    assert_eq!(imported["format"], json!("sf2"));
    assert_eq!(imported["source"], json!("imported"));
    assert_eq!(imported["bytes"], json!(tiny_sf2().len()));
    assert!(imported["instruments"].as_u64().unwrap() > 0);
    let listed = soundbank(dir.path(), &["list", "--json"]).json();
    assert_eq!(listed["banks"].as_array().unwrap().len(), 1);
    assert_eq!(listed["banks"][0]["id"], json!("warm-piano"));
    let human = soundbank(dir.path(), &["import", path_str(&file)]);
    assert!(
        human.stdout.starts_with("imported warm-piano"),
        "{}",
        human.stdout
    );
}

#[test]
fn an_sfz_folder_imports() {
    let dir = temp();
    let root = tiny_sfz(&dir.path().join("source"));
    let result = soundbank(dir.path(), &["import", path_str(&root), "--json"]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    let imported = &result.json()["imported"];
    assert_eq!(imported["format"], json!("sfz"));
    assert_eq!(imported["source"], json!("imported"));
    assert!(Path::new(imported["path"].as_str().unwrap()).is_dir());
}

#[test]
fn a_file_that_is_not_a_sound_bank_is_refused() {
    let dir = temp();
    let junk = dir.path().join("junk.sf2");
    fs::write(&junk, b"not a sound font").unwrap();
    let result = soundbank(dir.path(), &["import", path_str(&junk), "--json"]);
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["kind"], json!("sound_bank"));
    let gone = dir.path().join("gone.sf2");
    let result = soundbank(dir.path(), &["import", path_str(&gone), "--json"]);
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["kind"], json!("io"));
    let listed = soundbank(dir.path(), &["list", "--json"]).json();
    assert_eq!(listed["banks"], json!([]));
}

#[test]
fn the_built_in_catalogue_lists_nine_banks_without_the_network() {
    let dir = temp();
    let result = soundbank(dir.path(), &["catalogue", "--json"]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    let items = result.json()["catalogue"].as_array().unwrap().clone();
    assert_eq!(items.len(), 9);
    assert!(items.iter().all(|item| item["installed"] == json!(false)));
    assert_eq!(items[0]["id"], json!("musescore-general"));
    assert_eq!(items[0]["format"], json!("sf2"));
    assert_eq!(items[0]["archive"], json!("raw"));
    assert_eq!(items[0]["licence"], json!("MIT"));
}

#[test]
fn a_raw_sf2_installs_from_the_loopback_server_and_marks_the_catalogue() {
    let dir = temp();
    let hosted = sf2_hosted();
    let server = LoopbackServer::start(vec![Reply::ok(PATH, hosted.bytes.clone())]);
    let file = catalogue_file(
        dir.path(),
        vec![record(&hosted, &server.url(PATH), &hosted.bytes)],
    );
    let catalogue = path_str(&file);
    let listed = soundbank(
        dir.path(),
        &["catalogue", "--catalogue", catalogue, "--json"],
    );
    assert_eq!(listed.code, 0, "{}", listed.stderr);
    assert_eq!(listed.json()["catalogue"][0]["id"], json!("hosted-sf2"));
    assert_eq!(listed.json()["catalogue"][0]["installed"], json!(false));
    let result = soundbank(
        dir.path(),
        &["install", "hosted-sf2", "--catalogue", catalogue, "--json"],
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    let installed = &result.json()["installed"];
    assert_eq!(installed["id"], json!("hosted-sf2"));
    assert_eq!(installed["source"], json!("catalogue"));
    assert_eq!(installed["format"], json!("sf2"));
    let listed = soundbank(
        dir.path(),
        &["catalogue", "--catalogue", catalogue, "--json"],
    );
    assert_eq!(listed.json()["catalogue"][0]["installed"], json!(true));
    let again = soundbank(
        dir.path(),
        &["install", "hosted-sf2", "--catalogue", catalogue, "--json"],
    );
    assert_eq!(again.code, 0, "{}", again.stderr);
    assert_eq!(server.requests(), vec![PATH.to_owned()]);
    let banks = soundbank(dir.path(), &["list", "--json"]).json();
    assert_eq!(banks["banks"][0]["id"], json!("hosted-sf2"));
}

#[test]
fn the_catalogue_variable_replaces_the_catalogue_unless_a_file_is_given() {
    let dir = temp();
    let hosted = sf2_hosted();
    let server = LoopbackServer::start(vec![Reply::ok(PATH, hosted.bytes.clone())]);
    let file = catalogue_file(
        dir.path(),
        vec![record(&hosted, &server.url(PATH), &hosted.bytes)],
    );
    let listed = run_with_catalogue(dir.path(), &file, &["soundbank", "catalogue", "--json"]);
    assert_eq!(listed.code, 0, "{}", listed.stderr);
    let items = listed.json()["catalogue"].as_array().unwrap().clone();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], json!("hosted-sf2"));
    let gone = dir.path().join("gone.json");
    let overridden = run_with_catalogue(
        dir.path(),
        &gone,
        &[
            "soundbank",
            "catalogue",
            "--catalogue",
            path_str(&file),
            "--json",
        ],
    );
    assert_eq!(overridden.code, 0, "{}", overridden.stderr);
    assert_eq!(overridden.json()["catalogue"][0]["id"], json!("hosted-sf2"));
    let missing = run_with_catalogue(dir.path(), &gone, &["soundbank", "catalogue", "--json"]);
    assert_eq!(missing.code, 2);
    assert_eq!(missing.json()["error"]["kind"], json!("io"));
    let installed = run_with_catalogue(
        dir.path(),
        &file,
        &["soundbank", "install", "hosted-sf2", "--json"],
    );
    assert_eq!(installed.code, 0, "{}", installed.stderr);
    assert_eq!(installed.json()["installed"]["id"], json!("hosted-sf2"));
    assert_eq!(server.requests(), vec![PATH.to_owned()]);
}

#[test]
fn a_zipped_sfz_bank_installs_and_reports_its_stages() {
    let dir = temp();
    let archive = zipped_sfz(dir.path());
    let hosted = Hosted {
        id: "hosted-sfz",
        format: "sfz",
        archive: "zip",
        bytes: archive,
    };
    let server = LoopbackServer::start(vec![Reply::ok(PATH, hosted.bytes.clone())]);
    let file = catalogue_file(
        dir.path(),
        vec![record(&hosted, &server.url(PATH), &hosted.bytes)],
    );
    let result = soundbank(
        dir.path(),
        &["install", "hosted-sfz", "--catalogue", path_str(&file)],
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(
        result.stdout.starts_with("installed hosted-sfz"),
        "{}",
        result.stdout
    );
    for stage in ["downloading", "verifying", "unpacking", "registering"] {
        assert!(result.stderr.contains(stage), "{stage}: {}", result.stderr);
    }
    let banks = soundbank(dir.path(), &["list", "--json"]).json();
    assert_eq!(banks["banks"][0]["format"], json!("sfz"));
}

#[test]
fn a_download_that_does_not_match_its_hash_installs_nothing() {
    let dir = temp();
    let hosted = sf2_hosted();
    let server = LoopbackServer::start(vec![Reply::ok(PATH, hosted.bytes.clone())]);
    let file = catalogue_file(
        dir.path(),
        vec![record(&hosted, &server.url(PATH), b"something else")],
    );
    let result = soundbank(
        dir.path(),
        &[
            "install",
            "hosted-sf2",
            "--catalogue",
            path_str(&file),
            "--json",
        ],
    );
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["kind"], json!("sound_bank"));
    drop(server);
    let banks = soundbank(dir.path(), &["list", "--json"]).json();
    assert_eq!(banks["banks"], json!([]));
    assert_eq!(fs::read_dir(dir.path().join("staging")).unwrap().count(), 0);
}

#[test]
fn an_install_from_an_address_that_is_not_https_or_loopback_is_refused() {
    let dir = temp();
    let hosted = sf2_hosted();
    let file = catalogue_file(
        dir.path(),
        vec![record(
            &hosted,
            "http://example.org/bank.sf2",
            &hosted.bytes,
        )],
    );
    let result = soundbank(
        dir.path(),
        &[
            "install",
            "hosted-sf2",
            "--catalogue",
            path_str(&file),
            "--json",
        ],
    );
    assert_eq!(result.code, 2);
    let error = &result.json()["error"];
    assert_eq!(error["kind"], json!("sound_bank"));
    assert_eq!(error["sub_kind"], json!("insecure_address"));
    assert!(
        error["message"].as_str().unwrap().contains("(https)"),
        "{error}"
    );
}

#[test]
fn an_unknown_catalogue_id_or_a_bad_catalogue_file_is_an_error() {
    let dir = temp();
    let result = soundbank(dir.path(), &["install", "no-such-bank", "--json"]);
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["kind"], json!("sound_bank"));
    let broken = dir.path().join("broken.json");
    fs::write(&broken, b"{\"not\": \"a list\"}").unwrap();
    let result = soundbank(
        dir.path(),
        &["catalogue", "--catalogue", path_str(&broken), "--json"],
    );
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["kind"], json!("invalid_input"));
    let gone = dir.path().join("gone.json");
    let result = soundbank(
        dir.path(),
        &["catalogue", "--catalogue", path_str(&gone), "--json"],
    );
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["kind"], json!("io"));
    let result = soundbank(dir.path(), &["install", "Bad_Id", "--json"]);
    assert_eq!(result.code, 1);
    assert_eq!(result.json()["error"]["kind"], json!("usage"));
}

#[test]
fn remove_deletes_a_bank_and_refuses_an_unknown_one() {
    let dir = temp();
    let file = dir.path().join("Warm Piano.sf2");
    fs::write(&file, tiny_sf2()).unwrap();
    let imported = soundbank(dir.path(), &["import", path_str(&file), "--json"]).json();
    let folder = PathBuf::from(imported["imported"]["path"].as_str().unwrap());
    assert!(folder.exists());
    let result = soundbank(dir.path(), &["remove", "warm-piano", "--json"]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(result.json(), json!({"removed": "warm-piano"}));
    assert!(!folder.exists());
    let listed = soundbank(dir.path(), &["list", "--json"]).json();
    assert_eq!(listed["banks"], json!([]));
    let again = soundbank(dir.path(), &["remove", "warm-piano", "--json"]);
    assert_eq!(again.code, 2);
    assert_eq!(again.json()["error"]["kind"], json!("sound_bank"));
}

#[test]
fn assign_maps_an_instrument_family_to_a_bank() {
    let dir = temp();
    let file = dir.path().join("Warm Piano.sf2");
    fs::write(&file, tiny_sf2()).unwrap();
    soundbank(dir.path(), &["import", path_str(&file)]);
    let result = soundbank(
        dir.path(),
        &["assign", "Synth-Lead", "warm-piano", "--json"],
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        result.json(),
        json!({"assigned": {"instrument": "synth_lead", "bank": "warm-piano"}})
    );
    let drums = soundbank(dir.path(), &["assign", "drums", "built-in"]);
    assert_eq!(drums.code, 0, "{}", drums.stderr);
    assert_eq!(drums.stdout.trim(), "drums will use built-in");
    let listed = soundbank(dir.path(), &["list", "--json"]).json();
    assert_eq!(
        listed["assignments"],
        json!({"drums": "built-in", "synth_lead": "warm-piano"})
    );
}

#[test]
fn assign_refuses_an_unknown_bank_and_an_unknown_instrument() {
    let dir = temp();
    let result = soundbank(dir.path(), &["assign", "guitar", "nothing-here", "--json"]);
    assert_eq!(result.code, 2);
    assert_eq!(result.json()["error"]["kind"], json!("sound_bank"));
    let result = soundbank(dir.path(), &["assign", "banjo", "built-in", "--json"]);
    assert_eq!(result.code, 1);
    let error = &result.json()["error"];
    assert_eq!(error["kind"], json!("usage"));
    assert!(
        error["message"].as_str().unwrap().contains("guitar"),
        "{error}"
    );
    let listed = soundbank(dir.path(), &["list", "--json"]).json();
    assert_eq!(listed["assignments"], json!({}));
}

#[test]
fn a_damaged_registry_is_reported_and_playback_goes_on_with_nothing_installed() {
    let dir = temp();
    fs::write(dir.path().join("banks.json"), b"{ damaged").unwrap();
    let listed = soundbank(dir.path(), &["list", "--json"]);
    assert_eq!(listed.code, 0, "{}", listed.stderr);
    let body = listed.json();
    assert_eq!(body["banks"], json!([]));
    assert_eq!(body["problems"].as_array().unwrap().len(), 1);
    assert!(dir.path().join("banks.json.damaged").is_file());
}
