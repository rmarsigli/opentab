use std::sync::{Arc, Mutex};

use opentab_audio::{
    AudioError, DRUM_DECAY_MS, MELODIC_RELEASE_MS, RENDER_TAIL_MS, SampleRate, Scheduler, Synth,
    full_length,
};
use opentab_playback::{Channel, MidiMessage, Performance, Profile, Speed};

use crate::support::{
    PROFILES, built_in_voices, fixture, performance, render, render_all, render_frames,
};

const SILENCE_FIXTURES: [&str; 6] = [
    "note-effects.gp",
    "drums.gp",
    "multi-track.gp",
    "tremolo-bar.gp",
    "trill.gp",
    "mixer.gp",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Heard {
    frame: u64,
    channel: Channel,
    message: MidiMessage,
}

#[derive(Debug, Default)]
struct Recorder {
    rendered: u64,
    heard: Vec<Heard>,
}

struct RecordingSynth {
    log: Arc<Mutex<Recorder>>,
}

impl Synth for RecordingSynth {
    fn reset(&mut self) {
        let mut log = self.log.lock().expect("log");
        log.rendered = 0;
        log.heard.clear();
    }

    fn handle(&mut self, channel: Channel, message: &MidiMessage) {
        let mut log = self.log.lock().expect("log");
        let frame = log.rendered;
        log.heard.push(Heard {
            frame,
            channel,
            message: *message,
        });
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        right.fill(0.0);
        self.log.lock().expect("log").rendered += left.len() as u64;
    }
}

struct Recorders {
    logs: Vec<Arc<Mutex<Recorder>>>,
    voices: Vec<Box<dyn Synth>>,
}

fn recorders(performance: &Performance) -> Recorders {
    let logs: Vec<Arc<Mutex<Recorder>>> = performance
        .tracks
        .iter()
        .map(|_| Arc::new(Mutex::new(Recorder::default())))
        .collect();
    let voices = logs
        .iter()
        .map(|log| {
            Box::new(RecordingSynth {
                log: Arc::clone(log),
            }) as Box<dyn Synth>
        })
        .collect();
    Recorders { logs, voices }
}

#[test]
fn every_track_hears_its_own_events_at_their_exact_frames() {
    for name in ["multi-track.gp", "tempo-ramp.gp", "drums.gp"] {
        for profile in PROFILES {
            let performance = performance(&fixture(name), profile);
            let rate = SampleRate::CD;
            let Recorders { logs, voices } = recorders(&performance);
            let audio = render_all(&performance, voices, rate).expect("render");
            assert_eq!(
                audio.frames,
                full_length(&performance, rate).expect("length")
            );
            let scheduler = Scheduler::new(&performance, Speed::NORMAL, rate).expect("schedule");
            for (track, log) in logs.iter().enumerate() {
                let expected: Vec<Heard> = scheduler
                    .events()
                    .iter()
                    .filter(|event| event.track == track)
                    .map(|event| Heard {
                        frame: event.frame,
                        channel: event.channel,
                        message: event.message,
                    })
                    .collect();
                let log = log.lock().expect("log");
                assert!(!expected.is_empty(), "{name} track {track}");
                assert_eq!(log.heard, expected, "{name} track {track}");
                assert_eq!(log.rendered, audio.frames);
            }
        }
    }
}

#[test]
fn a_render_needs_one_synth_per_track() {
    let performance = performance(&fixture("multi-track.gp"), Profile::Musical);
    let rate = SampleRate::STUDIO;
    let mut voices = built_in_voices(&performance, rate);
    voices.pop();
    let error = render_all(&performance, voices, rate).expect_err("one short");
    assert_eq!(
        error,
        AudioError::VoiceCount {
            voices: performance.tracks.len() - 1,
            tracks: performance.tracks.len()
        }
    );
}

#[test]
fn the_render_ends_a_fixed_tail_after_the_score() {
    let performance = performance(&fixture("durations.gp"), Profile::Musical);
    let rate = SampleRate::STUDIO;
    let end = opentab_audio::frame_at(
        &performance.tempo,
        performance.order.end(),
        Speed::NORMAL,
        rate,
    )
    .expect("end");
    assert_eq!(
        full_length(&performance, rate).expect("length"),
        end + rate.frames_of_millis(RENDER_TAIL_MS)
    );
    let short =
        render_frames(&performance, built_in_voices(&performance, rate), rate, 100).expect("short");
    assert_eq!(
        (short.frames, short.left.len(), short.right.len()),
        (100, 100, 100)
    );
}

fn last_note_off(performance: &Performance, rate: SampleRate) -> u64 {
    Scheduler::new(performance, Speed::NORMAL, rate)
        .expect("schedule")
        .filter(|event| matches!(event.message, MidiMessage::NoteOff { .. }))
        .map(|event| event.frame)
        .max()
        .expect("a note-off")
}

#[test]
fn every_voice_falls_silent_after_the_last_note_off() {
    let rate = SampleRate::STUDIO;
    for name in SILENCE_FIXTURES {
        for profile in PROFILES {
            let performance = performance(&fixture(name), profile);
            let drums = performance.events.iter().any(|event| {
                event.channel == Channel::DRUMS
                    && matches!(event.message, MidiMessage::NoteOn { .. })
            });
            let release = if drums {
                DRUM_DECAY_MS
            } else {
                MELODIC_RELEASE_MS
            };
            let audio = render(&performance, rate);
            let quiet =
                usize::try_from(last_note_off(&performance, rate) + rate.frames_of_millis(release))
                    .expect("frame");
            assert!(quiet < audio.left.len(), "{name}");
            assert!(
                audio.left[..quiet].iter().any(|sample| *sample != 0.0),
                "{name} sounds"
            );
            let loud = audio.left[quiet..]
                .iter()
                .chain(&audio.right[quiet..])
                .filter(|sample| **sample != 0.0)
                .count();
            assert_eq!(loud, 0, "{name} {profile:?}");
        }
    }
}
