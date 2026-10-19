use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

use opentab_gp::read_gp;
use opentab_model::Fraction;
use opentab_playback::{
    ConductorKind, MidiMessage, PITCH_BEND_RANGE_RPN, Performance, PlaybackOptions, Profile,
    ScoreTime, perform,
};

use crate::smf::{SmfKind, read_smf};

const QUARTERS_PER_WHOLE: i64 = 4;
const DATA_ENTRY: u8 = 6;
const RPN_MSB: u8 = 101;
const RPN_LSB: u8 = 100;
const GOLDEN_FIXTURES: usize = 31;
const TICK_TOLERANCE: i64 = 1;
const ALTERNATING: [&str; 1] = ["trill.gp"];
const GP_TEMPO_DEFECTS: [&str; 2] = ["tempo.gp", "tempo-ramp.gp"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Onset {
    tick: i64,
    channel: u8,
    key: u8,
    velocity: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Played {
    onset: Onset,
    length: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct BendAt {
    tick: i64,
    value: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct TempoAt {
    tick: i64,
    micros: u32,
}

#[derive(Debug, Default)]
struct View {
    notes: Vec<Played>,
    onsets: Vec<Onset>,
    bends: BTreeMap<u8, Vec<BendAt>>,
    programs: Vec<(u8, u8)>,
    ranges: Vec<(u8, u8)>,
    tempi: Vec<TempoAt>,
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ticks(at: ScoreTime, division: u16) -> Fraction {
    at.whole_notes() * Fraction::from_integer(QUARTERS_PER_WHOLE * i64::from(division))
}

fn rounded(value: Fraction) -> i64 {
    (value + Fraction::new(1, 2)).floor().to_integer()
}

fn pair(
    view: &mut View,
    starts: &mut BTreeMap<(u8, u8), VecDeque<Onset>>,
    channel: u8,
    key: u8,
    tick: i64,
) {
    if let Some(onset) = starts
        .get_mut(&(channel, key))
        .and_then(VecDeque::pop_front)
    {
        view.notes.push(Played {
            onset,
            length: tick - onset.tick,
        });
    }
}

fn ours(performance: &Performance, division: u16) -> View {
    let mut view = View::default();
    let mut starts: BTreeMap<(u8, u8), VecDeque<Onset>> = BTreeMap::new();
    for event in &performance.events {
        let exact = ticks(event.at, division);
        let channel = event.channel.get();
        match event.message {
            MidiMessage::NoteOn { key, velocity } => {
                let onset = Onset {
                    tick: rounded(exact),
                    channel,
                    key,
                    velocity,
                };
                view.onsets.push(onset);
                starts.entry((channel, key)).or_default().push_back(onset);
            }
            MidiMessage::NoteOff { key, .. } => {
                pair(&mut view, &mut starts, channel, key, rounded(exact));
            }
            MidiMessage::PitchBend { value } => {
                view.bends.entry(channel).or_default().push(BendAt {
                    tick: exact.floor().to_integer(),
                    value,
                })
            }
            MidiMessage::Program { program } => view.programs.push((channel, program)),
            MidiMessage::Rpn { parameter, value } if parameter == PITCH_BEND_RANGE_RPN => {
                view.ranges.push((channel, value));
            }
            other => panic!("profile gp emitted {other:?}, which GP's export never holds"),
        }
    }
    for event in &performance.conductor {
        if let ConductorKind::Tempo { micros_per_quarter } = event.kind {
            view.tempi.push(TempoAt {
                tick: rounded(ticks(event.at, division)),
                micros: micros_per_quarter,
            });
        }
    }
    view
}

fn guitar_pros(bytes: &[u8]) -> (View, u16) {
    let smf = read_smf(bytes);
    let mut view = View::default();
    let mut starts: BTreeMap<(u8, u8), VecDeque<Onset>> = BTreeMap::new();
    let mut entered: BTreeMap<u8, u8> = BTreeMap::new();
    let mut events = smf.events.clone();
    events.sort_by_key(|event| event.tick);
    for event in events {
        let tick = i64::try_from(event.tick).expect("tick");
        let channel = event.channel;
        match event.kind {
            SmfKind::NoteOn { key, velocity } => {
                let onset = Onset {
                    tick,
                    channel,
                    key,
                    velocity,
                };
                view.onsets.push(onset);
                starts.entry((channel, key)).or_default().push_back(onset);
            }
            SmfKind::NoteOff { key } => pair(&mut view, &mut starts, channel, key, tick),
            SmfKind::PitchBend { value } => view
                .bends
                .entry(channel)
                .or_default()
                .push(BendAt { tick, value }),
            SmfKind::Program { program } => view.programs.push((channel, program)),
            SmfKind::Control { controller, value } => {
                if controller == RPN_MSB || controller == RPN_LSB {
                    *entered.entry(channel).or_default() += 1;
                } else if controller == DATA_ENTRY {
                    view.ranges.push((channel, value));
                } else {
                    panic!("GP's export holds controller {controller}");
                }
            }
            SmfKind::Tempo { micros } => view.tempi.push(TempoAt { tick, micros }),
            SmfKind::Other => {}
        }
    }
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

fn distinct<T: Ord + Clone>(items: &[T]) -> Vec<T> {
    let mut items = sorted(items);
    items.dedup();
    items
}

fn compare(name: &str) {
    let bytes = std::fs::read(root().join("fixtures/gp").join(name)).expect("fixture");
    let score = read_gp(&bytes).expect("fixture reads");
    let midi =
        std::fs::read(root().join("goldens/gp").join(name).join("export.mid")).expect("golden");
    let (theirs, division) = guitar_pros(&midi);
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
    if !ALTERNATING.contains(&name) {
        let mine = sorted(&ours.notes);
        let gp = sorted(&theirs.notes);
        assert_eq!(mine.len(), gp.len(), "{name}: notes");
        for (left, right) in mine.iter().zip(&gp) {
            assert_eq!(left.onset, right.onset, "{name}");
            assert!(
                (left.length - right.length).abs() <= TICK_TOLERANCE,
                "{name}: {left:?} against {right:?}"
            );
        }
    }
    let ours_bends: BTreeMap<u8, Vec<BendAt>> = ours
        .bends
        .iter()
        .map(|(channel, bends)| (*channel, sorted(bends)))
        .collect();
    let gp_bends: BTreeMap<u8, Vec<BendAt>> = theirs
        .bends
        .iter()
        .map(|(channel, bends)| (*channel, sorted(bends)))
        .collect();
    assert_eq!(ours_bends, gp_bends, "{name}: pitch bends");
    assert_eq!(
        distinct(&ours.programs),
        distinct(&theirs.programs),
        "{name}: programs"
    );
    assert_eq!(
        distinct(&ours.ranges),
        distinct(&theirs.ranges),
        "{name}: bend ranges"
    );
    if !GP_TEMPO_DEFECTS.contains(&name) {
        assert_eq!(ours.tempi, theirs.tempi, "{name}: tempo events");
    }
}

#[test]
fn profile_gp_plays_what_guitar_pro_exports_for_every_golden() {
    let names = golden_names();
    assert_eq!(names.len(), GOLDEN_FIXTURES);
    for name in &names {
        compare(name);
    }
}
