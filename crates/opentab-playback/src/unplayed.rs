use std::collections::BTreeMap;

use opentab_model::{
    AutomationValue, Balance, Beat, InstrumentKind, Level, MidiBank, Note, Score, Track,
};

use crate::ScoreTime;
use crate::bends::gp_drops;
use crate::harmonics::semitones;
use crate::perform::Profile;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnplayedKind {
    Ghost,
    Accent,
    Dead,
    PalmMute,
    LetRing,
    Vibrato,
    HammerOnPullOff,
    Slide,
    Tap,
    BendBelowPitch,
    DipBend,
    OffNodeHarmonic,
    TremoloBar,
    Stroke,
    FadeIn,
    Slap,
    Pop,
    TempoRamp,
    TripletFeel,
    SoundChange,
    Volume,
    Pan,
    Bank,
    OneChannelPerString,
    UnreadableDrumKit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnplayedFeature {
    pub kind: UnplayedKind,
    pub track: Option<usize>,
    pub first_bar: usize,
    pub count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlaybackWarning {
    ChannelFallback {
        track: usize,
        at: ScoreTime,
    },
    PitchOutOfRange {
        track: usize,
        at: ScoreTime,
        pitch: u16,
    },
    MissingString {
        track: usize,
        at: ScoreTime,
        string: u8,
    },
    UnknownArticulation {
        track: usize,
        at: ScoreTime,
        articulation: u16,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct FeatureKey {
    track: Option<usize>,
    kind: UnplayedKind,
}

#[derive(Debug, Clone, Copy)]
struct FeatureTally {
    first_bar: usize,
    count: u32,
}

#[derive(Debug, Default)]
struct Catalogue {
    found: BTreeMap<FeatureKey, FeatureTally>,
}

impl Catalogue {
    fn note(&mut self, track: Option<usize>, kind: UnplayedKind, bar: usize) {
        let tally = self
            .found
            .entry(FeatureKey { track, kind })
            .or_insert(FeatureTally {
                first_bar: bar,
                count: 0,
            });
        tally.first_bar = tally.first_bar.min(bar);
        tally.count = tally.count.saturating_add(1);
    }

    fn features(self) -> Vec<UnplayedFeature> {
        self.found
            .into_iter()
            .map(|(key, tally)| UnplayedFeature {
                kind: key.kind,
                track: key.track,
                first_bar: tally.first_bar,
                count: tally.count,
            })
            .collect()
    }
}

const TRIPLET_FEEL: &str = "<TripletFeel>";
const NO_TRIPLET_FEEL: &str = "<TripletFeel>None</TripletFeel>";

fn triplet_feel(score: &Score, bar: &opentab_model::MasterBar) -> bool {
    score
        .extensions
        .0
        .get(&bar.id.0)
        .into_iter()
        .flatten()
        .any(|payload| {
            payload.data.contains(TRIPLET_FEEL) && !payload.data.contains(NO_TRIPLET_FEEL)
        })
}

pub(crate) fn bend_below_pitch(note: &Note) -> bool {
    note.effects.bend.is_some_and(|bend| {
        [
            bend.origin(),
            bend.middle_start(),
            bend.middle_end(),
            bend.destination(),
        ]
        .iter()
        .any(|point| point.value() < 0)
    })
}

fn note_kinds(note: &Note, profile: Profile) -> Vec<UnplayedKind> {
    let effects = &note.effects;
    let flags = [
        (effects.ghost, UnplayedKind::Ghost),
        (effects.accent.is_some(), UnplayedKind::Accent),
        (effects.dead, UnplayedKind::Dead),
        (effects.palm_mute, UnplayedKind::PalmMute),
        (effects.let_ring, UnplayedKind::LetRing),
        (effects.vibrato.is_some(), UnplayedKind::Vibrato),
        (effects.hammer_on_pull_off, UnplayedKind::HammerOnPullOff),
        (effects.slide.is_some(), UnplayedKind::Slide),
        (effects.tap.is_some(), UnplayedKind::Tap),
        (bend_below_pitch(note), UnplayedKind::BendBelowPitch),
        (
            profile == Profile::Gp && effects.bend.is_some_and(gp_drops),
            UnplayedKind::DipBend,
        ),
        (
            effects
                .harmonic
                .is_some_and(|harmonic| semitones(harmonic.fret, profile).is_none()),
            UnplayedKind::OffNodeHarmonic,
        ),
    ];
    flags
        .into_iter()
        .filter_map(|(present, kind)| present.then_some(kind))
        .collect()
}

fn beat_kinds(beat: &Beat) -> Vec<UnplayedKind> {
    let effects = &beat.effects;
    let flags = [
        (
            effects.tremolo_bar.is_some_and(|bar| bar.moves()),
            UnplayedKind::TremoloBar,
        ),
        (effects.stroke.is_some(), UnplayedKind::Stroke),
        (effects.fade_in, UnplayedKind::FadeIn),
        (effects.slap, UnplayedKind::Slap),
        (effects.pop, UnplayedKind::Pop),
    ];
    flags
        .into_iter()
        .filter_map(|(present, kind)| present.then_some(kind))
        .collect()
}

fn track_kinds(score: &Score, track: &Track) -> Vec<(UnplayedKind, usize)> {
    let mut found = Vec::new();
    let bar_of = |automation: &opentab_model::Automation| {
        score
            .find_master_bar_index(automation.master_bar)
            .unwrap_or(0)
    };
    if track.midi.volume != Level::HALF {
        found.push((UnplayedKind::Volume, 0));
    }
    if track.midi.pan != Balance::CENTRE {
        found.push((UnplayedKind::Pan, 0));
    }
    if track.midi.bank != MidiBank::default()
        || track
            .sounds
            .iter()
            .any(|sound| sound.bank != MidiBank::default())
    {
        found.push((UnplayedKind::Bank, 0));
    }
    if track.midi.per_string {
        found.push((UnplayedKind::OneChannelPerString, 0));
    }
    if track.instrument.kind == InstrumentKind::Percussion
        && track.drum_kit.is_none()
        && score.kept_drum_kit(track.id)
    {
        found.push((UnplayedKind::UnreadableDrumKit, 0));
    }
    for automation in &track.automations {
        let kind = match automation.value {
            AutomationValue::Volume(_) => Some(UnplayedKind::Volume),
            AutomationValue::Pan(_) => Some(UnplayedKind::Pan),
            AutomationValue::Sound(sound) => track
                .sounds
                .get(sound.0)
                .filter(|chosen| {
                    chosen.program != track.instrument.midi_program
                        || chosen.bank != track.midi.bank
                })
                .map(|_| UnplayedKind::SoundChange),
        };
        if let Some(kind) = kind {
            found.push((kind, bar_of(automation)));
        }
    }
    found
}

pub(crate) fn catalogue(score: &Score, profile: Profile) -> Vec<UnplayedFeature> {
    let mut catalogue = Catalogue::default();
    for (bar, master_bar) in score.master_bars.iter().enumerate() {
        if triplet_feel(score, master_bar) {
            catalogue.note(None, UnplayedKind::TripletFeel, bar);
        }
    }
    if profile == Profile::Gp {
        for (bar, master_bar) in score.master_bars.iter().enumerate() {
            for _ in master_bar
                .tempo_changes
                .iter()
                .filter(|change| change.linear)
            {
                catalogue.note(None, UnplayedKind::TempoRamp, bar);
            }
        }
    }
    for (index, track) in score.tracks.iter().enumerate() {
        for (kind, bar) in track_kinds(score, track) {
            catalogue.note(Some(index), kind, bar);
        }
        for staff in &track.staves {
            for (bar_index, bar) in staff.bars.iter().enumerate() {
                for beat in bar.voices.iter().flat_map(|voice| voice.beats.iter()) {
                    for kind in beat_kinds(beat) {
                        catalogue.note(Some(index), kind, bar_index);
                    }
                    for note in &beat.notes {
                        for kind in note_kinds(note, profile) {
                            catalogue.note(Some(index), kind, bar_index);
                        }
                    }
                }
            }
        }
    }
    catalogue.features()
}
