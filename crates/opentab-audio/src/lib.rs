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
mod sfz;
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
pub use sfz::{
    AudioKind, CcRange, DecodedSample, Envelope, IgnoreReason, IgnoredOpcode, LoopMode, OffMode,
    SFZ_MASTER_GAIN, SFZ_MAX_AUDIO_FILE_BYTES, SFZ_MAX_BANK_TEXT_BYTES, SFZ_MAX_DECODED_BYTES,
    SFZ_MAX_DEFINE_BYTES, SFZ_MAX_DEFINES, SFZ_MAX_EXPANDED_BYTES, SFZ_MAX_FILE_BYTES,
    SFZ_MAX_FOLDER_DEPTH, SFZ_MAX_FOLDER_ENTRIES, SFZ_MAX_INCLUDE_DEPTH, SFZ_MAX_INCLUDES,
    SFZ_MAX_LOADER_BYTES, SFZ_MAX_REGIONS, SFZ_MAX_SAMPLE_CHANNELS, SFZ_MAX_SAMPLE_FRAMES,
    SFZ_MAX_SAMPLE_RATE, SFZ_MAX_SFZ_FILES, SFZ_MAX_SIDECAR_BYTES, SFZ_MAX_VOICES,
    SFZ_MAX_VOICES_PER_NOTE, SFZ_MAX_VORBIS_TABLE_VALUES, SFZ_MIN_SAMPLE_RATE, SIDECAR_NAME,
    SIDECAR_VERSION, SfzBank, SfzLimits, SfzRegion, SfzScope, SfzSynth, SfzText, SkippedInstrument,
    Trigger, VelocityPoint, audio_kind, decode_audio, parse_sfz_text,
};
pub use synth::{Bank, BankFormat, BankInstrument, Synth};
pub use wav::{WAV_HEADER_BYTES, encode_wav, pcm16};
