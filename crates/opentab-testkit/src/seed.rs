use opentab_model::{
    Base64Bytes, BeatEffects, Clef, ContainerEntry, Fraction, Grace, InstrumentKind, KeySignature,
    NoteEffects, NoteValue, OpaquePayload, Pitch, ScoreMeta, Tempo, TempoUnit, TimeSignature,
    Transpose,
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
const MAX_CONTAINER_ENTRIES: usize = 3;
const MAX_CONTAINER_BYTES: usize = 48;
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
pub struct GraceSeed {
    pub kind: Grace,
    pub value: NoteValue,
    pub notes: Vec<NoteSeed>,
}

#[derive(Debug, Clone)]
pub struct BeatSeed {
    pub pick: usize,
    pub notes: Vec<NoteSeed>,
    pub effects: BeatEffects,
    pub grace: Option<GraceSeed>,
}

#[derive(Debug, Clone)]
pub struct VoiceSeed {
    pub skip: u8,
    pub pickup_cut: bool,
    pub beats: Vec<BeatSeed>,
}

#[derive(Debug, Clone)]
pub struct BarSeed {
    pub clef: Clef,
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
    pub transpose: Transpose,
    pub staves: Vec<StaffSeed>,
}

#[derive(Debug, Clone)]
pub struct TempoChangeSeed {
    pub share: Fraction,
    pub bpm: Tempo,
    pub unit: TempoUnit,
}

#[derive(Debug, Clone)]
pub struct MarkerSeed {
    pub text: String,
    pub letter: String,
}

#[derive(Debug, Clone)]
pub struct MasterBarSeed {
    pub time_signature: TimeSignature,
    pub key_signature: KeySignature,
    pub repeat_start: bool,
    pub repeat_end: Option<u8>,
    pub alternate_endings: u8,
    pub double_bar: bool,
    pub marker: Option<MarkerSeed>,
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
    pub anacrusis: bool,
    pub master_bars: Vec<MasterBarSeed>,
    pub tracks: Vec<TrackSeed>,
    pub extensions: Vec<ExtensionSeed>,
    pub document_extensions: Vec<OpaquePayload>,
    pub container: Vec<ContainerEntry>,
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

fn arb_grace_seed(cfg: ScoreGenConfig) -> impl Strategy<Value = GraceSeed> {
    (
        select(vec![Grace::BeforeBeat, Grace::OnBeat]),
        select(vec![
            NoteValue::Eighth,
            NoteValue::Sixteenth,
            NoteValue::ThirtySecond,
            NoteValue::SixtyFourth,
        ]),
        vec(arb_note_seed(), 0..=cfg.max_notes_per_beat),
    )
        .prop_map(|(kind, value, notes)| GraceSeed { kind, value, notes })
}

fn arb_beat_seed(cfg: ScoreGenConfig) -> impl Strategy<Value = BeatSeed> {
    (
        0..MAX_PICK,
        vec(arb_note_seed(), 0..=cfg.max_notes_per_beat),
        arb_beat_effects(),
        option::of(arb_grace_seed(cfg)),
    )
        .prop_map(|(pick, notes, effects, grace)| BeatSeed {
            pick,
            notes,
            effects,
            grace,
        })
}

fn arb_voice_seed(cfg: ScoreGenConfig) -> impl Strategy<Value = VoiceSeed> {
    (
        0u8..=1,
        any::<bool>(),
        vec(arb_beat_seed(cfg), 0..=cfg.max_beats_per_voice),
    )
        .prop_map(|(skip, pickup_cut, beats)| VoiceSeed {
            skip,
            pickup_cut,
            beats,
        })
}

fn arb_clef() -> impl Strategy<Value = Clef> {
    select(vec![Clef::Treble, Clef::Bass, Clef::Neutral])
}

fn arb_bar_seed(cfg: ScoreGenConfig) -> impl Strategy<Value = BarSeed> {
    (arb_clef(), vec(arb_voice_seed(cfg), 1..=MAX_VOICES_PER_BAR))
        .prop_map(|(clef, voices)| BarSeed { clef, voices })
}

fn arb_transpose() -> impl Strategy<Value = Transpose> {
    (-12i8..=12, -3i8..=3).prop_map(|(chromatic, octave)| {
        Transpose::new(chromatic, octave).expect("transpose is within range")
    })
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
        arb_transpose(),
        vec(arb_staff_seed(cfg, bars), 1..=MAX_STAVES_PER_TRACK),
    )
        .prop_map(
            |((name, short_name), kind, (midi_program, capo), tuning, transpose, staves)| {
                TrackSeed {
                    name,
                    short_name,
                    kind,
                    midi_program,
                    tuning,
                    capo,
                    transpose,
                    staves,
                }
            },
        )
}

fn arb_time_signature() -> impl Strategy<Value = TimeSignature> {
    (1u8..=12, select(DENOMINATORS.to_vec())).prop_map(|(numerator, denominator)| {
        TimeSignature::new(numerator, denominator).expect("terms are non-zero")
    })
}

fn arb_tempo_change() -> impl Strategy<Value = TempoChangeSeed> {
    (
        0i64..=64,
        1i64..=16,
        MIN_BPM..=MAX_BPM,
        select(vec![
            TempoUnit::Eighth,
            TempoUnit::Quarter,
            TempoUnit::DottedQuarter,
            TempoUnit::Half,
            TempoUnit::DottedHalf,
        ]),
    )
        .prop_map(|(numer, denom, bpm, unit)| TempoChangeSeed {
            share: Fraction::new(numer % denom, denom),
            bpm: Tempo::new(bpm).expect("bpm within range"),
            unit,
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
        (
            any::<bool>(),
            option::of(
                (arb_text(12), arb_text(3)).prop_map(|(text, letter)| MarkerSeed { text, letter }),
            ),
        ),
        vec(arb_tempo_change(), 0..=MAX_TEMPO_CHANGES),
    )
        .prop_map(
            |(
                time_signature,
                key_signature,
                (repeat_start, repeat_end, alternate_endings),
                (double_bar, marker),
                tempo_changes,
            )| MasterBarSeed {
                time_signature,
                key_signature,
                repeat_start,
                repeat_end,
                alternate_endings,
                double_bar,
                marker,
                tempo_changes,
            },
        )
}

fn arb_payload() -> impl Strategy<Value = OpaquePayload> {
    (arb_text(8), arb_text(32)).prop_map(|(format, data)| OpaquePayload { format, data })
}

fn arb_container_entry() -> impl Strategy<Value = ContainerEntry> {
    (arb_text(16), vec(any::<u8>(), 0..=MAX_CONTAINER_BYTES)).prop_map(|(name, data)| {
        ContainerEntry {
            name,
            data: Base64Bytes(data),
        }
    })
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
        (arb_meta(), any::<bool>()),
        vec(arb_master_bar_seed(), 0..=cfg.max_master_bars),
        vec(arb_extension_seed(), 0..=MAX_EXTENSION_ENTRIES),
        (
            vec(arb_payload(), 0..=MAX_EXTENSION_PAYLOADS),
            vec(arb_container_entry(), 0..=MAX_CONTAINER_ENTRIES),
        ),
    )
        .prop_flat_map(
            move |(
                (meta, anacrusis),
                master_bars,
                extensions,
                (document_extensions, container),
            )| {
                let bars = master_bars.len();
                vec(arb_track_seed(cfg, bars), 0..=cfg.max_tracks).prop_map(move |tracks| {
                    ScoreSeed {
                        meta: meta.clone(),
                        anacrusis,
                        master_bars: master_bars.clone(),
                        tracks,
                        extensions: extensions.clone(),
                        document_extensions: document_extensions.clone(),
                        container: container.clone(),
                    }
                })
            },
        )
}
