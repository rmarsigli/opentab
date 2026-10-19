use crate::channels::ChannelPool;
use crate::midi::Channel;
use crate::{PlaybackError, ScoreTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelNeed {
    Shared,
    Muted,
    Exclusive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelGrant {
    Shared,
    Allocated { channel: Channel, fresh: bool },
    Fallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Held {
    channel: Channel,
    track: usize,
    group: bool,
    until: ScoreTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelAllocator {
    pool: ChannelPool,
    held: Vec<Held>,
    latest: ScoreTime,
}

impl ChannelAllocator {
    pub fn new(pool: ChannelPool) -> ChannelAllocator {
        ChannelAllocator {
            pool,
            held: Vec::new(),
            latest: ScoreTime::ZERO,
        }
    }

    pub fn pool(&self) -> &ChannelPool {
        &self.pool
    }

    fn release_until(&mut self, at: ScoreTime) {
        let (done, kept): (Vec<Held>, Vec<Held>) =
            self.held.iter().partition(|held| held.until <= at);
        for held in done {
            self.pool.release(held.channel);
        }
        self.held = kept;
    }

    pub fn request(
        &mut self,
        track: usize,
        start: ScoreTime,
        end: ScoreTime,
        need: ChannelNeed,
    ) -> Result<ChannelGrant, PlaybackError> {
        if start < self.latest {
            return Err(PlaybackError::RequestOutOfOrder);
        }
        self.latest = start;
        self.release_until(start);
        let group = match need {
            ChannelNeed::Shared => return Ok(ChannelGrant::Shared),
            ChannelNeed::Muted => true,
            ChannelNeed::Exclusive => false,
        };
        if group
            && let Some(held) = self
                .held
                .iter_mut()
                .find(|held| held.group && held.track == track)
        {
            held.until = held.until.max(end);
            return Ok(ChannelGrant::Allocated {
                channel: held.channel,
                fresh: false,
            });
        }
        Ok(match self.pool.acquire(None) {
            Some(channel) => {
                self.held.push(Held {
                    channel,
                    track,
                    group,
                    until: end,
                });
                ChannelGrant::Allocated {
                    channel,
                    fresh: true,
                }
            }
            None => ChannelGrant::Fallback,
        })
    }
}
