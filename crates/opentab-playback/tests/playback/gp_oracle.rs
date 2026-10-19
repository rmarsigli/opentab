use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use opentab_gp::read_gp;
use opentab_model::{DirectionJump, DirectionTarget, Score};
use opentab_playback::{
    MidiMessage, Performance, PlaybackOptions, Profile, TrackLayout, UnplayedKind, export_midi,
    perform, to_ticks,
};

use crate::oracle_allowances::{
    ALLOWED_DIFFERENCES, Rule, TempoAt, bend_ticks, explains, named, off_grid_ticks,
};
use crate::oracle_side::{CONDUCTOR_TRACK, Note, Onset, Selection, Side, Slot, TrackValue, side};

const GOLDEN_FIXTURES: usize = 31;
const TICKS_PER_QUARTER_TIMES: i64 = 4;

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
    let mut releases = gp.stray_offs.clone();
    let mut mine = Vec::with_capacity(ours.notes.len());
    let mut excused = 0;
    for note in &ours.notes {
        let held = auxiliaries.iter().position(|onset| *onset == note.onset);
        let released = releases.iter().position(|off| {
            off.track == note.onset.track
                && off.channel == note.onset.channel
                && off.key != note.onset.key
                && off.tick == note.onset.tick + note.length
        });
        match (held, released) {
            (Some(held), Some(released)) => {
                auxiliaries.remove(held);
                releases.remove(released);
                excused += 1;
            }
            _ => mine.push(*note),
        }
    }
    if excused > 0 {
        verdict.used.insert(named(|rule| {
            matches!(rule, Rule::TrillAuxiliaryReleasedOnThePrincipal)
        }));
    }
    if !auxiliaries.is_empty() || !releases.is_empty() {
        differ(
            verdict,
            Aspect::Unreleased,
            format!(
                "GP leaves {auxiliaries:?} sounding and releases {releases:?} on another key, beyond our {excused} auxiliaries"
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
    let lanes: BTreeSet<Selection> = ours.bends.keys().chain(gp.bends.keys()).copied().collect();
    for channel in lanes {
        let mine = ours.bends.get(&channel).cloned().unwrap_or_default();
        let theirs = gp.bends.get(&channel).cloned().unwrap_or_default();
        if mine.len() != theirs.len() {
            differ(
                verdict,
                Aspect::Bends,
                format!("{channel:?}: {} bends against {}", mine.len(), theirs.len()),
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
                    format!("{channel:?}: {left:?} against {right:?}"),
                ),
            }
        }
    }
}

pub fn compare(context: &Context, ours: &Side, gp: &Side) -> Verdict {
    let mut verdict = Verdict::default();
    if ours.sequence != gp.sequence {
        verdict
            .used
            .insert(named(|rule| matches!(rule, Rule::OrderWithinTick)));
    }
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
        if let Rule::TempoEvent {
            file,
            gp,
            ours,
            defects,
        } = allowance.rule
        {
            assert!(names.iter().any(|name| name == file));
            assert!(
                explains(defects, gp, ours),
                "{}: the named defects turn our event into GP's",
                allowance.name
            );
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
    program.programs[0].value += 1;
    rejects(&effects, &program, Aspect::Programs);
    let mut late_program = effects.ours.clone();
    late_program.programs[0].tick += 1;
    rejects(&effects, &late_program, Aspect::Programs);
    let mut twice = effects.ours.clone();
    let first: TrackValue = twice.programs[0];
    twice.programs.insert(0, first);
    rejects(&effects, &twice, Aspect::Programs);
    let mut range = effects.ours.clone();
    range.ranges[0].tick += 1;
    rejects(&effects, &range, Aspect::Ranges);
    let mut elsewhere = effects.ours.clone();
    let bent = elsewhere.bends.remove(&channel).expect("bends");
    elsewhere.bends.insert(
        Selection {
            track: channel.track + 1,
            ..channel
        },
        bent,
    );
    rejects(&effects, &elsewhere, Aspect::Bends);
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
    let mut shifted = Case {
        context: trill.context.clone(),
        ours: trill.ours.clone(),
        gp: trill.gp.clone(),
    };
    shifted.gp.stray_offs[0].tick += 1;
    rejects(&shifted, &trill.ours, Aspect::Unreleased);
}
