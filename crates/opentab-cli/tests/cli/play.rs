use serde_json::json;

use crate::support::{gp_fixture, path_str, run_in, temp};

#[cfg(not(feature = "live"))]
#[test]
fn play_without_the_live_feature_is_a_clear_error() {
    let dir = temp();
    let input = gp_fixture("repeats.gp");
    let result = run_in(dir.path(), &["play", path_str(&input), "--json"]);
    assert_eq!(result.code, 2);
    let error = &result.json()["error"];
    assert_eq!(error["kind"], json!("live_unavailable"));
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("--features live"), "{message}");
    assert!(message.contains("opentab audio"), "{message}");
    let human = run_in(dir.path(), &["play", path_str(&input)]);
    assert_eq!(human.code, 2);
    assert!(human.stderr.starts_with("error: "), "{}", human.stderr);
}

#[cfg(not(feature = "live"))]
#[test]
fn play_takes_the_options_of_audio_but_not_out() {
    let dir = temp();
    let input = gp_fixture("repeats.gp");
    let accepted = run_in(
        dir.path(),
        &[
            "play",
            path_str(&input),
            "--track",
            "1",
            "--bars",
            "1-2",
            "--speed",
            "50",
            "--metronome",
            "--count-in",
            "--profile",
            "gp",
            "--soundbank",
            "built-in",
            "--json",
        ],
    );
    assert_eq!(accepted.code, 2, "{}", accepted.stderr);
    assert_eq!(accepted.json()["error"]["kind"], json!("live_unavailable"));
    let refused = run_in(
        dir.path(),
        &["play", path_str(&input), "--out", "a.wav", "--json"],
    );
    assert_eq!(refused.code, 1);
    assert_eq!(refused.json()["error"]["kind"], json!("usage"));
}
