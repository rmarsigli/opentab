use opentab_audio::{SampleRate, encode_wav};
use opentab_playback::Profile;

use crate::support::{fixture, performance, render};

const OUTPUT_VARIABLE: &str = "OPENTAB_WAV_OUT";
const FIXTURE_VARIABLE: &str = "OPENTAB_WAV_FIXTURE";
const PROFILE_VARIABLE: &str = "OPENTAB_WAV_PROFILE";
const DEFAULT_FIXTURE: &str = "note-effects.gp";

#[test]
#[ignore = "writes a WAV for listening when OPENTAB_WAV_OUT names a file"]
fn writes_a_fixture_wav_for_listening() {
    let Some(path) = std::env::var_os(OUTPUT_VARIABLE) else {
        return;
    };
    let name = std::env::var(FIXTURE_VARIABLE).unwrap_or_else(|_| DEFAULT_FIXTURE.to_owned());
    let profile = match std::env::var(PROFILE_VARIABLE).as_deref() {
        Ok("gp") => Profile::Gp,
        _ => Profile::Musical,
    };
    let audio = render(
        &performance(&fixture(&name), profile),
        SampleRate::default(),
    );
    std::fs::write(path, encode_wav(&audio)).expect("write wav");
}
