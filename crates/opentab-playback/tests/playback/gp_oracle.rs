use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use opentab_gp::read_gp;
use opentab_model::{DirectionJump, DirectionTarget, Score};
use opentab_playback::{
    MidiMessage, Performance, PlaybackOptions, Profile, TrackLayout, UnplayedKind, export_midi,
    perform, to_ticks,
};

use crate::smf::{SmfKind, read_smf};

const GOLDEN_FIXTURES: usize = 31;
const CONDUCTOR_TRACK: usize = 0;
const DATA_ENTRY: u8 = 6;
const RPN_MSB: u8 = 101;
const RPN_LSB: u8 = 100;
const PITCH_BEND_RANGE: u16 = 0;
const SEVEN_BITS: u32 = 7;
const TICKS_PER_QUARTER_TIMES: i64 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TempoDefect {
    MicrosFloored,
    DottedQuarterAsFiveQuarters,
    TickBeforeThePosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TempoAt {
    pub tick: u64,
    pub micros: u32,
}

#[derive(Debug, Clone, Copy)]
enum Rule {
    TempoEvent {
        file: &'static str,
        gp: TempoAt,
        ours: TempoAt,
        defects: &'static [TempoDefect],
    },
    OffGridLength {
        ticks: u64,
    },
    BendTickFloored {
        ticks: u64,
    },
    OrderWithinTick,
    TrillAuxiliaryReleasedOnThePrincipal,
    CodaNeverReached,
    TripletFeel,
}

#[derive(Debug, Clone, Copy)]
struct Allowance {
    name: &'static str,
    reason: &'static str,
    rule: Rule,
}

const fn tempo_at(tick: u64, micros: u32) -> TempoAt {
    TempoAt { tick, micros }
}

const fn tempo_event(
    name: &'static str,
    file: &'static str,
    gp: TempoAt,
    ours: TempoAt,
    defects: &'static [TempoDefect],
) -> Allowance {
    Allowance {
        name,
        reason: "GP floors microseconds per quarter, writes a dotted-quarter tempo as five quarters (factor 1.25, not 1.5), or places a 3/4 quarter one tick early; we export the exact value (M3-D5)",
        rule: Rule::TempoEvent {
            file,
            gp,
            ours,
            defects,
        },
    }
}

const ALLOWED_DIFFERENCES: [Allowance; 13] = [
    tempo_event(
        "tempo-1920-floor",
        "tempo.gp",
        tempo_at(1920, 1_325_966),
        tempo_at(1920, 1_325_967),
        &[TempoDefect::MicrosFloored],
    ),
    tempo_event(
        "tempo-2880-dotted",
        "tempo.gp",
        tempo_at(2880, 480_000),
        tempo_at(2880, 400_000),
        &[TempoDefect::DottedQuarterAsFiveQuarters],
    ),
    tempo_event(
        "tempo-4320-early-floor",
        "tempo.gp",
        tempo_at(4319, 277_777),
        tempo_at(4320, 277_778),
        &[
            TempoDefect::TickBeforeThePosition,
            TempoDefect::MicrosFloored,
        ],
    ),
    tempo_event(
        "tempo-5280-dotted",
        "tempo.gp",
        tempo_at(5280, 360_225),
        tempo_at(5280, 300_188),
        &[TempoDefect::DottedQuarterAsFiveQuarters],
    ),
    tempo_event(
        "tempo-8160-floor",
        "tempo.gp",
        tempo_at(8160, 2_666_666),
        tempo_at(8160, 2_666_667),
        &[TempoDefect::MicrosFloored],
    ),
    tempo_event(
        "tempo-ramp-6720-floor",
        "tempo-ramp.gp",
        tempo_at(6720, 666_666),
        tempo_at(6720, 666_667),
        &[TempoDefect::MicrosFloored],
    ),
    tempo_event(
        "tempo-ramp-7680-dotted",
        "tempo-ramp.gp",
        tempo_at(7680, 600_000),
        tempo_at(7680, 500_000),
        &[TempoDefect::DottedQuarterAsFiveQuarters],
    ),
    Allowance {
        name: "off-grid-length",
        reason: "a tuplet note whose exact start or end falls between ticks: GP rounds each length on its own (69 for 68.57), we round both ends half up from the absolute position (M3-D4)",
        rule: Rule::OffGridLength { ticks: 1 },
    },
    Allowance {
        name: "bend-tick-floored",
        reason: "GP floors bend knot and step ticks per note and its float can land one tick under an exact tick (1919 for 1920); we round half up, so ours is GP's tick or one later",
        rule: Rule::BendTickFloored { ticks: 1 },
    },
    Allowance {
        name: "order-within-one-tick",
        reason: "GP writes a note-on before the track's program and RPN at tick 0; notes, programs and ranges are compared as multisets, bends keep their order per channel",
        rule: Rule::OrderWithinTick,
    },
    Allowance {
        name: "trill-auxiliary-release",
        reason: "GP releases a trill's auxiliary note with a note-off on the principal key, leaving the auxiliary sounding; we pair it on its own key",
        rule: Rule::TrillAuxiliaryReleasedOnThePrincipal,
    },
    Allowance {
        name: "coda-never-reached",
        reason: "when a To Coda lies on no played path, GP's export holds only the bars from the Coda on (gpif.md 5.1 rule 12, a GP defect); we play the whole order; no golden triggers it",
        rule: Rule::CodaNeverReached,
    },
    Allowance {
        name: "triplet-feel",
        reason: "GP swings eighths under triplet feel; the model keeps triplet feel opaque until M3-T11, so note times of such scores are not compared; no golden carries it",
        rule: Rule::TripletFeel,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Aspect {
    Division,
    Tracks,
    Onsets,
    Lengths,
    Unreleased,
    Bends,
    Programs,
    Ranges,
    Controllers,
    Tempi,
    TimeSignatures,
    KeySignatures,
    Expansion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Difference {
    pub aspect: Aspect,
    pub detail: String,
}

#[derive(Debug, Clone, Default)]
pub struct Verdict {
    pub differences: Vec<Difference>,
    pub used: BTreeSet<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Onset {
    pub track: usize,
    pub channel: u8,
    pub key: u8,
    pub velocity: u8,
    pub tick: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Note {
    pub onset: Onset,
    pub length: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BendAt {
    pub tick: u64,
    pub value: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TrackValue {
    pub track: usize,
    pub channel: u8,
    pub value: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Signature {
    Time { numerator: u8, denominator: u8 },
    Key { accidentals: i8, minor: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SignatureAt {
    pub tick: u64,
    pub signature: Signature,
}

#[derive(Debug, Clone, Default)]
pub struct Side {
    pub division: u16,
    pub tracks: usize,
    pub notes: Vec<Note>,
    pub unreleased: Vec<Onset>,
    pub stray_offs: Vec<Onset>,
    pub bends: BTreeMap<u8, Vec<BendAt>>,
    pub programs: BTreeSet<TrackValue>,
    pub ranges: BTreeSet<TrackValue>,
    pub controllers: Vec<String>,
    pub tempi: Vec<TempoAt>,
    pub times: Vec<SignatureAt>,
    pub keys: BTreeMap<usize, Vec<SignatureAt>>,
}

#[derive(Debug, Clone, Default)]
pub struct Context {
    pub name: String,
    pub off_grid: BTreeSet<GridKey>,
    pub bar_ticks: Vec<u64>,
    pub triplet_feel: bool,
    pub coda_never_reached: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct GridKey {
    pub lane: usize,
    pub channel: u8,
    pub key: u8,
    pub onset: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Started {
    onset: u64,
    whole: bool,
}

impl GridKey {
    fn of(onset: Onset) -> GridKey {
        GridKey {
            lane: onset.track,
            channel: onset.channel,
            key: onset.key,
            onset: onset.tick,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Slot {
    track: usize,
    channel: u8,
    key: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Selection {
    track: usize,
    channel: u8,
}

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn golden_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root().join("fixtures/gp"))
        .expect("fixtures")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.ends_with(".gp"))
        .filter(|name| {
            root()
                .join("goldens/gp")
                .join(name)
                .join("export.mid")
                .exists()
        })
        .collect();
    names.sort();
    names
}

fn tolerance(pick: fn(Rule) -> Option<u64>) -> u64 {
    ALLOWED_DIFFERENCES
        .iter()
        .find_map(|allowance| pick(allowance.rule))
        .unwrap_or(0)
}

fn off_grid_ticks() -> u64 {
    tolerance(|rule| match rule {
        Rule::OffGridLength { ticks } => Some(ticks),
        _ => None,
    })
}

fn bend_ticks() -> u64 {
    tolerance(|rule| match rule {
        Rule::BendTickFloored { ticks } => Some(ticks),
        _ => None,
    })
}

fn named(rule: fn(&Rule) -> bool) -> &'static str {
    ALLOWED_DIFFERENCES
        .iter()
        .find(|allowance| rule(&allowance.rule))
        .map_or("missing", |allowance| allowance.name)
}

pub fn side(bytes: &[u8]) -> Side {
    let smf = read_smf(bytes);
    let mut side = Side {
        division: smf.division,
        tracks: smf
            .events
            .iter()
            .map(|event| event.track + 1)
            .max()
            .unwrap_or(0),
        ..Side::default()
    };
    let mut open: BTreeMap<Slot, Vec<Onset>> = BTreeMap::new();
    let mut selects: BTreeMap<Selection, Vec<(u8, u8)>> = BTreeMap::new();
    let mut events = smf.events.clone();
    events.sort_by_key(|event| (event.track, event.tick));
    for event in events {
        let track = event.track;
        let channel = event.channel;
        let tick = event.tick;
        match event.kind {
            SmfKind::NoteOn { key, velocity } => {
                open.entry(Slot {
                    track,
                    channel,
                    key,
                })
                .or_default()
                .push(Onset {
                    track,
                    channel,
                    key,
                    velocity,
                    tick,
                });
            }
            SmfKind::NoteOff { key } => {
                let slot = Slot {
                    track,
                    channel,
                    key,
                };
                let started = open
                    .get_mut(&slot)
                    .filter(|starts| !starts.is_empty())
                    .map(|starts| starts.remove(0));
                match started {
                    Some(onset) => side.notes.push(Note {
                        onset,
                        length: tick.saturating_sub(onset.tick),
                    }),
                    None => side.stray_offs.push(Onset {
                        track,
                        channel,
                        key,
                        velocity: 0,
                        tick,
                    }),
                }
            }
            SmfKind::PitchBend { value } => side
                .bends
                .entry(channel)
                .or_default()
                .push(BendAt { tick, value }),
            SmfKind::Program { program } => {
                side.programs.insert(TrackValue {
                    track,
                    channel,
                    value: program,
                });
            }
            SmfKind::Control { controller, value } => {
                let selection = Selection { track, channel };
                if controller == RPN_MSB || controller == RPN_LSB {
                    selects
                        .entry(selection)
                        .or_default()
                        .push((controller, value));
                } else if controller == DATA_ENTRY {
                    let selected = selects.remove(&selection).unwrap_or_default();
                    let parameter = match selected.as_slice() {
                        [(RPN_MSB, msb), (RPN_LSB, lsb)] => {
                            Some((u16::from(*msb) << SEVEN_BITS) | u16::from(*lsb))
                        }
                        _ => None,
                    };
                    if parameter == Some(PITCH_BEND_RANGE) {
                        side.ranges.insert(TrackValue {
                            track,
                            channel,
                            value,
                        });
                    } else {
                        side.controllers
                            .push(format!("data entry after {selected:?} at {tick}"));
                    }
                } else {
                    side.controllers
                        .push(format!("controller {controller} = {value} at {tick}"));
                }
            }
            SmfKind::Tempo { micros } => side.tempi.push(TempoAt { tick, micros }),
            SmfKind::TimeSignature {
                numerator,
                denominator,
            } if track == CONDUCTOR_TRACK => side.times.push(SignatureAt {
                tick,
                signature: Signature::Time {
                    numerator,
                    denominator,
                },
            }),
            SmfKind::KeySignature { accidentals, minor } if track != CONDUCTOR_TRACK => {
                side.keys.entry(track).or_default().push(SignatureAt {
                    tick,
                    signature: Signature::Key { accidentals, minor },
                });
            }
            SmfKind::TimeSignature { .. } | SmfKind::KeySignature { .. } => side
                .controllers
                .push(format!("a signature on track {track} at {tick}")),
            SmfKind::Other => {}
        }
    }
    side.unreleased = open.into_values().flatten().collect();
    side.unreleased.sort();
    side.stray_offs.sort();
    side.tempi.sort();
    for (selection, selected) in selects {
        side.controllers.push(format!(
            "an RPN selection without data entry: {selection:?} {selected:?}"
        ));
    }
    side
}

fn lanes(performance: &Performance) -> Vec<usize> {
    let mut first = Vec::with_capacity(performance.tracks.len());
    let mut next = CONDUCTOR_TRACK + 1;
    for setup in &performance.tracks {
        first.push(next);
        next += match performance.profile.track_layout() {
            TrackLayout::PerStaff => setup.staves,
            TrackLayout::PerTrack => 1,
        };
    }
    first
}

fn coda_never_reached(score: &Score, played: &BTreeSet<usize>) -> bool {
    let pairs = [
        (DirectionJump::DaCoda, DirectionTarget::Coda),
        (DirectionJump::DaDoubleCoda, DirectionTarget::DoubleCoda),
    ];
    pairs.iter().any(|(jump, target)| {
        let last_jump = score
            .master_bars
            .iter()
            .rposition(|bar| bar.directions.jumps.contains(jump));
        let target_bar = score
            .master_bars
            .iter()
            .rposition(|bar| bar.directions.targets.contains(target));
        match (last_jump, target_bar) {
            (Some(jump_bar), Some(target_bar)) => {
                jump_bar < target_bar && !played.contains(&jump_bar)
            }
            _ => false,
        }
    })
}

pub fn context(name: &str, score: &Score, performance: &Performance) -> Context {
    let ppq = performance.profile.ppq();
    let first = lanes(performance);
    let mut off_grid = BTreeSet::new();
    let mut open: BTreeMap<Slot, VecDeque<Started>> = BTreeMap::new();
    for event in &performance.events {
        let (key, on) = match event.message {
            MidiMessage::NoteOn { key, .. } => (key, true),
            MidiMessage::NoteOff { key, .. } => (key, false),
            _ => continue,
        };
        let lane = match performance.profile.track_layout() {
            TrackLayout::PerStaff => first[event.track] + event.staff,
            TrackLayout::PerTrack => first[event.track],
        };
        let slot = Slot {
            track: lane,
            channel: event.channel.get(),
            key,
        };
        let exact = event.at.whole_notes() * (TICKS_PER_QUARTER_TIMES * i64::from(ppq.get()));
        if on {
            open.entry(slot).or_default().push_back(Started {
                onset: to_ticks(event.at, ppq),
                whole: exact.is_integer(),
            });
        } else if let Some(started) = open.get_mut(&slot).and_then(VecDeque::pop_front)
            && !(started.whole && exact.is_integer())
        {
            off_grid.insert(GridKey {
                lane,
                channel: slot.channel,
                key,
                onset: started.onset,
            });
        }
    }
    let played: BTreeSet<usize> = performance.order.sources().into_iter().collect();
    Context {
        name: name.to_string(),
        off_grid,
        bar_ticks: performance
            .order
            .bars
            .iter()
            .map(|bar| to_ticks(bar.start, ppq))
            .collect(),
        triplet_feel: performance
            .unplayed
            .iter()
            .any(|feature| feature.kind == UnplayedKind::TripletFeel),
        coda_never_reached: coda_never_reached(score, &played),
    }
}

fn differ(verdict: &mut Verdict, aspect: Aspect, detail: String) {
    verdict.differences.push(Difference { aspect, detail });
}

fn expected_tempi(name: &str, gp: &[TempoAt], verdict: &mut Verdict) -> Vec<TempoAt> {
    let mut out: Vec<TempoAt> = gp
        .iter()
        .map(|event| {
            ALLOWED_DIFFERENCES
                .iter()
                .find_map(|allowance| match allowance.rule {
                    Rule::TempoEvent {
                        file,
                        gp: theirs,
                        ours,
                        defects,
                    } if file == name && theirs == *event && !defects.is_empty() => {
                        verdict.used.insert(allowance.name);
                        Some(ours)
                    }
                    _ => None,
                })
                .unwrap_or(*event)
        })
        .collect();
    out.sort();
    out
}

fn expansion(notes: &[Note], bar_ticks: &[u64]) -> Vec<usize> {
    let mut counts = vec![0; bar_ticks.len()];
    for note in notes {
        let bar = bar_ticks.partition_point(|start| *start <= note.onset.tick);
        if let Some(count) = bar.checked_sub(1).and_then(|index| counts.get_mut(index)) {
            *count += 1;
        }
    }
    counts
}

fn compare_notes(context: &Context, ours: &Side, gp: &Side, verdict: &mut Verdict) {
    if !ours.unreleased.is_empty() || !ours.stray_offs.is_empty() {
        differ(
            verdict,
            Aspect::Unreleased,
            format!("ours: {:?} {:?}", ours.unreleased, ours.stray_offs),
        );
    }
    let mut auxiliaries = gp.unreleased.clone();
    let mut mine = Vec::with_capacity(ours.notes.len());
    let mut excused = 0;
    for note in &ours.notes {
        match auxiliaries.iter().position(|onset| *onset == note.onset) {
            Some(index) => {
                auxiliaries.remove(index);
                excused += 1;
            }
            None => mine.push(*note),
        }
    }
    if excused > 0 {
        verdict.used.insert(named(|rule| {
            matches!(rule, Rule::TrillAuxiliaryReleasedOnThePrincipal)
        }));
    }
    if !auxiliaries.is_empty() || gp.stray_offs.len() != excused {
        differ(
            verdict,
            Aspect::Unreleased,
            format!(
                "GP leaves {:?} unmatched and releases {} on another key, against {excused} auxiliaries of ours",
                auxiliaries,
                gp.stray_offs.len()
            ),
        );
    }
    let mut theirs = gp.notes.clone();
    if context.triplet_feel {
        verdict
            .used
            .insert(named(|rule| matches!(rule, Rule::TripletFeel)));
        let pitches = |notes: &[Note]| -> Vec<(usize, u8, u8, u8)> {
            let mut out: Vec<(usize, u8, u8, u8)> = notes
                .iter()
                .map(|note| {
                    (
                        note.onset.track,
                        note.onset.channel,
                        note.onset.key,
                        note.onset.velocity,
                    )
                })
                .collect();
            out.sort();
            out
        };
        if pitches(&mine) != pitches(&theirs) {
            differ(
                verdict,
                Aspect::Onsets,
                "pitches under triplet feel".to_string(),
            );
        }
        return;
    }
    mine.sort();
    theirs.sort();
    let counts = expansion(&mine, &context.bar_ticks);
    if counts != expansion(&theirs, &context.bar_ticks) {
        differ(
            verdict,
            Aspect::Expansion,
            format!("onsets per played bar {counts:?}"),
        );
    }
    let onsets = |notes: &[Note]| -> Vec<Onset> { notes.iter().map(|note| note.onset).collect() };
    if onsets(&mine) != onsets(&theirs) {
        let first = onsets(&mine)
            .into_iter()
            .zip(onsets(&theirs))
            .find(|(left, right)| left != right);
        differ(
            verdict,
            Aspect::Onsets,
            format!(
                "{} notes against {}; first difference {first:?}",
                mine.len(),
                theirs.len()
            ),
        );
        return;
    }
    let allowed = off_grid_ticks();
    for (left, right) in mine.iter().zip(&theirs) {
        let off_grid = context.off_grid.contains(&GridKey::of(left.onset));
        let gap = left.length.abs_diff(right.length);
        if gap == 0 {
            continue;
        }
        if off_grid && gap <= allowed {
            verdict
                .used
                .insert(named(|rule| matches!(rule, Rule::OffGridLength { .. })));
            continue;
        }
        differ(
            verdict,
            Aspect::Lengths,
            format!("{left:?} against {right:?}"),
        );
    }
}

fn compare_bends(ours: &Side, gp: &Side, verdict: &mut Verdict) {
    let allowed = bend_ticks();
    let channels: BTreeSet<u8> = ours.bends.keys().chain(gp.bends.keys()).copied().collect();
    for channel in channels {
        let mine = ours.bends.get(&channel).cloned().unwrap_or_default();
        let theirs = gp.bends.get(&channel).cloned().unwrap_or_default();
        if mine.len() != theirs.len() {
            differ(
                verdict,
                Aspect::Bends,
                format!(
                    "channel {channel}: {} bends against {}",
                    mine.len(),
                    theirs.len()
                ),
            );
            continue;
        }
        for (left, right) in mine.iter().zip(&theirs) {
            let late = left.tick.checked_sub(right.tick);
            match late {
                Some(0) if left.value == right.value => {}
                Some(late) if left.value == right.value && late <= allowed => {
                    verdict
                        .used
                        .insert(named(|rule| matches!(rule, Rule::BendTickFloored { .. })));
                }
                _ => differ(
                    verdict,
                    Aspect::Bends,
                    format!("channel {channel}: {left:?} against {right:?}"),
                ),
            }
        }
    }
}

pub fn compare(context: &Context, ours: &Side, gp: &Side) -> Verdict {
    let mut verdict = Verdict::default();
    verdict
        .used
        .insert(named(|rule| matches!(rule, Rule::OrderWithinTick)));
    if ours.division != gp.division {
        differ(
            &mut verdict,
            Aspect::Division,
            format!("{} against {}", ours.division, gp.division),
        );
    }
    if ours.tracks != gp.tracks {
        differ(
            &mut verdict,
            Aspect::Tracks,
            format!("{} against {}", ours.tracks, gp.tracks),
        );
    }
    if context.coda_never_reached {
        verdict
            .used
            .insert(named(|rule| matches!(rule, Rule::CodaNeverReached)));
    } else {
        compare_notes(context, ours, gp, &mut verdict);
        compare_bends(ours, gp, &mut verdict);
    }
    if ours.programs != gp.programs {
        differ(
            &mut verdict,
            Aspect::Programs,
            format!("{:?} against {:?}", ours.programs, gp.programs),
        );
    }
    if ours.ranges != gp.ranges {
        differ(
            &mut verdict,
            Aspect::Ranges,
            format!("{:?} against {:?}", ours.ranges, gp.ranges),
        );
    }
    if !ours.controllers.is_empty() || !gp.controllers.is_empty() {
        differ(
            &mut verdict,
            Aspect::Controllers,
            format!("{:?} against {:?}", ours.controllers, gp.controllers),
        );
    }
    let expected = expected_tempi(&context.name, &gp.tempi, &mut verdict);
    if ours.tempi != expected {
        differ(
            &mut verdict,
            Aspect::Tempi,
            format!("{:?} against {expected:?}", ours.tempi),
        );
    }
    if ours.times != gp.times {
        differ(
            &mut verdict,
            Aspect::TimeSignatures,
            format!("{:?} against {:?}", ours.times, gp.times),
        );
    }
    if ours.keys != gp.keys {
        differ(
            &mut verdict,
            Aspect::KeySignatures,
            format!("{:?} against {:?}", ours.keys, gp.keys),
        );
    }
    verdict
}

pub struct Case {
    pub context: Context,
    pub ours: Side,
    pub gp: Side,
}

pub fn case(name: &str) -> Case {
    let bytes = std::fs::read(root().join("fixtures/gp").join(name)).expect("fixture");
    let score = read_gp(&bytes).expect("fixture reads");
    let golden =
        std::fs::read(root().join("goldens/gp").join(name).join("export.mid")).expect("golden");
    let performance = perform(
        &score,
        &PlaybackOptions {
            profile: Profile::Gp,
        },
    )
    .expect("perform");
    let ours = export_midi(&score, Profile::Gp).expect("export");
    Case {
        context: context(name, &score, &performance),
        ours: side(&ours),
        gp: side(&golden),
    }
}

#[test]
fn profile_gp_exports_what_guitar_pro_exports_for_every_golden() {
    let names = golden_names();
    assert_eq!(names.len(), GOLDEN_FIXTURES);
    let mut used = BTreeSet::new();
    for name in &names {
        let case = case(name);
        let verdict = compare(&case.context, &case.ours, &case.gp);
        assert!(
            verdict.differences.is_empty(),
            "{name}: {:#?}",
            verdict.differences
        );
        used.extend(verdict.used);
    }
    for allowance in &ALLOWED_DIFFERENCES {
        assert!(!allowance.reason.is_empty());
        let declared_only = matches!(allowance.rule, Rule::CodaNeverReached | Rule::TripletFeel);
        assert_eq!(
            used.contains(allowance.name),
            !declared_only,
            "{}: used by the goldens exactly when it is not a declared-only allowance",
            allowance.name
        );
        if let Rule::TempoEvent { file, .. } = allowance.rule {
            assert!(names.iter().any(|name| name == file));
        }
    }
    let unique: BTreeSet<&str> = ALLOWED_DIFFERENCES
        .iter()
        .map(|allowance| allowance.name)
        .collect();
    assert_eq!(unique.len(), ALLOWED_DIFFERENCES.len());
}

fn aspects(case: &Case, ours: &Side) -> BTreeSet<Aspect> {
    compare(&case.context, ours, &case.gp)
        .differences
        .iter()
        .map(|difference| difference.aspect)
        .collect()
}

fn rejects(case: &Case, ours: &Side, aspect: Aspect) {
    assert!(
        aspects(case, ours).contains(&aspect),
        "{}: the comparator must report {aspect:?}",
        case.context.name
    );
}

fn accepts(case: &Case, ours: &Side) {
    assert_eq!(
        aspects(case, ours),
        BTreeSet::new(),
        "{}",
        case.context.name
    );
}

fn off_grid_note(case: &Case) -> usize {
    case.ours
        .notes
        .iter()
        .position(|note| case.context.off_grid.contains(&GridKey::of(note.onset)))
        .expect("an off-grid note")
}

#[test]
fn the_comparator_rejects_every_difference_no_allowance_covers() {
    let durations = case("durations.gp");
    accepts(&durations, &durations.ours);
    let index = off_grid_note(&durations);
    for delta in [2, 100, 480] {
        let mut longer = durations.ours.clone();
        longer.notes[index].length += delta;
        rejects(&durations, &longer, Aspect::Lengths);
        let mut shorter = durations.ours.clone();
        shorter.notes[index].length -= delta.min(shorter.notes[index].length - 1);
        rejects(&durations, &shorter, Aspect::Lengths);
    }
    let whole = durations
        .ours
        .notes
        .iter()
        .position(|note| {
            !durations
                .context
                .off_grid
                .contains(&GridKey::of(note.onset))
        })
        .expect("an on-grid note");
    let mut one_tick = durations.ours.clone();
    one_tick.notes[whole].length += 1;
    rejects(&durations, &one_tick, Aspect::Lengths);

    let effects = case("note-effects.gp");
    accepts(&effects, &effects.ours);
    let channel = *effects.ours.bends.keys().next().expect("a bent channel");
    let mut late = effects.ours.clone();
    late.bends.get_mut(&channel).expect("bends")[3].tick += 2;
    rejects(&effects, &late, Aspect::Bends);
    let mut early = effects.ours.clone();
    let gp_tick = effects.gp.bends[&channel][3].tick;
    early.bends.get_mut(&channel).expect("bends")[3].tick = gp_tick - 1;
    rejects(&effects, &early, Aspect::Bends);
    let mut value = effects.ours.clone();
    value.bends.get_mut(&channel).expect("bends")[3].value += 1;
    rejects(&effects, &value, Aspect::Bends);
    let mut velocity = effects.ours.clone();
    velocity.notes[0].onset.velocity += 1;
    rejects(&effects, &velocity, Aspect::Onsets);
    let mut moved = effects.ours.clone();
    moved.notes[0].onset.channel += 1;
    rejects(&effects, &moved, Aspect::Onsets);
    let mut missing = effects.ours.clone();
    missing.notes.pop();
    rejects(&effects, &missing, Aspect::Onsets);
    let mut program = effects.ours.clone();
    let first = *program.programs.iter().next().expect("a program");
    program.programs.remove(&first);
    program.programs.insert(TrackValue {
        value: first.value + 1,
        ..first
    });
    rejects(&effects, &program, Aspect::Programs);
    let mut controller = effects.ours.clone();
    controller.controllers.push("controller 7".to_string());
    rejects(&effects, &controller, Aspect::Controllers);
    let mut division = effects.ours.clone();
    division.division *= 2;
    rejects(&effects, &division, Aspect::Division);

    let tempo = case("tempo.gp");
    accepts(&tempo, &tempo.ours);
    let mut copied_defect = tempo.ours.clone();
    copied_defect.tempi = tempo.gp.tempi.clone();
    rejects(&tempo, &copied_defect, Aspect::Tempi);
    let mut first_tempo = tempo.ours.clone();
    first_tempo.tempi[0].micros += 1;
    rejects(&tempo, &first_tempo, Aspect::Tempi);
    let mut signature = tempo.ours.clone();
    signature.times.pop();
    rejects(&tempo, &signature, Aspect::TimeSignatures);

    let repeats = case("repeats.gp");
    accepts(&repeats, &repeats.ours);
    let mut keys = repeats.ours.clone();
    keys.keys.values_mut().next().expect("keys").pop();
    rejects(&repeats, &keys, Aspect::KeySignatures);
    let mut unrolled = repeats.ours.clone();
    let second_bar = repeats.context.bar_ticks[1];
    unrolled.notes.retain(|note| note.onset.tick < second_bar);
    rejects(&repeats, &unrolled, Aspect::Expansion);

    let trill = case("trill.gp");
    accepts(&trill, &trill.ours);
    let mut gp_paired = Case {
        context: trill.context.clone(),
        ours: trill.ours.clone(),
        gp: trill.gp.clone(),
    };
    gp_paired.gp.stray_offs.pop();
    rejects(&gp_paired, &trill.ours, Aspect::Unreleased);
}
