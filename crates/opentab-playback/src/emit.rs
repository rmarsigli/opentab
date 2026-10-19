use opentab_model::MidiBank;

use crate::allocate::{ChannelAllocator, ChannelGrant};
use crate::bends::BendEvent;
use crate::channels::ChannelPool;
use crate::controls::{MUTED_GUITAR_PROGRAM, Timeline};
use crate::exact::Exact;
use crate::midi::{BEND_CENTRE, Channel, ChannelSetup, ChannelState, MidiMessage, TrackSetup};
use crate::notes::{Lane, Strand, Stream, Struck, restrike};
use crate::{PlaybackError, PlaybackWarning, ScoreTime};

#[derive(Debug, Clone, Copy)]
struct Placed {
    index: usize,
    channel: Channel,
    fallback: bool,
    opening: Option<ChannelState>,
}

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
        let mut placed: Vec<Placed> = Vec::with_capacity(ordered.len());
        for (start, track, index) in ordered {
            let Some(strand) = strands.get(index) else {
                continue;
            };
            let end = strand.end();
            let grant = allocator.request(track, time_of(start)?, time_of(end)?, strand.need())?;
            let shared = self.shared_channel(track);
            let mut place = Placed {
                index,
                channel: shared,
                fallback: false,
                opening: None,
            };
            match grant {
                ChannelGrant::Shared => {}
                ChannelGrant::Fallback => place.fallback = true,
                ChannelGrant::Allocated { channel, fresh } => {
                    place.channel = channel;
                    if fresh {
                        let mut state = self
                            .timelines
                            .get(track)
                            .map(|timeline| timeline.state_at(start))
                            .unwrap_or_default();
                        if strand.muted {
                            state.program = MUTED_GUITAR_PROGRAM;
                            state.bank = MidiBank::default();
                        }
                        state.bend = BEND_CENTRE;
                        place.opening = Some(state);
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
                }
            }
            placed.push(place);
        }
        let mut struck: Vec<Struck> = Vec::new();
        let mut owners: Vec<usize> = Vec::new();
        for (slot, place) in placed.iter().enumerate() {
            for note in strands
                .get(place.index)
                .map_or(&[][..], |strand| &strand.notes)
            {
                struck.push(Struck {
                    channel: place.channel,
                    key: note.key,
                    start: note.start,
                    end: note.end,
                    dropped: false,
                });
                owners.push(slot);
            }
        }
        restrike(&mut struck);
        let mut cut = struck.iter().zip(&owners);
        for place in &placed {
            let Some(strand) = strands.get(place.index) else {
                continue;
            };
            let track = strand.track;
            let start = strand.start();
            let strand_lane = lane(track, strand.staff, place.channel);
            let mut bends: &[BendEvent] = &strand.bends;
            if let Some(mut state) = place.opening {
                if let Some(first) = bends.first().filter(|first| first.at == start) {
                    state.bend = first.value;
                    bends = &bends[1..];
                }
                for message in opening(state) {
                    stream.push(start, strand_lane, message);
                }
            }
            let mut end = start;
            let mut notes: Vec<Struck> = Vec::with_capacity(strand.notes.len());
            let mut velocities: Vec<u8> = Vec::with_capacity(strand.notes.len());
            for note in &strand.notes {
                let Some((kept, _)) = cut.next() else {
                    continue;
                };
                if !kept.dropped {
                    end = end.max(kept.end);
                    notes.push(*kept);
                    velocities.push(note.velocity);
                }
            }
            for bend in bends.iter().filter(|bend| bend.at < end) {
                stream.push(
                    bend.at,
                    strand_lane,
                    MidiMessage::PitchBend { value: bend.value },
                );
            }
            for (note, velocity) in notes.iter().zip(velocities) {
                if place.fallback {
                    warnings.push(PlaybackWarning::ChannelFallback {
                        track,
                        at: time_of(note.start)?,
                    });
                }
                stream.note(note.start, note.end, strand_lane, note.key, velocity);
            }
            if place.fallback && !strand.bends.is_empty() && !notes.is_empty() {
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
