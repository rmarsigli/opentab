use std::path::Path;

use opentab_audio::{SampleRate, frame_at};
use opentab_files::write_bytes;
use opentab_model::Score;
use opentab_playback::{PlaybackOptions, Speed, perform, write_smf};
use serde::Serialize;

use crate::args::ExportMidiArgs;
use crate::error::ToolError;
use crate::report::{UnplayedWire, WarningWire, unplayed_wire, warnings_wire};
use crate::sound::{profile_name, profile_of};

pub const MIDI_SUFFIX: &str = ".mid";

#[derive(Serialize)]
pub struct MidiExported {
    pub exported: String,
    pub bytes: usize,
    pub profile: &'static str,
    pub ppq: u16,
    pub seconds: f64,
    pub played_bars: Vec<usize>,
    pub unplayed: Vec<UnplayedWire>,
    pub warnings: Vec<WarningWire>,
}

pub fn has_midi_suffix(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.len() > MIDI_SUFFIX.len()
                && name.is_char_boundary(name.len() - MIDI_SUFFIX.len())
                && name[name.len() - MIDI_SUFFIX.len()..].eq_ignore_ascii_case(MIDI_SUFFIX)
        })
}

pub fn export_midi(score: &Score, args: ExportMidiArgs) -> Result<MidiExported, ToolError> {
    let path = Path::new(&args.path);
    if !has_midi_suffix(path) {
        return Err(ToolError::new(
            "unsupported_format",
            format!(
                "{}: unsupported file type, supported formats: {MIDI_SUFFIX}",
                crate::error::echo(&args.path)
            ),
        ));
    }
    let profile = args.profile.unwrap_or_default();
    let performance = perform(
        score,
        &PlaybackOptions {
            profile: profile_of(profile),
        },
    )?;
    let ppq = profile_of(profile).ppq();
    let bytes = write_smf(&performance, ppq);
    write_bytes(path, &bytes)?;
    let rate = SampleRate::default();
    let frames = frame_at(
        &performance.tempo,
        performance.order.end(),
        Speed::NORMAL,
        rate,
    )?;
    Ok(MidiExported {
        exported: args.path,
        bytes: bytes.len(),
        profile: profile_name(profile),
        ppq: ppq.get(),
        seconds: frames as f64 / f64::from(rate.hz()),
        played_bars: performance
            .order
            .bars
            .iter()
            .map(|bar| bar.source + 1)
            .collect(),
        unplayed: unplayed_wire(&performance.unplayed),
        warnings: warnings_wire(&performance.warnings),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_named_mid_file_is_accepted_in_any_case() {
        for accepted in ["a.mid", "dir/song.MID", "x.y.Mid"] {
            assert!(has_midi_suffix(Path::new(accepted)), "{accepted}");
        }
        for refused in [".mid", "a.midi", "a.wav", "mid", "dir/", ""] {
            assert!(!has_midi_suffix(Path::new(refused)), "{refused}");
        }
    }
}
