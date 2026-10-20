use opentab_audio::{
    BankFormat, BankRef, BarRange, SampleRate, Session, SessionConfig, TrackBackend, played_range,
};
use opentab_files as files;
use opentab_model::{Score, SoundOverride};
use opentab_playback::{
    PlaybackOptions, PlaybackWarning, Profile, Speed, UnplayedFeature, UnplayedKind, perform,
};
use opentab_render::{BarSpan, SelectionError};
use serde::Serialize;

use crate::bank_store;
use crate::cli::PlaybackArgs;
use crate::error::CliError;
use crate::render::selection_failure;
use crate::text::escape_controls;

#[derive(Clone, Copy, PartialEq, Eq, Debug, clap::ValueEnum)]
pub enum ProfileChoice {
    Gp,
    Musical,
}

impl ProfileChoice {
    pub fn profile(self) -> Profile {
        match self {
            ProfileChoice::Gp => Profile::Gp,
            ProfileChoice::Musical => Profile::Musical,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ProfileChoice::Gp => "gp",
            ProfileChoice::Musical => "musical",
        }
    }
}

#[derive(Serialize)]
pub struct BackendWire {
    track: usize,
    format: &'static str,
    bank: String,
    fallback: Option<String>,
}

#[derive(Serialize)]
pub struct UnplayedWire {
    kind: &'static str,
    track: Option<usize>,
    first_bar: usize,
    count: u32,
}

#[derive(Serialize)]
pub struct WarningWire {
    kind: &'static str,
    track: usize,
    at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<u32>,
}

#[derive(Serialize, Clone, Copy)]
pub struct BarsWire {
    first: u32,
    last: u32,
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

pub struct Prepared {
    pub session: Session,
    pub backends: Vec<TrackBackend>,
    pub names: Vec<String>,
    pub range: Option<BarRange>,
    pub tracks: Vec<usize>,
    pub choice: ProfileChoice,
    pub speed: Speed,
    pub bars: Option<BarSpan>,
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

pub fn unplayed_lines(features: &[UnplayedFeature]) -> Vec<String> {
    features
        .iter()
        .map(|feature| {
            let place = match feature.track {
                Some(track) => format!("track {}, from bar {}", track + 1, feature.first_bar + 1),
                None => format!("from bar {}", feature.first_bar + 1),
            };
            format!(
                "not played: {} x{} ({place})",
                kind_name(feature.kind),
                feature.count
            )
        })
        .collect()
}

impl Prepared {
    pub fn details(&self, backends: &[TrackBackend]) -> Details {
        let performance = self.session.transport.performance();
        Details {
            profile: self.choice.name(),
            speed: self.speed.percent(),
            tracks: self.tracks.iter().map(|track| track + 1).collect(),
            bars: self.bars.map(|span| BarsWire {
                first: span.first,
                last: span.last,
            }),
            backends: backends_wire(backends),
            unplayed: unplayed_wire(&performance.unplayed),
            warnings: warnings_wire(&performance.warnings),
        }
    }

    pub fn source_bars(&self, played: &[usize]) -> Vec<usize> {
        let order = &self.session.transport.performance().order;
        played
            .iter()
            .filter_map(|index| order.bars.get(*index))
            .map(|bar| bar.source + 1)
            .collect()
    }

    pub fn lines(&self, backends: &[TrackBackend]) -> Vec<String> {
        let mut lines = Vec::new();
        for backend in backends {
            let name = self
                .names
                .get(backend.track)
                .map(|name| escape_controls(name))
                .unwrap_or_default();
            let mut line = format!(
                "  track {} {name}: {} ({})",
                backend.track + 1,
                backend.bank,
                format_name(backend.format)
            );
            if let Some(fallback) = &backend.fallback {
                line.push_str(&format!(
                    "; fell back to the built-in synthesizer: {}",
                    escape_controls(fallback)
                ));
            }
            lines.push(line);
        }
        lines.extend(unplayed_lines(
            &self.session.transport.performance().unplayed,
        ));
        lines
    }
}

fn select_tracks(score: &Score, chosen: &[u32]) -> Result<Vec<usize>, CliError> {
    let count = score.tracks.len();
    let mut indices = Vec::new();
    for number in chosen {
        let index = usize::try_from(*number - 1).unwrap_or(usize::MAX);
        if index >= count {
            return Err(selection_failure(SelectionError::Track {
                number: *number,
                count,
            }));
        }
        indices.push(index);
    }
    indices.sort_unstable();
    indices.dedup();
    if indices.is_empty() {
        indices.extend(0..count);
    }
    Ok(indices)
}

fn source_range(
    score: &Score,
    performance: &opentab_playback::Performance,
    span: BarSpan,
) -> Result<BarRange, CliError> {
    let count = score.master_bars.len();
    let last = usize::try_from(span.last).unwrap_or(usize::MAX);
    if last > count {
        return Err(selection_failure(SelectionError::Bars {
            first: span.first,
            last: span.last,
            count,
        }));
    }
    let sources = BarRange {
        first: usize::try_from(span.first - 1).unwrap_or(usize::MAX),
        last: last - 1,
    };
    played_range(&performance.order, sources).ok_or_else(|| CliError::InvalidInput {
        message: format!(
            "--bars {}: none of these bars is ever played",
            if span.first == span.last {
                span.first.to_string()
            } else {
                format!("{}-{}", span.first, span.last)
            }
        ),
    })
}

fn force_bank(score: &mut Score, bank: &BankRef) {
    score.sound_overrides = score
        .tracks
        .iter()
        .map(|track| SoundOverride {
            track: track.id,
            bank: bank.to_string(),
        })
        .collect();
}

pub fn prepare(args: &PlaybackArgs, rate: SampleRate) -> Result<Prepared, CliError> {
    let mut score = files::read_score(&args.path)?;
    let tracks = select_tracks(&score, &args.tracks)?;
    let profile = args.profile.profile();
    let performance = perform(&score, &PlaybackOptions { profile })?;
    let range = match args.bars {
        Some(span) => Some(source_range(&score, &performance, span)?),
        None => None,
    };
    let speed = Speed::new(args.speed)?;
    let manager = bank_store::open(None)?;
    if let Some(bank) = &args.soundbank {
        force_bank(&mut score, bank);
    }
    let plan = manager.resolve(&score);
    let loaded = manager.load_voices(&plan, rate);
    let config = SessionConfig {
        rate,
        speed,
        metronome: args.metronome,
        count_in: args.count_in,
    };
    let session = Session::new(performance, loaded.voices, config)?;
    Ok(Prepared {
        session,
        backends: loaded.backends,
        names: score
            .tracks
            .iter()
            .map(|track| track.name.clone())
            .collect(),
        range,
        tracks,
        choice: args.profile,
        speed,
        bars: args.bars,
    })
}
