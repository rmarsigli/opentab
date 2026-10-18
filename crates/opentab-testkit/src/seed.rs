use opentab_model::{
    BeatEffects, Fraction, InstrumentKind, KeySignature, NoteEffects, OpaquePayload, Pitch,
    ScoreMeta, Tempo, TimeSignature,
};
use proptest::collection::vec;
use proptest::option;
use proptest::prelude::*;
use proptest::sample::select;

use crate::effects::{arb_beat_effects, arb_note_effects};
use crate::text::{arb_meta_text, arb_text};

const MAX_VOICES_PER_BAR: usize = 2;
const MAX_STAVES_PER_TRACK: usize = 2;
const MAX_EXTENSION_ENTRIES: usize = 4;
const MAX_EXTENSION_PAYLOADS: usize = 3;
const MAX_TEMPO_CHANGES: usize = 2;
const MAX_STRINGS: usize = 8;
const MAX_FRET: u8 = 24;
const MAX_MIDI: u8 = 127;
const MAX_CAPO: u8 = 12;
const MAX_PICK: usize = 4096;
const MIN_BPM: f64 = 1.0;
const MAX_BPM: f64 = 1000.0;
const DENOMINATORS: [u8; 4] = [2, 4, 8, 16];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScoreGenConfig {
    pub max_tracks: usize,
    pub max_master_bars: usize,
    pub max_beats_per_voice: usize,
    pub max_notes_per_beat: usize,
}

impl Default for ScoreGenConfig {
    fn default() -> Self {
        ScoreGenConfig {
            max_tracks: 3,
            max_master_bars: 6,
            max_beats_per_voice: 8,
            max_notes_per_beat: 4,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NoteSeed {
    pub string: u8,
    pub fret: u8,
    pub articulation: u16,
    pub effects: NoteEffects,
}

#[derive(Debug, Clone)]
pub struct BeatSeed {
    pub pick: usize,
    pub notes: Vec<NoteSeed>,
    pub effects: BeatEffects,
}

#[derive(Debug, Clone)]
pub struct VoiceSeed {
    pub beats: Vec<BeatSeed>,
}

#[derive(Debug, Clone)]
pub struct BarSeed {
    pub voices: Vec<VoiceSeed>,
}

#[derive(Debug, Clone)]
pub struct StaffSeed {
    pub bars: Vec<BarSeed>,
}

#[derive(Debug, Clone)]
pub struct TrackSeed {
    pub name: String,
    pub short_name: String,
    pub kind: InstrumentKind,
    pub midi_program: u8,
    pub tuning: Vec<Pitch>,
    pub capo: u8,
    pub staves: Vec<StaffSeed>,
}

#[derive(Debug, Clone)]
pub struct TempoChangeSeed {
    pub position: Fraction,
    pub bpm: Tempo,
}

#[derive(Debug, Clone)]
pub struct MasterBarSeed {
    pub time_signature: TimeSignature,
    pub key_signature: KeySignature,
    pub repeat_start: bool,
    pub repeat_end: Option<u8>,
    pub alternate_endings: u8,
    pub marker: Option<String>,
    pub tempo_changes: Vec<TempoChangeSeed>,
}

#[derive(Debug, Clone)]
pub struct ExtensionSeed {
    pub target: usize,
    pub payloads: Vec<OpaquePayload>,
}

#[derive(Debug, Clone)]
pub struct ScoreSeed {
    pub meta: ScoreMeta,
    pub master_bars: Vec<MasterBarSeed>,
    pub tracks: Vec<TrackSeed>,
    pub extensions: Vec<ExtensionSeed>,
}

fn arb_note_seed() -> impl Strategy<Value = NoteSeed> {
    (any::<u8>(), 0..=MAX_FRET, any::<u16>(), arb_note_effects()).prop_map(
        |(string, fret, articulation, effects)| NoteSeed {
            string,
            fret,
            articulation,
            effects,
        },
    )
}

fn arb_beat_seed(cfg: ScoreGenConfig) -> impl Strategy<Value = BeatSeed> {
    (
        0..MAX_PICK,
        vec(arb_note_seed(), 0..=cfg.max_notes_per_beat),
        arb_beat_effects(),
    )
        .prop_map(|(pick, notes, effects)| BeatSeed {
            pick,
            notes,
            effects,
        })
}

fn arb_voice_seed(cfg: ScoreGenConfig) -> impl Strategy<Value = VoiceSeed> {
    vec(arb_beat_seed(cfg), 0..=cfg.max_beats_per_voice).prop_map(|beats| VoiceSeed { beats })
}

fn arb_bar_seed(cfg: ScoreGenConfig) -> impl Strategy<Value = BarSeed> {
    vec(arb_voice_seed(cfg), 1..=MAX_VOICES_PER_BAR).prop_map(|voices| BarSeed { voices })
}

fn arb_staff_seed(cfg: ScoreGenConfig, bars: usize) -> impl Strategy<Value = StaffSeed> {
    vec(arb_bar_seed(cfg), bars..=bars).prop_map(|bars| StaffSeed { bars })
}

fn arb_track_seed(cfg: ScoreGenConfig, bars: usize) -> impl Strategy<Value = TrackSeed> {
    (
        (arb_text(16), arb_text(6)),
        select(vec![InstrumentKind::Fretted, InstrumentKind::Percussion]),
        (0..=MAX_MIDI, 0..=MAX_CAPO),
        vec((0..=MAX_MIDI).prop_map(Pitch), 1..=MAX_STRINGS),
        vec(arb_staff_seed(cfg, bars), 1..=MAX_STAVES_PER_TRACK),
    )
        .prop_map(
            |((name, short_name), kind, (midi_program, capo), tuning, staves)| TrackSeed {
                name,
                short_name,
                kind,
                midi_program,
                tuning,
                capo,
                staves,
            },
        )
}

fn arb_time_signature() -> impl Strategy<Value = TimeSignature> {
    (1u8..=12, select(DENOMINATORS.to_vec())).prop_map(|(numerator, denominator)| {
        TimeSignature::new(numerator, denominator).expect("terms are non-zero")
    })
}

fn arb_tempo_change() -> impl Strategy<Value = TempoChangeSeed> {
    (0i64..=64, 1i64..=16, MIN_BPM..=MAX_BPM).prop_map(|(numer, denom, bpm)| TempoChangeSeed {
        position: Fraction::new(numer, denom),
        bpm: Tempo::new(bpm).expect("bpm within range"),
    })
}

fn arb_key_signature() -> impl Strategy<Value = KeySignature> {
    (-7i8..=7, any::<bool>()).prop_map(|(accidentals, minor)| {
        KeySignature::new(accidentals, minor).expect("accidentals are within range")
    })
}

fn arb_master_bar_seed() -> impl Strategy<Value = MasterBarSeed> {
    (
        arb_time_signature(),
        arb_key_signature(),
        (any::<bool>(), option::of(any::<u8>()), any::<u8>()),
        option::of(arb_text(12)),
        vec(arb_tempo_change(), 0..=MAX_TEMPO_CHANGES),
    )
        .prop_map(
            |(
                time_signature,
                key_signature,
                (repeat_start, repeat_end, alternate_endings),
                marker,
                tempo_changes,
            )| MasterBarSeed {
                time_signature,
                key_signature,
                repeat_start,
                repeat_end,
                alternate_endings,
                marker,
                tempo_changes,
            },
        )
}

fn arb_payload() -> impl Strategy<Value = OpaquePayload> {
    (arb_text(8), arb_text(32)).prop_map(|(format, data)| OpaquePayload { format, data })
}

fn arb_extension_seed() -> impl Strategy<Value = ExtensionSeed> {
    (
        any::<usize>(),
        vec(arb_payload(), 1..=MAX_EXTENSION_PAYLOADS),
    )
        .prop_map(|(target, payloads)| ExtensionSeed { target, payloads })
}

fn arb_meta() -> impl Strategy<Value = ScoreMeta> {
    (
        (arb_meta_text(), arb_meta_text(), arb_meta_text()),
        (arb_meta_text(), arb_meta_text(), arb_meta_text()),
        (arb_meta_text(), arb_meta_text(), arb_meta_text()),
        arb_meta_text(),
    )
        .prop_map(
            |(
                (title, subtitle, artist),
                (album, words, music),
                (copyright, tab_author, instructions),
                notices,
            )| ScoreMeta {
                title,
                subtitle,
                artist,
                album,
                words,
                music,
                copyright,
                tab_author,
                instructions,
                notices,
            },
        )
}

pub fn arb_score_seed(cfg: ScoreGenConfig) -> impl Strategy<Value = ScoreSeed> {
    (
        arb_meta(),
        vec(arb_master_bar_seed(), 0..=cfg.max_master_bars),
        vec(arb_extension_seed(), 0..=MAX_EXTENSION_ENTRIES),
    )
        .prop_flat_map(move |(meta, master_bars, extensions)| {
            let bars = master_bars.len();
            vec(arb_track_seed(cfg, bars), 0..=cfg.max_tracks).prop_map(move |tracks| ScoreSeed {
                meta: meta.clone(),
                master_bars: master_bars.clone(),
                tracks,
                extensions: extensions.clone(),
            })
        })
}
