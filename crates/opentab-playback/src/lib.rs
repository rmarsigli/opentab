#![deny(clippy::float_arithmetic)]

mod beats;
mod error;
mod exact;
mod order;
mod tempo;
mod time;

pub use beats::{Click, beat_grid};
pub use error::PlaybackError;
pub use order::{MAX_PLAYED_BARS, PlayOrder, PlayedBar, PlayedPosition, play_order};
pub use tempo::{
    DEFAULT_QUARTERS_PER_MINUTE, MAX_PLAYBACK_DECIMALS, RampGrid, TempoMap, TempoSegment,
    playback_bpm, quarters_per_minute, quarters_per_unit,
};
pub use time::{MAX_SPEED, MIN_SPEED, ScoreTime, Seconds, Speed};
