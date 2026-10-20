use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use opentab_gp::read_gp;
use opentab_model::{DirectionJump, DirectionTarget, Score, TRIPLET_FEELS, TripletFeel};
use opentab_playback::{
    MidiMessage, Performance, PlaybackOptions, Profile, TrackLayout, export_midi, perform, to_ticks,
};

use crate::oracle_allowances::{
    ALLOWED_DIFFERENCES, Rule, TempoAt, bend_ticks, explains, named, off_grid_ticks,
};
use crate::oracle_side::{CONDUCTOR_TRACK, Note, Onset, Selection, Side, Slot, TrackValue, side};
use crate::smf::SmfText;

const GOLDEN_FIXTURES: usize = 35;
const SWING_FIXTURE: &str = "triplet-feel.gp";
const UNISON_FIXTURE: &str = "unisons.gp";
const UNEQUAL_UNISONS: usize = 8;
const TITLE_KIND: u8 = 0x03;
const COPYRIGHT_KIND: u8 = 0x02;
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
    Texts,
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

pub fn restruck(notes: &[Note]) -> (Vec<Note>, bool) {
    let mut order: Vec<usize> = (0..notes.len()).collect();
    order.sort_by_key(|index| {
        let onset = notes[*index].onset;
        (onset.channel, onset.key, onset.tick, onset.track, *index)
    });
    let mut kept: Vec<Note> = notes.to_vec();
    let mut dropped = vec![false; notes.len()];
    let mut changed = false;
    let mut previous: Option<usize> = None;
    for index in order {
        let current = kept[index].onset;
        let Some(before) = previous.filter(|before| {
            let earlier = kept[*before].onset;
            earlier.channel == current.channel && earlier.key == current.key
        }) else {
            previous = Some(index);
            continue;
        };
        let earlier = kept[before];
        let end = earlier.onset.tick + earlier.length;
        if end <= current.tick {
            previous = Some(index);
        } else if earlier.onset.tick == current.tick {
            kept[before].length = earlier.length.max(kept[index].length);
            kept[before].onset.velocity = earlier.onset.velocity.max(current.velocity);
            dropped[index] = true;
            changed = true;
        } else {
            kept[before].length = current.tick - earlier.onset.tick;
            changed = true;
            previous = Some(index);
        }
    }
    let notes = kept
        .into_iter()
        .zip(dropped)
        .filter_map(|(note, dropped)| (!dropped).then_some(note))
        .collect();
    (notes, changed)
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
    let (mut theirs, cut) = restruck(&gp.notes);
    if cut {
        verdict
            .used
            .insert(named(|rule| matches!(rule, Rule::RestrikeCut)));
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
    if ours.texts != gp.texts {
        differ(
            &mut verdict,
            Aspect::Texts,
            format!("{:?} against {:?}", ours.texts, gp.texts),
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
    let golden =
        std::fs::read(root().join("goldens/gp").join(name).join("export.mid")).expect("golden");
    case_of(name, &bytes, &golden)
}

pub fn case_of(name: &str, bytes: &[u8], golden: &[u8]) -> Case {
    let score = read_gp(bytes).expect("fixture reads");
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
        gp: side(golden),
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
        let declared_only = matches!(allowance.rule, Rule::CodaNeverReached);
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

#[test]
fn profile_gp_swings_every_feel_and_rhythm_as_guitar_pro_exports_it() {
    let case = case(SWING_FIXTURE);
    let verdict = compare(&case.context, &case.ours, &case.gp);
    assert!(verdict.differences.is_empty(), "{:#?}", verdict.differences);
    let bytes = std::fs::read(root().join("fixtures/gp").join(SWING_FIXTURE)).expect("fixture");
    let score = read_gp(&bytes).expect("fixture reads");
    let feels: BTreeSet<TripletFeel> = score
        .master_bars
        .iter()
        .map(|bar| bar.triplet_feel)
        .collect();
    assert_eq!(feels.len(), TRIPLET_FEELS.len());
    let mut straight = score.clone();
    for bar in &mut straight.master_bars {
        bar.triplet_feel = TripletFeel::None;
    }
    let performance = perform(
        &straight,
        &PlaybackOptions {
            profile: Profile::Gp,
        },
    )
    .expect("perform");
    let unswung = export_midi(&straight, Profile::Gp).expect("export");
    let verdict = compare(
        &context(SWING_FIXTURE, &straight, &performance),
        &side(&unswung),
        &case.gp,
    );
    assert!(
        verdict
            .differences
            .iter()
            .any(|difference| difference.aspect == Aspect::Onsets),
        "{:#?}",
        verdict.differences
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Strike {
    channel: u8,
    key: u8,
    tick: u64,
}

fn strike(note: &Note) -> Strike {
    Strike {
        channel: note.onset.channel,
        key: note.onset.key,
        tick: note.onset.tick,
    }
}

fn strikes(notes: &[Note]) -> BTreeMap<Strike, Vec<Note>> {
    let mut out: BTreeMap<Strike, Vec<Note>> = BTreeMap::new();
    for note in notes {
        out.entry(strike(note)).or_default().push(*note);
    }
    out
}

#[test]
fn guitar_pro_writes_both_notes_of_an_unequal_unison_and_we_keep_the_louder_and_longer() {
    let case = case(UNISON_FIXTURE);
    let ours = strikes(&case.ours.notes);
    let gp = strikes(&case.gp.notes);
    let unequal: Vec<(&Strike, &Vec<Note>)> = gp
        .iter()
        .filter(|(_, notes)| notes.len() == 2 && notes[0].onset.velocity != notes[1].onset.velocity)
        .collect();
    assert_eq!(unequal.len(), UNEQUAL_UNISONS, "{gp:#?}");
    let mut louder_first = 0;
    let mut louder_second = 0;
    for (at, pair) in &unequal {
        if pair[0].onset.velocity > pair[1].onset.velocity {
            louder_first += 1;
        } else {
            louder_second += 1;
        }
        let kept = ours.get(at).expect("our merged note");
        assert_eq!(kept.len(), 1, "{at:?}");
        let loudest = pair.iter().map(|note| note.onset.velocity).max();
        let longest = pair.iter().map(|note| note.length).max();
        assert_eq!(Some(kept[0].onset.velocity), loudest, "{at:?}");
        assert_eq!(Some(kept[0].length), longest, "{at:?}");
    }
    assert!(louder_first > 0 && louder_second > 0);
    assert!(
        unequal
            .iter()
            .any(|(_, pair)| pair[0].length != pair[1].length)
    );
    assert!(
        unequal
            .iter()
            .any(|(_, pair)| pair[0].onset.track != pair[1].onset.track)
    );
    let staggered: Vec<(Note, Note)> = case
        .gp
        .notes
        .iter()
        .flat_map(|early| {
            case.gp.notes.iter().filter_map(move |late| {
                let same =
                    early.onset.channel == late.onset.channel && early.onset.key == late.onset.key;
                let inside = early.onset.tick < late.onset.tick
                    && late.onset.tick < early.onset.tick + early.length;
                (same && inside).then_some((*early, *late))
            })
        })
        .collect();
    assert_eq!(staggered.len(), 1, "{staggered:?}");
    let (early, late) = staggered[0];
    let cut = ours.get(&strike(&early)).expect("our earlier note");
    assert_eq!(cut[0].length, late.onset.tick - early.onset.tick);
    let verdict = compare(&case.context, &case.ours, &case.gp);
    assert!(verdict.differences.is_empty(), "{:#?}", verdict.differences);
    assert!(verdict.used.contains("restrike-cut"));
}

fn conductor_text(side: &Side, kind: u8) -> Option<&SmfText> {
    side.texts
        .iter()
        .find(|text| text.track == CONDUCTOR_TRACK && text.kind == kind)
}

#[test]
fn the_conductor_track_carries_the_title_and_copyright_as_guitar_pro_writes_them() {
    let metadata = case("metadata.gp");
    accepts(&metadata, &metadata.ours);
    let title = conductor_text(&metadata.gp, TITLE_KIND).expect("GP's title");
    assert_eq!((title.tick, title.text.is_empty()), (0, false));
    assert!(conductor_text(&metadata.gp, COPYRIGHT_KIND).is_some());
    let mut untitled = metadata.ours.clone();
    untitled
        .texts
        .retain(|text| !(text.track == CONDUCTOR_TRACK && text.kind == TITLE_KIND));
    rejects(&metadata, &untitled, Aspect::Texts);
    let mut uncredited = metadata.ours.clone();
    uncredited
        .texts
        .retain(|text| !(text.track == CONDUCTOR_TRACK && text.kind == COPYRIGHT_KIND));
    rejects(&metadata, &uncredited, Aspect::Texts);
    let mut renamed = metadata.ours.clone();
    renamed.texts[0].text.push(b'!');
    rejects(&metadata, &renamed, Aspect::Texts);
    let blank = case("from-scratch.gp");
    accepts(&blank, &blank.ours);
    assert!(conductor_text(&blank.gp, TITLE_KIND).is_none());
    let mut titled = blank.ours.clone();
    titled.texts.push(SmfText {
        track: CONDUCTOR_TRACK,
        tick: 0,
        kind: TITLE_KIND,
        text: Vec::new(),
    });
    titled.texts.sort();
    rejects(&blank, &titled, Aspect::Texts);
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
