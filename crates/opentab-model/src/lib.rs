mod blank;
mod container;
mod diagnostics;
mod directions;
mod drums;
mod duration;
mod effects;
mod error;
mod extensions;
mod fraction;
mod ids;
mod json;
mod master_bar;
mod midi;
mod play_order;
mod score;
mod track;

pub use blank::blank_score;
pub use container::{Base64Bytes, ContainerEntry};
pub use diagnostics::{Diagnostic, diagnose};
pub use directions::{
    DIRECTION_JUMPS, DIRECTION_TARGETS, DirectionJump, DirectionTarget, Directions, JumpDestination,
};
pub use drums::{
    DrumArticulation, DrumElement, DrumKit, MAX_DRUM_BEAT_NOTES, TechniquePlacement, gm_drum_kit,
};
pub use duration::{Duration, NoteValue, Tuplet};
pub use effects::{
    Accent, BeatEffects, Bend, BendPoint, Dynamic, Finger, Grace, Harmonic, HarmonicKind,
    NoteEffects, Slide, Stroke, StrokeDirection, StrokeTicks, Tap, TremoloBar, TremoloPicking,
    Trill, TrillSpeed, Vibrato,
};
pub use error::ModelError;
pub use extensions::{Extensions, OpaquePayload};
pub use fraction::{Fraction, WholeNotes};
pub use ids::{
    BarId, BeatId, ElementId, IdAllocator, MasterBarId, NoteId, StaffId, TrackId, VoiceId,
};
pub use json::{check_sound_overrides, from_json, to_json};
pub use master_bar::{
    KeySignature, Marker, MasterBar, TRIPLET_FEELS, Tempo, TempoChange, TempoUnit, TimeSignature,
    TripletFeel,
};
pub use midi::{
    AUTOMATION_GRID, Automation, AutomationValue, Balance, InstrumentFamily, InstrumentSet, Level,
    MAX_PROGRAM, MidiBank, MidiChannel, Sound, SoundOverride, SoundRef, TrackMidi,
    automations_in_order, check_program, on_automation_grid,
};
pub use play_order::{MAX_PLAYED_BARS, PlayedSource, SourceOrder, source_order};
pub use score::{BeatPath, NotePath, Score, ScoreMeta, VoicePath};
pub use track::{
    Bar, Beat, Clef, Instrument, InstrumentKind, Note, NoteKind, Pitch, Staff, Track, Transpose,
    TrillTuning, Tuning, Voice, VoiceSlot,
};
