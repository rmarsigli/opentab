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

pub const SF3_UNSUPPORTED: &str = "This is an SF3 SoundFont, which keeps its sounds compressed. OpenTab plays SF2 SoundFonts only. Convert the file to SF2 (the free Polyphone editor can do it) and import the SF2 file instead.";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BankError {
    #[error("{message}")]
    Unsupported { message: String },
    #[error("this file is not a usable SoundFont: {reason}")]
    Invalid { reason: String },
    #[error("the sound engine failed while reading this SoundFont: {message}")]
    Panicked { message: String },
}
