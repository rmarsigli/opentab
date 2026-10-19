use opentab_model::{
    Balance, Base64Bytes, BeatEffects, Clef, ContainerEntry, Fraction, Grace, InstrumentKind,
    InstrumentSet, KeySignature, Level, MidiBank, MidiChannel, NoteEffects, NoteValue,
    OpaquePayload, Pitch, ScoreMeta, Tempo, TempoUnit, TimeSignature, TrackMidi, Transpose,
};
use proptest::collection::vec;
use proptest::option;
use proptest::prelude::*;
use proptest::sample::select;

use crate::effects::{arb_beat_effects, arb_note_effects};
use crate::text::{arb_meta_text, arb_text, arb_text_for};

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
const MAX_WRITABLE_OPEN: u8 = 91;
const GRID_DENOMINATORS: [i64; 11] = [1, 2, 3, 4, 5, 6, 8, 10, 12, 15, 16];
const MAX_EXTRA_CHARS: usize = 12;
const MAX_SOUNDS: usize = 3;
const MAX_AUTOMATIONS: usize = 5;
const MAX_OVERRIDES: usize = 2;
const MAX_PORT: u8 = 3;
const MAX_CHANNEL: u8 = 15;
const MAX_BANK: u8 = 127;
const MILLIONTHS: u32 = 1_000_000;
const SOUND_NAMES: [&str; 2] = ["", "Clean"];
const INSTRUMENT_SETS: [InstrumentSet; 12] = [
    InstrumentSet::ElectricGuitar,
    InstrumentSet::SteelGuitar,
    InstrumentSet::NylonGuitar,
    InstrumentSet::ElectricBass,
    InstrumentSet::AcousticBass,
    InstrumentSet::Violin,
    InstrumentSet::AcousticPiano,
    InstrumentSet::Saxophone,
    InstrumentSet::Trumpet,
    InstrumentSet::ElectricOrgan,
    InstrumentSet::Voice,
    InstrumentSet::DrumKit,
];
const SCORE_ENTRY: &str = "Content/score.gpif";
const DIRECTORY_ENTRY: &str = "Content/";
const CONTAINER_NAMES: [&str; 6] = [
    DIRECTORY_ENTRY,
    "Content/BinaryStylesheet",
    "Content/LayoutConfiguration",
    "Content/Preferences.json",
    "meta.json",
    "VERSION",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScoreGenConfig {
    pub max_tracks: usize,
    pub max_master_bars: usize,
    pub max_beats_per_voice: usize,
    pub max_notes_per_beat: usize,
    pub gp_writable: bool,
}

impl Default for ScoreGenConfig {
    fn default() -> Self {
        ScoreGenConfig {
            max_tracks: 3,
            max_master_bars: 6,
            max_beats_per_voice: 8,
            max_notes_per_beat: 4,
            gp_writable: false,
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
pub struct SoundSeed {
    pub name: String,
    pub program: u8,
    pub bank: MidiBank,
}

#[derive(Debug, Clone)]
pub enum AutomationValueSeed {
    Volume(Level),
    Pan(Balance),
    Sound(usize),
}

#[derive(Debug, Clone)]
pub struct AutomationSeed {
    pub bar: usize,
    pub share: Fraction,
    pub linear: bool,
    pub visible: bool,
    pub value: AutomationValueSeed,
}

#[derive(Debug, Clone)]
pub struct OverrideSeed {
    pub track: usize,
    pub bank: String,
}

#[derive(Debug, Clone)]
pub struct TrackSeed {
    pub name: String,
    pub short_name: String,
    pub kind: InstrumentKind,
    pub set: Option<InstrumentSet>,
    pub midi: TrackMidi,
    pub sounds: Vec<SoundSeed>,
    pub automations: Vec<AutomationSeed>,
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
    pub extras: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ScoreEntrySeed {
    pub position: usize,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct ScoreSeed {
    pub meta: ScoreMeta,
    pub anacrusis: bool,
    pub master_bars: Vec<MasterBarSeed>,
    pub tracks: Vec<TrackSeed>,
    pub extensions: Vec<ExtensionSeed>,
    pub document_extensions: Vec<OpaquePayload>,
    pub document_extras: Vec<String>,
    pub container: Vec<ContainerEntry>,
    pub score_entry: Option<ScoreEntrySeed>,
    pub sound_overrides: Vec<OverrideSeed>,
}

#[derive(Debug, Clone)]
struct DocumentSeed {
    document_extensions: Vec<OpaquePayload>,
    document_extras: Vec<String>,
    container: Vec<ContainerEntry>,
    score_entry: Option<ScoreEntrySeed>,
}

fn arb_note_seed(cfg: ScoreGenConfig) -> impl Strategy<Value = NoteSeed> {
    (
        any::<u8>(),
        0..=MAX_FRET,
        any::<u16>(),
        arb_note_effects(cfg.gp_writable),
    )
        .prop_map(|(string, fret, articulation, effects)| NoteSeed {
            string,
            fret,
            articulation,
            effects,
        })
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
        vec(arb_note_seed(cfg), 0..=cfg.max_notes_per_beat),
    )
        .prop_map(|(kind, value, notes)| GraceSeed { kind, value, notes })
}

fn arb_beat_seed(cfg: ScoreGenConfig) -> impl Strategy<Value = BeatSeed> {
    (
        0..MAX_PICK,
        vec(arb_note_seed(cfg), 0..=cfg.max_notes_per_beat),
        arb_beat_effects(cfg.gp_writable),
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

fn arb_bank() -> impl Strategy<Value = MidiBank> {
    prop_oneof![
        2 => Just(MidiBank::default()),
        1 => (0..=MAX_BANK, 0..=MAX_BANK)
            .prop_map(|(msb, lsb)| MidiBank::new(msb, lsb).expect("bank bytes are within range")),
    ]
}

fn arb_millionths() -> impl Strategy<Value = u32> {
    prop_oneof![
        1 => Just(MILLIONTHS / 2),
        1 => select(vec![0, MILLIONTHS, 620_000, 812_500, 1]),
        2 => 0..=MILLIONTHS,
    ]
}

fn arb_channel() -> impl Strategy<Value = MidiChannel> {
    (0..=MAX_CHANNEL).prop_map(|channel| MidiChannel::new(channel).expect("channel is in range"))
}

fn arb_track_midi() -> impl Strategy<Value = TrackMidi> {
    (
        (0..=MAX_PORT, arb_channel(), arb_channel(), any::<bool>()),
        arb_bank(),
        (arb_millionths(), arb_millionths()),
    )
        .prop_map(
            |((port, primary, secondary, per_string), bank, (volume, pan))| TrackMidi {
                port,
                primary,
                secondary,
                per_string,
                bank,
                volume: Level::new(volume).expect("volume is in range"),
                pan: Balance::new(pan).expect("pan is in range"),
            },
        )
}

fn arb_sound_name(gp_writable: bool) -> impl Strategy<Value = String> {
    prop_oneof![
        3 => arb_text_for(8, gp_writable).boxed(),
        1 => select(SOUND_NAMES.to_vec()).prop_map(str::to_string).boxed(),
    ]
}

fn arb_sound_seed(gp_writable: bool) -> impl Strategy<Value = SoundSeed> {
    (arb_sound_name(gp_writable), 0..=MAX_MIDI, arb_bank()).prop_map(|(name, program, bank)| {
        SoundSeed {
            name,
            program,
            bank,
        }
    })
}

fn arb_automation_value() -> impl Strategy<Value = AutomationValueSeed> {
    prop_oneof![
        arb_millionths()
            .prop_map(|value| AutomationValueSeed::Volume(Level::new(value).expect("in range"))),
        arb_millionths()
            .prop_map(|value| AutomationValueSeed::Pan(Balance::new(value).expect("in range"))),
        any::<usize>().prop_map(AutomationValueSeed::Sound),
    ]
}

fn arb_automation_seed() -> impl Strategy<Value = AutomationSeed> {
    (
        any::<usize>(),
        (0i64..=64, arb_share_denominator(true)),
        any::<bool>(),
        prop::bool::weighted(0.8),
        arb_automation_value(),
    )
        .prop_map(
            |(bar, (numer, denom), linear, visible, value)| AutomationSeed {
                bar,
                share: Fraction::new(numer % (denom + 1), denom),
                linear,
                visible,
                value,
            },
        )
}

fn arb_track_seed(cfg: ScoreGenConfig, bars: usize) -> impl Strategy<Value = TrackSeed> {
    let highest_open = if cfg.gp_writable {
        MAX_WRITABLE_OPEN
    } else {
        MAX_MIDI
    };
    (
        (
            arb_text_for(16, cfg.gp_writable),
            arb_text_for(6, cfg.gp_writable),
        ),
        (
            select(vec![InstrumentKind::Fretted, InstrumentKind::Percussion]),
            option::of(select(INSTRUMENT_SETS.to_vec())),
        ),
        (
            arb_track_midi(),
            vec(arb_sound_seed(cfg.gp_writable), 0..=MAX_SOUNDS),
            vec(arb_automation_seed(), 0..=MAX_AUTOMATIONS),
        ),
        (0..=MAX_MIDI, 0..=MAX_CAPO),
        vec((0..=highest_open).prop_map(Pitch), 1..=MAX_STRINGS),
        arb_transpose(),
        vec(arb_staff_seed(cfg, bars), 1..=MAX_STAVES_PER_TRACK),
    )
        .prop_map(
            |(
                (name, short_name),
                (kind, set),
                (midi, sounds, automations),
                (midi_program, capo),
                tuning,
                transpose,
                staves,
            )| {
                TrackSeed {
                    name,
                    short_name,
                    kind,
                    set,
                    midi,
                    sounds,
                    automations,
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

fn arb_share_denominator(gp_writable: bool) -> BoxedStrategy<i64> {
    if gp_writable {
        select(GRID_DENOMINATORS.to_vec()).boxed()
    } else {
        (1i64..=16).boxed()
    }
}

fn arb_tempo_change(gp_writable: bool) -> impl Strategy<Value = TempoChangeSeed> {
    (
        0i64..=64,
        arb_share_denominator(gp_writable),
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

fn arb_master_bar_seed(gp_writable: bool) -> impl Strategy<Value = MasterBarSeed> {
    (
        arb_time_signature(),
        arb_key_signature(),
        (any::<bool>(), option::of(any::<u8>()), any::<u8>()),
        (
            any::<bool>(),
            option::of(
                (arb_text_for(12, gp_writable), arb_text_for(3, gp_writable))
                    .prop_map(|(text, letter)| MarkerSeed { text, letter }),
            ),
        ),
        vec(arb_tempo_change(gp_writable), 0..=MAX_TEMPO_CHANGES),
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
        ContainerEntry::File {
            name,
            data: Base64Bytes(data),
        }
    })
}

fn arb_extension_seed(gp_writable: bool) -> BoxedStrategy<ExtensionSeed> {
    if gp_writable {
        (
            any::<usize>(),
            vec(
                arb_text_for(MAX_EXTRA_CHARS, true),
                1..=MAX_EXTENSION_PAYLOADS,
            ),
        )
            .prop_map(|(target, extras)| ExtensionSeed {
                target,
                payloads: Vec::new(),
                extras,
            })
            .boxed()
    } else {
        (
            any::<usize>(),
            vec(arb_payload(), 1..=MAX_EXTENSION_PAYLOADS),
        )
            .prop_map(|(target, payloads)| ExtensionSeed {
                target,
                payloads,
                extras: Vec::new(),
            })
            .boxed()
    }
}

fn arb_writable_container() -> impl Strategy<Value = Vec<ContainerEntry>> {
    let files = proptest::sample::subsequence(CONTAINER_NAMES.to_vec(), 0..=CONTAINER_NAMES.len())
        .prop_shuffle()
        .prop_flat_map(|names| {
            let count = names.len();
            (
                Just(names),
                vec(vec(any::<u8>(), 0..=MAX_CONTAINER_BYTES), count..=count),
                0..=count,
            )
        })
        .prop_map(|(names, data, position)| {
            let mut entries: Vec<ContainerEntry> = names
                .into_iter()
                .zip(data)
                .map(|(name, bytes)| ContainerEntry::File {
                    name: name.to_string(),
                    data: Base64Bytes(if name == DIRECTORY_ENTRY {
                        Vec::new()
                    } else {
                        bytes
                    }),
                })
                .collect();
            entries.insert(
                position,
                ContainerEntry::Score {
                    name: SCORE_ENTRY.to_string(),
                },
            );
            entries
        });
    prop_oneof![1 => Just(Vec::new()), 2 => files]
}

fn arb_document_seed(gp_writable: bool) -> BoxedStrategy<DocumentSeed> {
    if gp_writable {
        (
            vec(
                arb_text_for(MAX_EXTRA_CHARS, true),
                0..=MAX_EXTENSION_PAYLOADS,
            ),
            arb_writable_container(),
        )
            .prop_map(|(document_extras, container)| DocumentSeed {
                document_extensions: Vec::new(),
                document_extras,
                container,
                score_entry: None,
            })
            .boxed()
    } else {
        (
            vec(arb_payload(), 0..=MAX_EXTENSION_PAYLOADS),
            vec(arb_container_entry(), 0..=MAX_CONTAINER_ENTRIES),
            option::of(
                (any::<usize>(), arb_text(16))
                    .prop_map(|(position, name)| ScoreEntrySeed { position, name }),
            ),
        )
            .prop_map(
                |(document_extensions, container, score_entry)| DocumentSeed {
                    document_extensions,
                    document_extras: Vec::new(),
                    container,
                    score_entry,
                },
            )
            .boxed()
    }
}

fn arb_meta(gp_writable: bool) -> impl Strategy<Value = ScoreMeta> {
    let text = move || arb_meta_text(gp_writable);
    (
        (text(), text(), text()),
        (text(), text(), text()),
        (text(), text(), text()),
        text(),
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
        (arb_meta(cfg.gp_writable), any::<bool>()),
        vec(
            arb_master_bar_seed(cfg.gp_writable),
            usize::from(cfg.gp_writable)..=cfg.max_master_bars.max(usize::from(cfg.gp_writable)),
        ),
        vec(
            arb_extension_seed(cfg.gp_writable),
            0..=MAX_EXTENSION_ENTRIES,
        ),
        arb_document_seed(cfg.gp_writable),
        vec(
            (any::<usize>(), arb_text_for(12, cfg.gp_writable))
                .prop_map(|(track, bank)| OverrideSeed { track, bank }),
            0..=MAX_OVERRIDES,
        ),
    )
        .prop_flat_map(
            move |((meta, anacrusis), master_bars, extensions, document, sound_overrides)| {
                let bars = master_bars.len();
                vec(
                    arb_track_seed(cfg, bars),
                    usize::from(cfg.gp_writable)..=cfg.max_tracks.max(usize::from(cfg.gp_writable)),
                )
                .prop_map(move |tracks| ScoreSeed {
                    meta: meta.clone(),
                    anacrusis,
                    master_bars: master_bars.clone(),
                    tracks,
                    extensions: extensions.clone(),
                    document_extensions: document.document_extensions.clone(),
                    document_extras: document.document_extras.clone(),
                    container: document.container.clone(),
                    score_entry: document.score_entry.clone(),
                    sound_overrides: sound_overrides.clone(),
                })
            },
        )
}
