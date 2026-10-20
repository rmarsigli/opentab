use opentab_audio::{BankFormat, TrackBackend};
use opentab_playback::{PlaybackWarning, UnplayedFeature, UnplayedKind};
use serde::Serialize;

#[derive(Serialize)]
pub struct BackendWire {
    pub track: usize,
    pub format: &'static str,
    pub bank: String,
    pub fallback: Option<String>,
}

#[derive(Serialize)]
pub struct UnplayedWire {
    pub kind: &'static str,
    pub track: Option<usize>,
    pub first_bar: usize,
    pub count: u32,
}

#[derive(Serialize)]
pub struct WarningWire {
    pub kind: &'static str,
    pub track: usize,
    pub at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<u32>,
}

#[derive(Serialize, Clone, Copy)]
pub struct BarsWire {
    pub first: u32,
    pub last: u32,
}

#[derive(Serialize)]
pub struct Details {
    pub profile: &'static str,
    pub speed: u16,
    pub tracks: Vec<usize>,
    pub bars: Option<BarsWire>,
    pub backends: Vec<BackendWire>,
    pub unplayed: Vec<UnplayedWire>,
    pub warnings: Vec<WarningWire>,
}

pub fn format_name(format: BankFormat) -> &'static str {
    match format {
        BankFormat::BuiltIn => "built_in",
        BankFormat::Sf2 => "sf2",
        BankFormat::Sfz => "sfz",
    }
}

pub fn kind_name(kind: UnplayedKind) -> &'static str {
    match kind {
        UnplayedKind::Ghost => "ghost",
        UnplayedKind::Accent => "accent",
        UnplayedKind::Dead => "dead",
        UnplayedKind::PalmMute => "palm_mute",
        UnplayedKind::LetRing => "let_ring",
        UnplayedKind::Vibrato => "vibrato",
        UnplayedKind::HammerOnPullOff => "hammer_on_pull_off",
        UnplayedKind::Slide => "slide",
        UnplayedKind::Tap => "tap",
        UnplayedKind::DipBend => "dip_bend",
        UnplayedKind::OffNodeHarmonic => "off_node_harmonic",
        UnplayedKind::TremoloBar => "tremolo_bar",
        UnplayedKind::Stroke => "stroke",
        UnplayedKind::FadeIn => "fade_in",
        UnplayedKind::Slap => "slap",
        UnplayedKind::Pop => "pop",
        UnplayedKind::TempoRamp => "tempo_ramp",
        UnplayedKind::SoundChange => "sound_change",
        UnplayedKind::Volume => "volume",
        UnplayedKind::Pan => "pan",
        UnplayedKind::Bank => "bank",
        UnplayedKind::OneChannelPerString => "one_channel_per_string",
        UnplayedKind::UnreadableDrumKit => "unreadable_drum_kit",
    }
}

pub fn unplayed_wire(features: &[UnplayedFeature]) -> Vec<UnplayedWire> {
    features
        .iter()
        .map(|feature| UnplayedWire {
            kind: kind_name(feature.kind),
            track: feature.track.map(|track| track + 1),
            first_bar: feature.first_bar + 1,
            count: feature.count,
        })
        .collect()
}

pub fn warnings_wire(warnings: &[PlaybackWarning]) -> Vec<WarningWire> {
    warnings
        .iter()
        .map(|warning| match *warning {
            PlaybackWarning::ChannelFallback { track, at } => WarningWire {
                kind: "channel_fallback",
                track: track + 1,
                at: at.whole_notes().to_string(),
                detail: None,
            },
            PlaybackWarning::PitchOutOfRange { track, at, pitch } => WarningWire {
                kind: "pitch_out_of_range",
                track: track + 1,
                at: at.whole_notes().to_string(),
                detail: Some(u32::from(pitch)),
            },
            PlaybackWarning::MissingString { track, at, string } => WarningWire {
                kind: "missing_string",
                track: track + 1,
                at: at.whole_notes().to_string(),
                detail: Some(u32::from(string)),
            },
            PlaybackWarning::UnknownArticulation {
                track,
                at,
                articulation,
            } => WarningWire {
                kind: "unknown_articulation",
                track: track + 1,
                at: at.whole_notes().to_string(),
                detail: Some(u32::from(articulation)),
            },
            PlaybackWarning::MergedUnison { track, at } => WarningWire {
                kind: "merged_unison",
                track: track + 1,
                at: at.whole_notes().to_string(),
                detail: None,
            },
        })
        .collect()
}

pub fn backends_wire(backends: &[TrackBackend]) -> Vec<BackendWire> {
    backends
        .iter()
        .map(|backend| BackendWire {
            track: backend.track + 1,
            format: format_name(backend.format),
            bank: backend.bank.clone(),
            fallback: backend.fallback.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_and_bars_are_numbered_from_one_on_the_wire() {
        let wire = unplayed_wire(&[UnplayedFeature {
            kind: UnplayedKind::PalmMute,
            track: Some(0),
            first_bar: 2,
            count: 5,
        }]);
        assert_eq!(wire[0].kind, "palm_mute");
        assert_eq!(wire[0].track, Some(1));
        assert_eq!(wire[0].first_bar, 3);
        let backends = backends_wire(&[TrackBackend::built_in(3)]);
        assert_eq!(backends[0].track, 4);
        assert_eq!(backends[0].format, "built_in");
        assert_eq!(backends[0].bank, "built-in");
    }
}
