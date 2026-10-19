use opentab_audio::{BuiltInSynth, SampleRate, SoundingVoice, Synth};
use opentab_playback::{Channel, MidiMessage};

const VOICES: usize = 32;
const STEEL: u8 = 25;
const FIRST_KEY: u8 = 30;
const VELOCITY: u8 = 100;
const ALL_SOUND_OFF: u8 = 120;
const ALL_NOTES_OFF: u8 = 123;
const RELEASE_FRAMES: usize = 4_800;

fn send(synth: &mut BuiltInSynth, channel: u8, message: MidiMessage) {
    synth.handle(Channel::new(channel).expect("channel"), &message);
}

fn on(synth: &mut BuiltInSynth, channel: u8, key: u8) {
    send(
        synth,
        channel,
        MidiMessage::NoteOn {
            key,
            velocity: VELOCITY,
        },
    );
}

fn off(synth: &mut BuiltInSynth, channel: u8, key: u8) {
    send(synth, channel, MidiMessage::NoteOff { key, velocity: 64 });
}

fn control(synth: &mut BuiltInSynth, channel: u8, controller: u8) {
    send(
        synth,
        channel,
        MidiMessage::Control {
            controller,
            value: 0,
        },
    );
}

fn advance(synth: &mut BuiltInSynth, frames: usize) {
    let mut left = vec![0.0; frames];
    let mut right = vec![0.0; frames];
    synth.render(&mut left, &mut right);
}

fn fresh() -> BuiltInSynth {
    let mut synth = BuiltInSynth::new(SampleRate::STUDIO);
    for channel in 0..2 {
        send(&mut synth, channel, MidiMessage::Program { program: STEEL });
    }
    synth
}

fn keys(voices: &[SoundingVoice], channel: u8) -> Vec<u8> {
    let mut keys: Vec<u8> = voices
        .iter()
        .filter(|voice| voice.channel == channel)
        .map(|voice| voice.key)
        .collect();
    keys.sort_unstable();
    keys
}

fn held(count: u8) -> BuiltInSynth {
    let mut synth = fresh();
    for offset in 0..count {
        on(&mut synth, 0, FIRST_KEY + offset);
        advance(&mut synth, 16);
    }
    synth
}

#[test]
fn a_33rd_note_steals_the_oldest_held_voice() {
    let mut synth = held(32);
    assert_eq!(synth.sounding(), VOICES);
    on(&mut synth, 0, 90);
    let expected: Vec<u8> = (FIRST_KEY + 1..FIRST_KEY + 32).chain([90]).collect();
    assert_eq!(keys(&synth.voices(), 0), expected);
    assert_eq!(synth.sounding(), VOICES);
    let slot = synth
        .voices()
        .iter()
        .find(|voice| voice.key == 90)
        .expect("new")
        .slot;
    assert_eq!(slot, 0);
}

#[test]
fn a_released_voice_is_stolen_before_any_held_one() {
    let mut synth = held(32);
    off(&mut synth, 0, FIRST_KEY + 20);
    off(&mut synth, 0, FIRST_KEY + 10);
    on(&mut synth, 0, 90);
    let voices = synth.voices();
    assert!(voices.iter().all(|voice| voice.key != FIRST_KEY + 10));
    assert!(
        voices
            .iter()
            .any(|voice| voice.key == FIRST_KEY + 20 && voice.released)
    );
    assert!(voices.iter().any(|voice| voice.key == FIRST_KEY));
    assert_eq!(synth.sounding(), VOICES);
}

#[test]
fn a_restrike_without_note_off_releases_the_earlier_voice() {
    let mut synth = fresh();
    on(&mut synth, 0, 60);
    advance(&mut synth, 480);
    on(&mut synth, 0, 60);
    let voices = synth.voices();
    assert_eq!(voices.len(), 2);
    assert_eq!(voices.iter().filter(|voice| voice.released).count(), 1);
    advance(&mut synth, RELEASE_FRAMES);
    let voices = synth.voices();
    assert_eq!(voices.len(), 1);
    assert!(!voices[0].released);
}

#[test]
fn a_note_off_stops_only_its_own_channel() {
    let mut synth = fresh();
    on(&mut synth, 0, 60);
    on(&mut synth, 1, 60);
    off(&mut synth, 1, 60);
    let voices = synth.voices();
    assert!(
        voices
            .iter()
            .any(|voice| voice.channel == 0 && voice.key == 60 && !voice.released)
    );
    assert!(
        voices
            .iter()
            .any(|voice| voice.channel == 1 && voice.released)
    );
    advance(&mut synth, RELEASE_FRAMES);
    assert_eq!(keys(&synth.voices(), 0), vec![60]);
    assert!(keys(&synth.voices(), 1).is_empty());
}

#[test]
fn all_notes_off_releases_and_all_sound_off_frees_one_channel_only() {
    let mut synth = fresh();
    for channel in 0..2 {
        on(&mut synth, channel, 60);
        on(&mut synth, channel, 64);
    }
    control(&mut synth, 0, ALL_NOTES_OFF);
    let voices = synth.voices();
    assert!(
        voices
            .iter()
            .filter(|voice| voice.channel == 0)
            .all(|voice| voice.released)
    );
    assert!(
        voices
            .iter()
            .filter(|voice| voice.channel == 1)
            .all(|voice| !voice.released)
    );
    assert_eq!(voices.len(), 4);
    control(&mut synth, 1, ALL_SOUND_OFF);
    let voices = synth.voices();
    assert_eq!(keys(&voices, 0), vec![60, 64]);
    assert!(keys(&voices, 1).is_empty());
    control(&mut synth, 0, ALL_SOUND_OFF);
    assert_eq!(synth.sounding(), 0);
}
