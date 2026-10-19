use crate::allocate::{ChannelAllocator, ChannelGrant};
use crate::channels::ChannelPool;
use crate::controls::{MUTED_GUITAR_PROGRAM, Timeline};
use crate::exact::Exact;
use crate::midi::{
    BEND_CENTRE, Channel, ChannelSetup, ChannelState, MidiMessage, NOTE_OFF_VELOCITY, TrackSetup,
};
use crate::notes::{Lane, Strand, Stream};
use crate::{PlaybackError, PlaybackWarning, ScoreTime};

#[derive(Debug, Clone, Copy)]
struct Hold {
    track: usize,
    channel: Channel,
    from: Exact,
    until: Exact,
    muted: bool,
}

pub(crate) struct Musical<'a> {
    pub(crate) shared: &'a [Channel],
    pub(crate) timelines: &'a [Timeline],
    pub(crate) pool: ChannelPool,
}

fn time_of(at: Exact) -> Result<ScoreTime, PlaybackError> {
    at.fraction().and_then(ScoreTime::new)
}

fn lane(track: usize, staff: usize, channel: Channel) -> Lane {
    Lane {
        track,
        staff,
        channel,
    }
}

fn opening(state: ChannelState) -> [MidiMessage; 6] {
    state.messages()
}

impl Musical<'_> {
    fn shared_channel(&self, track: usize) -> Channel {
        self.shared.get(track).copied().unwrap_or(Channel::DRUMS)
    }

    pub(crate) fn emit(
        self,
        strands: Vec<Strand>,
        stream: &mut Stream,
        warnings: &mut Vec<PlaybackWarning>,
        setups: &mut [TrackSetup],
    ) -> Result<(), PlaybackError> {
        for (track, timeline) in self.timelines.iter().enumerate() {
            let channel = self.shared_channel(track);
            for change in &timeline.changes {
                stream.push(change.at, lane(track, 0, channel), change.message);
            }
        }
        let mut ordered: Vec<(Exact, usize, usize)> = Vec::with_capacity(strands.len());
        for (index, strand) in strands.iter().enumerate() {
            ordered.push((strand.start(), strand.track, index));
        }
        ordered.sort();
        let mut allocator = ChannelAllocator::new(self.pool.clone());
        let mut holds: Vec<Hold> = Vec::new();
        for (start, track, index) in ordered {
            let Some(strand) = strands.get(index) else {
                continue;
            };
            let end = strand.end();
            let grant = allocator.request(track, time_of(start)?, time_of(end)?, strand.need());
            let shared = self.shared_channel(track);
            let (channel, fallback) = match grant {
                ChannelGrant::Shared => (shared, false),
                ChannelGrant::Fallback => (shared, true),
                ChannelGrant::Allocated { channel, fresh } => {
                    if fresh {
                        let mut state = self
                            .timelines
                            .get(track)
                            .map(|timeline| timeline.state_at(start))
                            .unwrap_or_default();
                        if strand.muted {
                            state.program = MUTED_GUITAR_PROGRAM;
                        }
                        state.bend = BEND_CENTRE;
                        for message in opening(state) {
                            stream.push(start, lane(track, strand.staff, channel), message);
                        }
                        holds.push(Hold {
                            track,
                            channel,
                            from: start,
                            until: end,
                            muted: strand.muted,
                        });
                        if let Some(setup) = setups.get_mut(track)
                            && !setup.channels.iter().any(|known| known.channel == channel)
                        {
                            setup.channels.push(ChannelSetup {
                                channel,
                                state: ChannelState::default(),
                            });
                        }
                    } else if let Some(hold) = holds
                        .iter_mut()
                        .rev()
                        .find(|hold| hold.track == track && hold.channel == channel)
                    {
                        hold.until = hold.until.max(end);
                    }
                    (channel, false)
                }
            };
            let strand_lane = lane(track, strand.staff, channel);
            for bend in &strand.bends {
                stream.push(
                    bend.at,
                    strand_lane,
                    MidiMessage::PitchBend { value: bend.value },
                );
            }
            for note in &strand.notes {
                if fallback {
                    warnings.push(PlaybackWarning::ChannelFallback {
                        track,
                        at: time_of(note.start)?,
                    });
                }
                stream.push(
                    note.start,
                    strand_lane,
                    MidiMessage::NoteOn {
                        key: note.key,
                        velocity: note.velocity,
                    },
                );
                stream.push(
                    note.end,
                    strand_lane,
                    MidiMessage::NoteOff {
                        key: note.key,
                        velocity: NOTE_OFF_VELOCITY,
                    },
                );
            }
            if fallback && !strand.bends.is_empty() {
                stream.push(
                    end,
                    strand_lane,
                    MidiMessage::PitchBend { value: BEND_CENTRE },
                );
            }
        }
        for hold in holds {
            let Some(timeline) = self.timelines.get(hold.track) else {
                continue;
            };
            for change in timeline
                .changes
                .iter()
                .filter(|change| change.at > hold.from && change.at < hold.until)
            {
                let forwarded = match change.message {
                    MidiMessage::Program { .. } | MidiMessage::Bank { .. } => !hold.muted,
                    _ => true,
                };
                if forwarded {
                    stream.push(change.at, lane(hold.track, 0, hold.channel), change.message);
                }
            }
        }
        Ok(())
    }
}
