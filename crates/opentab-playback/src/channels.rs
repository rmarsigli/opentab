use std::collections::BTreeSet;

use opentab_model::{InstrumentKind, Track};

use crate::midi::{CHANNELS, Channel};
use crate::{PlaybackWarning, ScoreTime};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelPool {
    free: BTreeSet<Channel>,
    shared: usize,
}

impl Default for ChannelPool {
    fn default() -> ChannelPool {
        ChannelPool::new()
    }
}

impl ChannelPool {
    pub fn new() -> ChannelPool {
        ChannelPool {
            free: ChannelPool::melodic().into_iter().collect(),
            shared: 0,
        }
    }

    pub fn melodic() -> Vec<Channel> {
        (0..CHANNELS)
            .filter_map(Channel::new)
            .filter(|channel| *channel != Channel::DRUMS)
            .collect()
    }

    pub fn free(&self) -> usize {
        self.free.len()
    }

    pub fn acquire(&mut self, preferred: Option<Channel>) -> Option<Channel> {
        let chosen = preferred
            .filter(|channel| self.free.contains(channel))
            .or_else(|| self.free.first().copied())?;
        self.free.remove(&chosen);
        Some(chosen)
    }

    pub fn release(&mut self, channel: Channel) {
        if channel != Channel::DRUMS {
            self.free.insert(channel);
        }
    }

    pub fn fallback(&mut self) -> Channel {
        let melodic = ChannelPool::melodic();
        let index = self.shared % melodic.len().max(1);
        self.shared = self.shared.saturating_add(1);
        melodic.get(index).copied().unwrap_or(Channel::DRUMS)
    }
}

pub(crate) fn gp_channels(tracks: &[Track]) -> Vec<Channel> {
    tracks
        .iter()
        .map(|track| match track.instrument.kind {
            InstrumentKind::Percussion => Channel::DRUMS,
            InstrumentKind::Fretted => Channel::from(track.midi.primary),
        })
        .collect()
}

pub(crate) fn pooled_channels(
    tracks: &[Track],
    warnings: &mut Vec<PlaybackWarning>,
) -> Vec<Channel> {
    let mut pool = ChannelPool::new();
    tracks
        .iter()
        .enumerate()
        .map(|(index, track)| match track.instrument.kind {
            InstrumentKind::Percussion => Channel::DRUMS,
            InstrumentKind::Fretted => pool
                .acquire(Some(Channel::from(track.midi.primary)))
                .unwrap_or_else(|| {
                    warnings.push(PlaybackWarning::ChannelFallback {
                        track: index,
                        at: ScoreTime::ZERO,
                    });
                    pool.fallback()
                }),
        })
        .collect()
}
