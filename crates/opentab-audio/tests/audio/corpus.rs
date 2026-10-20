use std::path::PathBuf;

use opentab_audio::{
    Bank, BarRange, BuiltInBank, SampleRate, ScriptStep, Session, TransportCommand, frame_at,
    render_session,
};
use opentab_gp::read_gp;
use opentab_playback::{MidiMessage, Speed, export_midi};

use crate::recording::{heard, notes, recorders};
use crate::support::{
    PROFILES, bank_voices, config, performance, profile_name, render_frames, tiny_bank,
    tiny_sfz_bank,
};

const CORPUS_VARIABLE: &str = "OPENTAB_CORPUS";
const EXPECTED_FILES: usize = 27;
const PLAYED_BARS: usize = 8;
const TAIL_MS: u32 = 1000;

fn corpus() -> Option<Vec<PathBuf>> {
    let dir = PathBuf::from(std::env::var_os(CORPUS_VARIABLE)?);
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("corpus directory")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "gp"))
        .collect();
    paths.sort();
    Some(paths)
}

#[test]
fn every_corpus_example_renders_its_opening_bars_in_both_profiles() {
    render_corpus(&BuiltInBank);
}

#[test]
fn every_corpus_example_renders_its_opening_bars_with_the_tiny_sf2() {
    let bank = tiny_bank();
    render_corpus(&bank);
    assert!(bank.faults().is_empty(), "{:?}", bank.faults());
}

#[test]
fn every_corpus_example_renders_its_opening_bars_with_the_tiny_sfz() {
    let bank = tiny_sfz_bank();
    render_corpus(&bank);
    assert!(bank.faults().is_empty(), "{:?}", bank.faults());
}

fn render_corpus(bank: &dyn Bank) {
    let Some(paths) = corpus() else {
        return;
    };
    assert_eq!(paths.len(), EXPECTED_FILES);
    let rate = SampleRate::STUDIO;
    let mut rendered = 0;
    for path in &paths {
        let score = read_gp(&std::fs::read(path).expect("read")).expect("parse");
        for profile in PROFILES {
            let performance = performance(&score, profile);
            let cut = performance
                .order
                .bars
                .get(PLAYED_BARS)
                .map_or(performance.order.end(), |bar| bar.start);
            let frames = frame_at(&performance.tempo, cut, Speed::NORMAL, rate).expect("frame")
                + rate.frames_of_millis(TAIL_MS);
            let audio = render_frames(
                &performance,
                bank_voices(bank, &performance, rate),
                rate,
                frames,
            )
            .expect("render");
            let samples = audio.left.iter().chain(&audio.right);
            assert!(
                samples.clone().all(|sample| sample.is_finite()),
                "{}",
                path.display()
            );
            let peak = samples.fold(0.0f32, |peak, sample| peak.max(sample.abs()));
            assert!(
                peak > 0.0,
                "{} {} is silent",
                path.display(),
                profile_name(profile)
            );
            println!(
                "{:?} {} {} tracks {} frames {} peak {peak}",
                bank.format(),
                path.file_name().expect("name").to_string_lossy(),
                profile_name(profile),
                performance.tracks.len(),
                audio.frames
            );
            rendered += 1;
        }
    }
    assert_eq!(rendered, 2 * EXPECTED_FILES);
}

#[test]
fn every_corpus_example_plays_paired_notes_through_the_transport_and_exports_midi() {
    let Some(paths) = corpus() else {
        return;
    };
    assert_eq!(paths.len(), EXPECTED_FILES);
    let rate = SampleRate::STUDIO;
    let mut checked = 0;
    for path in &paths {
        let score = read_gp(&std::fs::read(path).expect("read")).expect("parse");
        for profile in PROFILES {
            let performance = performance(&score, profile);
            let bars = performance.order.bars.len();
            let cut = performance
                .order
                .bars
                .get(PLAYED_BARS)
                .map_or(performance.order.end(), |bar| bar.start);
            let frames = frame_at(&performance.tempo, cut, Speed::NORMAL, rate).expect("frame")
                + rate.frames_of_millis(TAIL_MS);
            let seek = bars.saturating_sub(1).min(2);
            let first = bars.saturating_sub(1).min(1);
            let last = bars.saturating_sub(1).min(first + 1);
            let script = [
                ScriptStep {
                    frame: frames / 4,
                    command: TransportCommand::Seek(seek),
                },
                ScriptStep {
                    frame: frames / 2,
                    command: TransportCommand::Loop(BarRange { first, last }),
                },
                ScriptStep {
                    frame: frames - 1,
                    command: TransportCommand::Stop,
                },
            ];
            let recorded = recorders(&performance);
            let mut session =
                Session::new(performance.clone(), recorded.voices, config(rate)).expect("session");
            render_session(&mut session, &script, frames).expect("render");
            let mut started = 0;
            for log in &recorded.logs {
                for note in notes(&heard(log)) {
                    assert!(
                        note.off.is_some(),
                        "{} {}: {note:?} is left on",
                        path.display(),
                        profile_name(profile)
                    );
                    started += 1;
                }
            }
            assert!(started > 0, "{} plays no note", path.display());
            let smf = export_midi(&score, profile).expect("export");
            let parsed = midly::Smf::parse(&smf).expect("the exported MIDI reparses");
            let exported = parsed
                .tracks
                .iter()
                .flatten()
                .filter(|event| {
                    matches!(
                        event.kind,
                        midly::TrackEventKind::Midi {
                            message: midly::MidiMessage::NoteOn { vel, .. },
                            ..
                        } if vel.as_int() > 0
                    )
                })
                .count();
            let performed = performance
                .events
                .iter()
                .filter(|event| matches!(event.message, MidiMessage::NoteOn { velocity, .. } if velocity > 0))
                .count();
            assert_eq!(exported, performed, "{}", path.display());
            checked += 1;
        }
    }
    assert_eq!(checked, 2 * EXPECTED_FILES);
}
