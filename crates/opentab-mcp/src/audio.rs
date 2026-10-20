use opentab_audio::{
    BarRange, RENDER_TAIL_MS, SampleRate, ScriptStep, TransportCommand, encode_wav, range_frames,
    render_session,
};
use serde::Serialize;

use crate::error::ToolError;
use crate::report::Details;
use crate::sound::Prepared;

pub const RENDER_LIMIT_SECONDS: u64 = 60;
pub const WAV_MIME: &str = "audio/wav";

#[derive(Serialize)]
pub struct AudioReport {
    pub seconds: f64,
    pub frames: u64,
    pub sample_rate: u32,
    pub bytes: usize,
    #[serde(flatten)]
    pub details: Details,
    pub played_bars: Vec<usize>,
}

pub struct Wav {
    pub bytes: Vec<u8>,
    pub report: AudioReport,
}

pub fn limit_frames(rate: SampleRate) -> u64 {
    u64::from(rate.hz()) * RENDER_LIMIT_SECONDS
}

pub fn within_limit(frames: u64, rate: SampleRate) -> bool {
    frames <= limit_frames(rate)
}

pub fn too_long(seconds: f64) -> ToolError {
    ToolError::invalid_input(format!(
        "the render would last {seconds:.1} seconds with its {} second tail; render_audio is limited to {RENDER_LIMIT_SECONDS} seconds, so pick fewer bars with bars",
        RENDER_TAIL_MS / 1000
    ))
}

fn music_frames(prepared: &Prepared) -> Result<u64, ToolError> {
    let performance = prepared.session.transport.performance();
    let range = match prepared.range {
        Some(range) => range,
        None => match performance.order.bars.len().checked_sub(1) {
            Some(last) => BarRange { first: 0, last },
            None => return Ok(0),
        },
    };
    Ok(range_frames(
        performance,
        range,
        prepared.speed,
        prepared.session.transport.rate(),
    )?)
}

pub fn render(mut prepared: Prepared) -> Result<Wav, ToolError> {
    let rate = prepared.session.transport.rate();
    let music = music_frames(&prepared)?;
    let total = music.saturating_add(rate.frames_of_millis(RENDER_TAIL_MS));
    if !within_limit(total, rate) {
        return Err(too_long(total as f64 / f64::from(rate.hz())));
    }
    let mut script: Vec<ScriptStep> = prepared
        .commands()
        .into_iter()
        .map(|command| ScriptStep { frame: 0, command })
        .collect();
    if prepared.range.is_some() {
        script.push(ScriptStep {
            frame: music,
            command: TransportCommand::Pause,
        });
    }
    let audio = render_session(&mut prepared.session, &script, total)?;
    let bytes = encode_wav(&audio);
    let backends = prepared.backends.clone();
    let report = prepared.session.report(backends);
    let details = prepared.details(&report.backends);
    let played_bars = prepared.source_bars(&report.played_bars);
    Ok(Wav {
        report: AudioReport {
            seconds: audio.frames as f64 / f64::from(rate.hz()),
            frames: audio.frames,
            sample_rate: rate.hz(),
            bytes: bytes.len(),
            details,
            played_bars,
        },
        bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limit_is_sixty_seconds_of_frames_and_the_boundary_is_inside() {
        let rate = SampleRate::default();
        assert_eq!(limit_frames(rate), 60 * u64::from(rate.hz()));
        assert!(within_limit(limit_frames(rate), rate));
        assert!(!within_limit(limit_frames(rate) + 1, rate));
        assert!(within_limit(0, rate));
    }

    #[test]
    fn the_refusal_names_the_cap_the_length_and_the_remedy() {
        let error = too_long(67.04);
        assert_eq!(error.kind, "invalid_input");
        assert!(error.message.contains("67.0 seconds"));
        assert!(error.message.contains("limited to 60 seconds"));
        assert!(error.message.contains("3 second tail"));
        assert!(error.message.contains("pick fewer bars"));
    }
}
