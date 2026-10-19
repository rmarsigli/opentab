use opentab_playback::PlaybackError;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum AudioError {
    #[error("playback time: {0}")]
    Playback(#[from] PlaybackError),
    #[error("sample rate {hz} Hz is not supported (44100 or 48000)")]
    UnsupportedRate { hz: u32 },
    #[error("{voices} synths were given for {tracks} tracks")]
    VoiceCount { voices: usize, tracks: usize },
    #[error("{frames} frames exceed the longest render a WAV file holds")]
    TooLong { frames: u64 },
    #[error("an event falls at a frame beyond 64 bits")]
    FrameOverflow,
}
