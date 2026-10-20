use opentab_audio::{
    BarRange, SampleRate, Session, SessionConfig, TrackBackend, TransportCommand, played_range,
    range_frames,
};
use opentab_model::Score;
use opentab_playback::{
    MAX_SPEED, MIN_SPEED, Performance, PlaybackOptions, Profile, Speed, perform,
};
use opentab_render::{BarSpan, SelectionError, parse_bars};

use crate::args::{ProfileArg, SoundArgs};
use crate::banks::open_manager;
use crate::error::ToolError;
use crate::report::{BarsWire, Details, backends_wire, unplayed_wire, warnings_wire};

pub struct Planned {
    pub performance: Performance,
    pub range: Option<BarRange>,
    pub tracks: Vec<usize>,
    pub track_count: usize,
    pub profile: ProfileArg,
    pub speed: Speed,
    pub bars: Option<BarSpan>,
}

pub struct Prepared {
    pub session: Session,
    pub backends: Vec<TrackBackend>,
    pub range: Option<BarRange>,
    pub tracks: Vec<usize>,
    pub track_count: usize,
    pub profile: ProfileArg,
    pub speed: Speed,
    pub bars: Option<BarSpan>,
}

pub fn profile_of(arg: ProfileArg) -> Profile {
    match arg {
        ProfileArg::Gp => Profile::Gp,
        ProfileArg::Musical => Profile::Musical,
    }
}

pub fn profile_name(arg: ProfileArg) -> &'static str {
    match arg {
        ProfileArg::Gp => "gp",
        ProfileArg::Musical => "musical",
    }
}

fn selection_failure(error: SelectionError) -> ToolError {
    ToolError::invalid_input(error.to_string())
}

fn select_tracks(score: &Score, chosen: &[u32]) -> Result<Vec<usize>, ToolError> {
    let count = score.tracks.len();
    let mut indices = Vec::new();
    for number in chosen {
        let index = number
            .checked_sub(1)
            .and_then(|index| usize::try_from(index).ok())
            .filter(|index| *index < count)
            .ok_or_else(|| {
                selection_failure(SelectionError::Track {
                    number: *number,
                    count,
                })
            })?;
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
    performance: &Performance,
    span: BarSpan,
) -> Result<BarRange, ToolError> {
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
    played_range(&performance.order, sources).ok_or_else(|| {
        ToolError::invalid_input(format!(
            "bars {}: none of these bars is ever played",
            if span.first == span.last {
                span.first.to_string()
            } else {
                format!("{}-{}", span.first, span.last)
            }
        ))
    })
}

fn speed_of(percent: Option<u32>) -> Result<Speed, ToolError> {
    let Some(percent) = percent else {
        return Ok(Speed::NORMAL);
    };
    u16::try_from(percent)
        .ok()
        .and_then(|narrow| Speed::new(narrow).ok())
        .ok_or_else(|| {
            ToolError::invalid_input(format!(
                "speed {percent} % is outside {MIN_SPEED}..={MAX_SPEED} %"
            ))
        })
}

pub fn plan(score: &Score, args: SoundArgs) -> Result<Planned, ToolError> {
    let chosen = args
        .tracks
        .map(|tracks| tracks.numbers())
        .unwrap_or_default();
    let tracks = select_tracks(score, &chosen)?;
    let profile = args.profile.unwrap_or_default();
    let performance = perform(
        score,
        &PlaybackOptions {
            profile: profile_of(profile),
        },
    )?;
    let bars = match &args.bars {
        Some(bars) => Some(parse_bars(&bars.text()).map_err(ToolError::usage)?),
        None => None,
    };
    let range = match bars {
        Some(span) => Some(source_range(score, &performance, span)?),
        None => None,
    };
    let speed = speed_of(args.speed)?;
    Ok(Planned {
        performance,
        range,
        tracks,
        track_count: score.tracks.len(),
        profile,
        speed,
        bars,
    })
}

impl Planned {
    pub fn music_frames(&self, rate: SampleRate) -> Result<u64, ToolError> {
        let range = match self.range {
            Some(range) => range,
            None => match self.performance.order.bars.len().checked_sub(1) {
                Some(last) => BarRange { first: 0, last },
                None => return Ok(0),
            },
        };
        Ok(range_frames(&self.performance, range, self.speed, rate)?)
    }

    pub fn build(self, score: &Score, rate: SampleRate) -> Result<Prepared, ToolError> {
        let manager = open_manager()?;
        let plan = manager.resolve(score);
        let loaded = manager.load_voices(&plan, rate);
        let config = SessionConfig {
            rate,
            speed: self.speed,
            metronome: false,
            count_in: false,
        };
        let session = Session::new(self.performance, loaded.voices, config)?;
        Ok(Prepared {
            session,
            backends: loaded.backends,
            range: self.range,
            tracks: self.tracks,
            track_count: self.track_count,
            profile: self.profile,
            speed: self.speed,
            bars: self.bars,
        })
    }
}

pub fn prepare(score: &Score, args: SoundArgs, rate: SampleRate) -> Result<Prepared, ToolError> {
    plan(score, args)?.build(score, rate)
}

impl Prepared {
    pub fn details(&self, backends: &[TrackBackend]) -> Details {
        let performance = self.session.transport.performance();
        Details {
            profile: profile_name(self.profile),
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

    pub fn commands(&self) -> Vec<TransportCommand> {
        let mut commands = Vec::new();
        if self.tracks.len() < self.track_count {
            commands.extend(
                self.tracks
                    .iter()
                    .map(|track| TransportCommand::Solo(*track, true)),
            );
        }
        commands.push(
            self.range
                .map_or(TransportCommand::Play, TransportCommand::PlayRange),
        );
        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Score {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/gp/multi-track.gp");
        opentab_gp::read_gp(&std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn tracks_are_picked_from_one_sorted_and_deduplicated() {
        let score = fixture();
        let count = u32::try_from(score.tracks.len()).unwrap();
        assert!(count >= 2);
        assert_eq!(
            select_tracks(&score, &[count, 1, count]).unwrap(),
            vec![0, score.tracks.len() - 1]
        );
        assert_eq!(
            select_tracks(&score, &[]).unwrap(),
            (0..score.tracks.len()).collect::<Vec<_>>()
        );
        for bad in [0, count + 1] {
            let error = select_tracks(&score, &[bad]).unwrap_err();
            assert_eq!(error.kind, "invalid_input");
        }
    }
}
