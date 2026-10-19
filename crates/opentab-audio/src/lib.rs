#[deny(clippy::float_arithmetic)]
mod builtin;

mod bank;
mod error;
mod gm;
mod rate;
mod render;
mod schedule;
mod synth;
mod wav;

pub use bank::{BuiltInBank, BuiltInSynth};
pub use builtin::{DRUM_DECAY_MS, MELODIC_RELEASE_MS};
pub use error::AudioError;
pub use rate::SampleRate;
pub use render::{
    BLOCK_FRAMES, MAX_FRAMES, RENDER_TAIL_MS, RenderedAudio, full_length, render_all, render_frames,
};
pub use schedule::{SampledEvent, Scheduler, frame_at};
pub use synth::{Bank, BankFormat, BankInstrument, Synth};
pub use wav::{WAV_HEADER_BYTES, encode_wav, pcm16};
