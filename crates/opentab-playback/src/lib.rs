#![deny(clippy::float_arithmetic)]

mod beats;
mod bends;
mod channels;
mod error;
mod exact;
mod harmonics;
mod midi;
mod notes;
mod order;
mod perform;
mod tempo;
mod time;
mod unplayed;

pub use beats::{Click, beat_grid};
pub use channels::ChannelPool;
pub use error::PlaybackError;
pub use midi::{
    BEND_CENTRE, BEND_MAX, BEND_RANGE_SEMITONES, Channel, ChannelSetup, ChannelState,
    ConductorEvent, ConductorKind, DEFAULT_PAN, DEFAULT_VOLUME, MidiMessage, NOTE_OFF_VELOCITY,
    PAN_CONTROLLER, PITCH_BEND_RANGE_RPN, TimedEvent, TrackSetup, VOLUME_CONTROLLER, velocity_of,
};
pub use order::{MAX_PLAYED_BARS, PlayOrder, PlayedBar, PlayedPosition, play_order};
pub use perform::{Performance, PlaybackOptions, Profile, perform};
pub use tempo::{
    DEFAULT_QUARTERS_PER_MINUTE, MAX_PLAYBACK_DECIMALS, RampGrid, RampStep, TempoMap, TempoSegment,
    playback_bpm, quarters_per_minute, quarters_per_unit,
};
pub use time::{MAX_SPEED, MIN_SPEED, ScoreTime, Seconds, Speed};
pub use unplayed::{PlaybackWarning, UnplayedFeature, UnplayedKind};
