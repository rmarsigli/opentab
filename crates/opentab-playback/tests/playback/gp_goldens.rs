use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use opentab_gp::read_gp;
use opentab_model::{Fraction, Score};
use opentab_playback::{
    ConductorKind, MidiMessage, PITCH_BEND_RANGE_RPN, Performance, PlaybackOptions, Profile,
    ScoreTime, perform,
};

use crate::smf::{SmfKind, read_smf};

const QUARTERS_PER_WHOLE: i64 = 4;
const DATA_ENTRY: u8 = 6;
const RPN_MSB: u8 = 101;
const RPN_LSB: u8 = 100;
const RPN_SELECT: [(u8, u8); 2] = [(RPN_MSB, 0), (RPN_LSB, 0)];
const GOLDEN_FIXTURES: usize = 31;
const TICK_TOLERANCE: i64 = 1;
const TRILL_FIXTURE: &str = "trill.gp";
const CONDUCTOR_TRACK: usize = 0;
const FIRST_NOTE_TRACK: usize = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TempoDefect {
    MicrosFloored,
    DottedQuarterAsFiveQuarters,
    TickBeforeThePosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct TempoAt {
    tick: i64,
    micros: u32,
}

#[derive(Debug, Clone, Copy)]
struct TempoAllowance {
    file: &'static str,
    gp: TempoAt,
    ours: TempoAt,
    defects: &'static [TempoDefect],
}

const fn tempo_at(tick: i64, micros: u32) -> TempoAt {
    TempoAt { tick, micros }
}

const ALLOWED_TEMPI: [TempoAllowance; 7] = [
    TempoAllowance {
        file: "tempo.gp",
        gp: tempo_at(1920, 1_325_966),
        ours: tempo_at(1920, 1_325_967),
        defects: &[TempoDefect::MicrosFloored],
    },
    TempoAllowance {
        file: "tempo.gp",
        gp: tempo_at(2880, 480_000),
        ours: tempo_at(2880, 400_000),
        defects: &[TempoDefect::DottedQuarterAsFiveQuarters],
    },
    TempoAllowance {
        file: "tempo.gp",
        gp: tempo_at(4319, 277_777),
        ours: tempo_at(4320, 277_778),
        defects: &[
            TempoDefect::TickBeforeThePosition,
            TempoDefect::MicrosFloored,
        ],
    },
    TempoAllowance {
        file: "tempo.gp",
        gp: tempo_at(5280, 360_225),
        ours: tempo_at(5280, 300_188),
        defects: &[TempoDefect::DottedQuarterAsFiveQuarters],
    },
    TempoAllowance {
        file: "tempo.gp",
        gp: tempo_at(8160, 2_666_666),
        ours: tempo_at(8160, 2_666_667),
        defects: &[TempoDefect::MicrosFloored],
    },
    TempoAllowance {
        file: "tempo-ramp.gp",
        gp: tempo_at(6720, 666_666),
        ours: tempo_at(6720, 666_667),
        defects: &[TempoDefect::MicrosFloored],
    },
    TempoAllowance {
        file: "tempo-ramp.gp",
        gp: tempo_at(7680, 600_000),
        ours: tempo_at(7680, 500_000),
        defects: &[TempoDefect::DottedQuarterAsFiveQuarters],
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Onset {
    tick: i64,
    track: usize,
    channel: u8,
    key: u8,
    velocity: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Played {
    onset: Onset,
    length: i64,
    whole_ticks: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct BendAt {
    tick: i64,
    value: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct TrackValue {
    track: usize,
    channel: u8,
    value: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SignatureAt {
    tick: i64,
    kind: ConductorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct OpenNote {
    onset: Onset,
    whole: bool,
}

#[derive(Debug, Default)]
struct View {
    notes: Vec<Played>,
    onsets: Vec<Onset>,
    orphans: usize,
    bends: BTreeMap<u8, Vec<BendAt>>,
    programs: BTreeSet<TrackValue>,
    ranges: BTreeSet<TrackValue>,
    tempi: Vec<TempoAt>,
    times: Vec<SignatureAt>,
    keys: Vec<SignatureAt>,
}

type Open = BTreeMap<(usize, u8, u8), VecDeque<OpenNote>>;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ticks(at: ScoreTime, division: u16) -> Fraction {
    at.whole_notes() * Fraction::from_integer(QUARTERS_PER_WHOLE * i64::from(division))
}

fn rounded(value: Fraction) -> i64 {
    (value + Fraction::new(1, 2)).floor().to_integer()
}

fn release(view: &mut View, open: &mut Open, onset: Onset, tick: i64, whole: bool) {
    match open
        .get_mut(&(onset.track, onset.channel, onset.key))
        .and_then(VecDeque::pop_front)
    {
        Some(start) => view.notes.push(Played {
            onset: start.onset,
            length: tick - start.onset.tick,
            whole_ticks: start.whole && whole,
        }),
        None => view.orphans += 1,
    }
}

fn ours(performance: &Performance, division: u16) -> View {
    let mut view = View::default();
    let mut open = Open::new();
    for event in &performance.events {
        let exact = ticks(event.at, division);
        let whole = exact.is_integer();
        let channel = event.channel.get();
        let track = event.track;
        match event.message {
            MidiMessage::NoteOn { key, velocity } => {
                let onset = Onset {
                    tick: rounded(exact),
                    track,
                    channel,
                    key,
                    velocity,
                };
                view.onsets.push(onset);
                open.entry((track, channel, key))
                    .or_default()
                    .push_back(OpenNote { onset, whole });
            }
            MidiMessage::NoteOff { key, .. } => {
                let onset = Onset {
                    tick: rounded(exact),
                    track,
                    channel,
                    key,
                    velocity: 0,
                };
                release(&mut view, &mut open, onset, rounded(exact), whole);
            }
            MidiMessage::PitchBend { value } => {
                view.bends.entry(channel).or_default().push(BendAt {
                    tick: exact.floor().to_integer(),
                    value,
                });
            }
            MidiMessage::Program { program } => {
                view.programs.insert(TrackValue {
                    track,
                    channel,
                    value: program,
                });
            }
            MidiMessage::Rpn { parameter, value } if parameter == PITCH_BEND_RANGE_RPN => {
                view.ranges.insert(TrackValue {
                    track,
                    channel,
                    value,
                });
            }
            other => panic!("profile gp emitted {other:?}, which GP's export never holds"),
        }
    }
    for event in &performance.conductor {
        let tick = rounded(ticks(event.at, division));
        match event.kind {
            ConductorKind::Tempo { micros_per_quarter } => view.tempi.push(TempoAt {
                tick,
                micros: micros_per_quarter,
            }),
            ConductorKind::TimeSignature { .. } => view.times.push(SignatureAt {
                tick,
                kind: event.kind,
            }),
            ConductorKind::KeySignature { .. } => view.keys.push(SignatureAt {
                tick,
                kind: event.kind,
            }),
        }
    }
    view
}

fn staff_tracks(score: &Score) -> Vec<usize> {
    score
        .tracks
        .iter()
        .enumerate()
        .flat_map(|(index, track)| std::iter::repeat_n(index, track.staves.len()))
        .collect()
}

fn guitar_pros(bytes: &[u8], score: &Score) -> (View, u16) {
    let smf = read_smf(bytes);
    let owners = staff_tracks(score);
    let owner = |midi_track: usize| -> usize {
        *midi_track
            .checked_sub(FIRST_NOTE_TRACK)
            .and_then(|staff| owners.get(staff))
            .expect("one MIDI track per staff")
    };
    let mut view = View::default();
    let mut open = Open::new();
    let mut selects: BTreeMap<usize, Vec<(u8, u8)>> = BTreeMap::new();
    let mut events = smf.events.clone();
    events.sort_by_key(|event| event.tick);
    for event in events {
        let tick = i64::try_from(event.tick).expect("tick");
        let channel = event.channel;
        match event.kind {
            SmfKind::NoteOn { key, velocity } => {
                let onset = Onset {
                    tick,
                    track: owner(event.track),
                    channel,
                    key,
                    velocity,
                };
                view.onsets.push(onset);
                open.entry((onset.track, channel, key))
                    .or_default()
                    .push_back(OpenNote { onset, whole: true });
            }
            SmfKind::NoteOff { key } => {
                let onset = Onset {
                    tick,
                    track: owner(event.track),
                    channel,
                    key,
                    velocity: 0,
                };
                release(&mut view, &mut open, onset, tick, true);
            }
            SmfKind::PitchBend { value } => view
                .bends
                .entry(channel)
                .or_default()
                .push(BendAt { tick, value }),
            SmfKind::Program { program } => {
                view.programs.insert(TrackValue {
                    track: owner(event.track),
                    channel,
                    value: program,
                });
            }
            SmfKind::Control { controller, value } => {
                if controller == RPN_MSB || controller == RPN_LSB {
                    selects
                        .entry(event.track)
                        .or_default()
                        .push((controller, value));
                } else if controller == DATA_ENTRY {
                    assert_eq!(
                        selects.remove(&event.track),
                        Some(RPN_SELECT.to_vec()),
                        "GP selects RPN 0 before the data entry"
                    );
                    view.ranges.insert(TrackValue {
                        track: owner(event.track),
                        channel,
                        value,
                    });
                } else {
                    panic!("GP's export holds controller {controller}");
                }
            }
            SmfKind::Tempo { micros } => view.tempi.push(TempoAt { tick, micros }),
            SmfKind::TimeSignature {
                numerator,
                denominator,
            } if event.track == CONDUCTOR_TRACK => view.times.push(SignatureAt {
                tick,
                kind: ConductorKind::TimeSignature {
                    numerator,
                    denominator,
                },
            }),
            SmfKind::KeySignature { accidentals, minor } if event.track == FIRST_NOTE_TRACK => {
                view.keys.push(SignatureAt {
                    tick,
                    kind: ConductorKind::KeySignature { accidentals, minor },
                });
            }
            SmfKind::TimeSignature { .. } | SmfKind::KeySignature { .. } | SmfKind::Other => {}
        }
    }
    assert!(selects.is_empty(), "an RPN selection without data entry");
    (view, smf.division)
}

fn golden_names() -> Vec<String> {
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

fn sorted<T: Ord + Clone>(items: &[T]) -> Vec<T> {
    let mut items = items.to_vec();
    items.sort();
    items
}

fn by_onset(notes: &[Played]) -> Vec<Played> {
    let mut notes = notes.to_vec();
    notes.sort_by_key(|note| (note.onset, note.length));
    notes
}

fn allowed_tempi(name: &str, theirs: &[TempoAt]) -> Vec<TempoAt> {
    let allowances: Vec<&TempoAllowance> = ALLOWED_TEMPI
        .iter()
        .filter(|allowance| allowance.file == name)
        .collect();
    for allowance in &allowances {
        assert!(
            theirs.contains(&allowance.gp),
            "{name}: allowance {allowance:?} matches no GP event"
        );
        assert!(!allowance.defects.is_empty());
    }
    theirs
        .iter()
        .map(|event| {
            allowances
                .iter()
                .find(|allowance| allowance.gp == *event)
                .map_or(*event, |allowance| allowance.ours)
        })
        .collect()
}

fn compare_notes(name: &str, ours: &View, theirs: &View) {
    let gp_keys: BTreeSet<u8> = theirs.notes.iter().map(|note| note.onset.key).collect();
    let (mine, unreleased): (Vec<Played>, Vec<Played>) = if name == TRILL_FIXTURE {
        ours.notes
            .iter()
            .partition(|note| gp_keys.contains(&note.onset.key))
    } else {
        (ours.notes.clone(), Vec::new())
    };
    assert_eq!(
        unreleased.len(),
        theirs.orphans,
        "{name}: GP's unreleased notes"
    );
    let mine = by_onset(&mine);
    let gp = by_onset(&theirs.notes);
    assert_eq!(mine.len(), gp.len(), "{name}: notes");
    for (left, right) in mine.iter().zip(&gp) {
        assert_eq!(left.onset, right.onset, "{name}");
        let tolerance = if left.whole_ticks { 0 } else { TICK_TOLERANCE };
        assert!(
            (left.length - right.length).abs() <= tolerance,
            "{name}: {left:?} against {right:?}"
        );
    }
}

fn compare(name: &str) {
    let bytes = std::fs::read(root().join("fixtures/gp").join(name)).expect("fixture");
    let score = read_gp(&bytes).expect("fixture reads");
    let midi =
        std::fs::read(root().join("goldens/gp").join(name).join("export.mid")).expect("golden");
    let (theirs, division) = guitar_pros(&midi, &score);
    let options = PlaybackOptions {
        profile: Profile::Gp,
    };
    let performance = perform(&score, &options).expect("perform");
    let ours = ours(&performance, division);
    assert_eq!(
        sorted(&ours.onsets),
        sorted(&theirs.onsets),
        "{name}: note onsets"
    );
    compare_notes(name, &ours, &theirs);
    assert_eq!(ours.bends, theirs.bends, "{name}: pitch bends in order");
    assert_eq!(ours.programs, theirs.programs, "{name}: programs");
    assert_eq!(ours.ranges, theirs.ranges, "{name}: bend ranges");
    assert_eq!(
        ours.tempi,
        allowed_tempi(name, &theirs.tempi),
        "{name}: tempo events"
    );
    assert_eq!(ours.times, theirs.times, "{name}: time signatures");
    assert_eq!(ours.keys, theirs.keys, "{name}: key signatures");
}

#[test]
fn profile_gp_plays_what_guitar_pro_exports_for_every_golden() {
    let names = golden_names();
    assert_eq!(names.len(), GOLDEN_FIXTURES);
    for name in &names {
        compare(name);
    }
    let files: BTreeSet<&str> = ALLOWED_TEMPI
        .iter()
        .map(|allowance| allowance.file)
        .collect();
    assert!(
        files
            .iter()
            .all(|file| names.iter().any(|name| name == file))
    );
}
