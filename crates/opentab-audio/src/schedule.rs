use opentab_playback::{ScoreTime, Speed, TempoMap};

use crate::timeline::{step_of, unit_at};
use crate::{AudioError, SampleRate};

pub fn frame_at(
    tempo: &TempoMap,
    at: ScoreTime,
    speed: Speed,
    rate: SampleRate,
) -> Result<u64, AudioError> {
    Ok(unit_at(tempo, at, rate)? / step_of(speed))
}
