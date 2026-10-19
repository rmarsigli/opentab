use opentab_model::Score;

use crate::beats::{Click, beat_grid};
use crate::channels::{ChannelPool, gp_channels, pooled_channels};
use crate::controls::{Timeline, timeline};
use crate::emit::Musical;
use crate::exact::Exact;
use crate::midi::{
    BEND_RANGE_SEMITONES, Channel, ChannelSetup, ChannelState, ConductorEvent, ConductorKind,
    MidiMessage, PITCH_BEND_RANGE_RPN, TimedEvent, TrackSetup,
};
use crate::notes::{Lane, Stream, TrackWriter};
use crate::order::{PlayOrder, play_order};
use crate::smf::{Ppq, TrackLayout};
use crate::tempo::{JumpTempo, RampGrid, TempoMap};
use crate::unplayed::{UnplayedFeature, catalogue};
use crate::{PlaybackError, PlaybackWarning, ScoreTime};

const MICROS_PER_MINUTE: i128 = 60_000_000;
const OPENING_MESSAGES: usize = 5;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Profile {
    Gp,
    #[default]
    Musical,
}

impl Profile {
    pub fn ramp_grid(self) -> RampGrid {
        match self {
            Profile::Gp => RampGrid::GP,
            Profile::Musical => RampGrid::MUSICAL,
        }
    }

    pub fn jump_tempo(self) -> JumpTempo {
        match self {
            Profile::Gp => JumpTempo::HeldUntilBackward,
            Profile::Musical => JumpTempo::SourcePlace,
        }
    }

    pub fn ppq(self) -> Ppq {
        match self {
            Profile::Gp => Ppq::GP,
            Profile::Musical => Ppq::MUSICAL,
        }
    }

    pub fn track_layout(self) -> TrackLayout {
        match self {
            Profile::Gp => TrackLayout::PerStaff,
            Profile::Musical => TrackLayout::PerTrack,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct PlaybackOptions {
    pub profile: Profile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Performance {
    pub profile: Profile,
    pub order: PlayOrder,
    pub tempo: TempoMap,
    pub tracks: Vec<TrackSetup>,
    pub conductor: Vec<ConductorEvent>,
    pub events: Vec<TimedEvent>,
    pub clicks: Vec<Click>,
    pub unplayed: Vec<UnplayedFeature>,
    pub warnings: Vec<PlaybackWarning>,
}

fn conductor(
    score: &Score,
    order: &PlayOrder,
    tempo: &TempoMap,
) -> Result<Vec<ConductorEvent>, PlaybackError> {
    let mut points: Vec<ScoreTime> = tempo
        .segments()
        .iter()
        .map(|segment| segment.start)
        .collect();
    let mut previous: Option<usize> = None;
    for played in &order.bars {
        let Some(bar) = score.master_bars.get(played.source) else {
            return Err(PlaybackError::OrderMismatch);
        };
        if previous.is_some_and(|source| played.source < source) {
            points.push(played.start);
        }
        previous = Some(played.source);
        for change in &bar.tempo_changes {
            let offset = Exact::of(change.position).min(Exact::of(played.length));
            if offset < Exact::of(played.length) {
                points.push(ScoreTime::new(
                    Exact::of(played.start.whole_notes())
                        .add(offset)?
                        .fraction()?,
                )?);
            }
        }
    }
    points.sort();
    points.dedup();
    let mut events = Vec::new();
    for at in points {
        let index = tempo
            .segments()
            .partition_point(|segment| segment.start <= at)
            .saturating_sub(1);
        let Some(segment) = tempo.segments().get(index) else {
            continue;
        };
        let micros = Exact::integer(MICROS_PER_MINUTE)
            .div(Exact::of(segment.quarters_per_minute))?
            .floor();
        events.push(ConductorEvent {
            at,
            kind: ConductorKind::Tempo {
                micros_per_quarter: u32::try_from(micros)
                    .map_err(|_| PlaybackError::TimeOverflow)?,
            },
        });
    }
    let mut signature = None;
    let mut key = None;
    for played in &order.bars {
        let Some(bar) = score.master_bars.get(played.source) else {
            return Err(PlaybackError::OrderMismatch);
        };
        let restated = played
            .source
            .checked_sub(1)
            .and_then(|before| score.master_bars.get(before));
        if restated.is_none_or(|before| before.time_signature != bar.time_signature) {
            signature = None;
        }
        if restated.is_none_or(|before| before.key_signature != bar.key_signature) {
            key = None;
        }
        let time = ConductorKind::TimeSignature {
            numerator: bar.time_signature.numerator(),
            denominator: bar.time_signature.denominator(),
        };
        if signature != Some(time) {
            signature = Some(time);
            events.push(ConductorEvent {
                at: played.start,
                kind: time,
            });
        }
        let current = ConductorKind::KeySignature {
            accidentals: bar.key_signature.accidentals(),
            minor: bar.key_signature.minor(),
        };
        if key != Some(current) {
            key = Some(current);
            events.push(ConductorEvent {
                at: played.start,
                kind: current,
            });
        }
    }
    events.sort_by_key(|event| event.at);
    Ok(events)
}

fn setup(
    score: &Score,
    channels: &[Channel],
    timelines: &[Timeline],
    layout: TrackLayout,
    stream: &mut Stream,
) -> Vec<TrackSetup> {
    score
        .tracks
        .iter()
        .zip(channels)
        .enumerate()
        .map(|(index, (track, channel))| {
            let musical = timelines.get(index);
            let state = match musical {
                Some(timeline) => timeline.initial,
                None => ChannelState {
                    program: track.instrument.midi_program,
                    bend_range: BEND_RANGE_SEMITONES,
                    ..ChannelState::default()
                },
            };
            let lanes = match layout {
                TrackLayout::PerStaff => track.staves.len(),
                TrackLayout::PerTrack => 1,
            };
            for staff in 0..lanes {
                let lane = Lane {
                    track: index,
                    staff,
                    channel: *channel,
                };
                let messages: Vec<MidiMessage> = match musical {
                    Some(_) => state.messages()[..OPENING_MESSAGES].to_vec(),
                    None => vec![
                        MidiMessage::Program {
                            program: state.program,
                        },
                        MidiMessage::Rpn {
                            parameter: PITCH_BEND_RANGE_RPN,
                            value: state.bend_range,
                        },
                    ],
                };
                for message in messages {
                    stream.push(Exact::ZERO, lane, message);
                }
            }
            TrackSetup {
                name: track.name.clone(),
                staves: track.staves.len(),
                channel: *channel,
                channels: vec![ChannelSetup {
                    channel: *channel,
                    state,
                }],
            }
        })
        .collect()
}

pub fn perform(score: &Score, options: &PlaybackOptions) -> Result<Performance, PlaybackError> {
    let order = play_order(score);
    let tempo = TempoMap::build_with(
        score,
        &order,
        options.profile.ramp_grid(),
        options.profile.jump_tempo(),
    )?;
    let mut warnings = Vec::new();
    let (channels, pool) = match options.profile {
        Profile::Gp => (gp_channels(&score.tracks), ChannelPool::new()),
        Profile::Musical => pooled_channels(&score.tracks, &mut warnings),
    };
    let timelines: Vec<Timeline> = match options.profile {
        Profile::Gp => Vec::new(),
        Profile::Musical => score
            .tracks
            .iter()
            .map(|track| timeline(score, track, &order))
            .collect::<Result<_, _>>()?,
    };
    let mut stream = Stream::default();
    let mut tracks = setup(
        score,
        &channels,
        &timelines,
        options.profile.track_layout(),
        &mut stream,
    );
    let mut strands = Vec::new();
    for ((index, track), channel) in score.tracks.iter().enumerate().zip(&channels) {
        let writer = TrackWriter::new(score, index, track, *channel, options.profile);
        let output = writer.write(&order, &mut stream)?;
        warnings.extend(output.warnings);
        strands.extend(output.strands);
    }
    if options.profile == Profile::Musical {
        Musical {
            shared: &channels,
            timelines: &timelines,
            pool,
        }
        .emit(strands, &mut stream, &mut warnings, &mut tracks)?;
    }
    let mut pending = stream.pending;
    pending.sort_by(|left, right| {
        left.at
            .cmp(&right.at)
            .then(left.rank.cmp(&right.rank))
            .then(left.order.cmp(&right.order))
    });
    let mut events = Vec::with_capacity(pending.len());
    for (seq, item) in pending.into_iter().enumerate() {
        events.push(TimedEvent {
            at: ScoreTime::new(item.at.fraction()?)?,
            seq: u32::try_from(seq).map_err(|_| PlaybackError::TooManyEvents)?,
            track: item.track,
            staff: item.staff,
            channel: item.channel,
            message: item.message,
        });
    }
    Ok(Performance {
        profile: options.profile,
        conductor: conductor(score, &order, &tempo)?,
        clicks: beat_grid(score, &order),
        unplayed: catalogue(score, options.profile),
        order,
        tempo,
        tracks,
        events,
        warnings,
    })
}

impl Performance {
    pub fn chase(&self, track: usize, at: ScoreTime) -> Vec<TimedEvent> {
        let Some(setup) = self.tracks.get(track) else {
            return Vec::new();
        };
        let mut states: Vec<ChannelSetup> = setup.channels.clone();
        for event in self
            .events
            .iter()
            .take_while(|event| event.at <= at)
            .filter(|event| event.track == track)
        {
            if let Some(found) = states
                .iter_mut()
                .find(|candidate| candidate.channel == event.channel)
            {
                found.state.apply(event.message);
            }
        }
        let mut out = Vec::new();
        for channel in &states {
            for message in channel.state.messages() {
                out.push(TimedEvent {
                    at,
                    seq: u32::try_from(out.len()).unwrap_or(u32::MAX),
                    track,
                    staff: 0,
                    channel: channel.channel,
                    message,
                });
            }
        }
        out
    }
}
