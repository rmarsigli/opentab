#[deny(clippy::float_arithmetic)]
mod builtin;

#[deny(clippy::float_arithmetic)]
mod bus;

mod bank;
mod error;
mod gm;
mod rate;
mod render;
mod schedule;
mod sf2;
mod synth;
mod wav;

pub use bank::{BuiltInBank, BuiltInSynth};
pub use builtin::{DRUM_DECAY_MS, MELODIC_RELEASE_MS, OUTPUT_FRACTION_BITS, SoundingVoice};
pub use bus::{BUS_FRACTION_BITS, Limiter, LimiterSettings};
pub use error::{AudioError, BankError, SF3_UNSUPPORTED, UnsafeBank};
pub use rate::SampleRate;
pub use render::{
    BLOCK_FRAMES, MAX_FRAMES, RENDER_TAIL_MS, RenderedAudio, TRACK_HEADROOM, full_length,
    render_all, render_frames,
};
pub use schedule::{SampledEvent, Scheduler, frame_at};
pub use sf2::{
    BankShape, FAULT_LOG_LIMIT, LOADER_FILE_FACTOR, LOADER_SLACK_BYTES, MASTER_VOLUME,
    MAX_FILE_BYTES, MAX_GRID_PRESETS, MAX_LAYERS_PER_NOTE, MAX_PAIRS_PER_PRESET, MAX_REGION_PAIRS,
    MAX_VOICES_PER_NOTE, Sf2Bank, Sf2Limits, Sf2Synth, SynthFault, SynthStage,
};
pub use synth::{Bank, BankFormat, BankInstrument, Synth};
pub use wav::{WAV_HEADER_BYTES, encode_wav, pcm16};
