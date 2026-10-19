use std::cell::Cell;

use opentab_model::{
    DIRECTION_JUMPS, DIRECTION_TARGETS, DirectionJump, DirectionTarget, Fraction, JumpDestination,
    MasterBar, Score,
};

use crate::ScoreTime;
use crate::exact::Exact;

pub const MAX_PLAYED_BARS: usize = 1 << 20;
const MAX_STEPS: usize = 1 << 22;
const ENDINGS: u32 = 8;
pub(crate) const SIXTY_FOURTHS: i64 = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayedBar {
    pub source: usize,
    pub start: ScoreTime,
    pub length: Fraction,
    pub pass: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayedPosition {
    pub played_bar: usize,
    pub source_bar: usize,
    pub offset: Fraction,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayOrder {
    pub bars: Vec<PlayedBar>,
    pub truncated: bool,
}

impl PlayOrder {
    pub fn played_of(&self, source: usize) -> Vec<usize> {
        self.bars
            .iter()
            .enumerate()
            .filter(|(_, bar)| bar.source == source)
            .map(|(index, _)| index)
            .collect()
    }

    pub fn sources(&self) -> Vec<usize> {
        self.bars.iter().map(|bar| bar.source).collect()
    }

    pub fn end(&self) -> ScoreTime {
        self.bars
            .last()
            .and_then(|bar| {
                Exact::of(bar.start.whole_notes())
                    .add(Exact::of(bar.length))
                    .and_then(Exact::fraction)
                    .ok()
            })
            .and_then(|end| ScoreTime::new(end).ok())
            .unwrap_or(ScoreTime::ZERO)
    }

    pub fn position_at(&self, t: ScoreTime) -> Option<PlayedPosition> {
        let after = self.bars.partition_point(|bar| bar.start <= t);
        let played_bar = after.checked_sub(1)?;
        let bar = self.bars.get(played_bar)?;
        let offset = Exact::of(t.whole_notes())
            .sub(Exact::of(bar.start.whole_notes()))
            .and_then(Exact::fraction)
            .ok()?;
        (offset < bar.length).then_some(PlayedPosition {
            played_bar,
            source_bar: bar.source,
            offset,
        })
    }
}

pub(crate) fn sixty_fourths(bar: &MasterBar) -> i64 {
    let signature = bar.time_signature;
    i64::from(signature.numerator()) * (SIXTY_FOURTHS / i64::from(signature.denominator()))
}

fn jump_bit(jump: DirectionJump) -> u16 {
    let index = DIRECTION_JUMPS
        .iter()
        .position(|candidate| *candidate == jump)
        .unwrap_or(0);
    1 << index
}

fn target_slot(target: DirectionTarget) -> usize {
    DIRECTION_TARGETS
        .iter()
        .position(|candidate| *candidate == target)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct JumpCopy {
    jump: DirectionJump,
    bar: usize,
    slot: usize,
}

#[derive(Debug, Clone, Copy)]
struct Firing {
    jump: DirectionJump,
    target: usize,
}

struct Plan<'s> {
    bars: &'s [MasterBar],
    jumps: Vec<Vec<DirectionJump>>,
    targets: [Option<usize>; DIRECTION_TARGETS.len()],
    section_start: Vec<usize>,
    section_count: Vec<Option<u8>>,
    exhausted: Cell<bool>,
}

#[derive(Debug, Clone)]
struct State {
    index: usize,
    pass: u32,
    start: usize,
    ending: Option<u8>,
    bounced: bool,
    fired: u16,
    fine: bool,
    coda: bool,
    double_coda: bool,
}

enum Leave {
    Stop,
    Moved,
}

impl<'s> Plan<'s> {
    fn new(bars: &'s [MasterBar]) -> Plan<'s> {
        let mut last_jump: Vec<JumpCopy> = Vec::new();
        let mut targets = [None; DIRECTION_TARGETS.len()];
        for (index, bar) in bars.iter().enumerate() {
            for target in &bar.directions.targets {
                targets[target_slot(*target)] = Some(index);
            }
            for (slot, jump) in bar.directions.jumps.iter().enumerate() {
                last_jump.retain(|kept| kept.jump != *jump);
                last_jump.push(JumpCopy {
                    jump: *jump,
                    bar: index,
                    slot,
                });
            }
        }
        let jumps = bars
            .iter()
            .enumerate()
            .map(|(index, bar)| {
                bar.directions
                    .jumps
                    .iter()
                    .enumerate()
                    .filter(|(slot, jump)| {
                        last_jump.contains(&JumpCopy {
                            jump: **jump,
                            bar: index,
                            slot: *slot,
                        })
                    })
                    .map(|(_, jump)| *jump)
                    .collect()
            })
            .collect();
        let mut section_start = Vec::with_capacity(bars.len());
        let mut open = 0;
        for (index, bar) in bars.iter().enumerate() {
            if bar.repeat_start {
                open = index;
            }
            section_start.push(open);
        }
        let mut section_count = vec![None; bars.len()];
        let mut ahead: Option<u8> = None;
        for (index, bar) in bars.iter().enumerate().rev() {
            let next_opens = bars.get(index + 1).is_none_or(|next| next.repeat_start);
            if next_opens {
                ahead = None;
            }
            ahead = bar.repeat_end.or(ahead);
            section_count[index] = ahead;
        }
        Plan {
            bars,
            jumps,
            targets,
            section_start,
            section_count,
            exhausted: Cell::new(false),
        }
    }

    fn destination(&self, jump: DirectionJump) -> Option<usize> {
        match jump.destination() {
            JumpDestination::Start => (!self.bars.is_empty()).then_some(0),
            JumpDestination::Target(target) => self.targets[target_slot(target)],
        }
    }

    fn enter(&self, state: &mut State) -> Option<bool> {
        let bar = self.bars.get(state.index)?;
        if bar.repeat_start && !state.bounced {
            state.start = state.index;
            state.pass = 1;
        }
        state.bounced = false;
        if bar.alternate_endings != 0 {
            state.ending = Some(bar.alternate_endings);
        } else if bar.repeat_start {
            state.ending = None;
        }
        Some(match state.ending {
            Some(mask) => state.pass <= ENDINGS && mask & (1u8 << (state.pass - 1)) != 0,
            None => true,
        })
    }

    fn advance(&self, state: &mut State) -> bool {
        for _ in 0..MAX_STEPS {
            match self.enter(state) {
                None => return false,
                Some(true) => return true,
                Some(false) => state.index += 1,
            }
        }
        self.exhausted.set(true);
        false
    }

    fn last_pass(&self, state: &State) -> bool {
        match self.section_count[state.index] {
            Some(count) => state.pass >= u32::from(count),
            None => true,
        }
    }

    fn return_fires(&self, state: &State, jump: DirectionJump) -> Option<usize> {
        if state.fired & jump_bit(jump) != 0 || !self.last_pass(state) {
            return None;
        }
        self.destination(jump)
    }

    fn released(state: &State, jump: DirectionJump) -> bool {
        match jump {
            DirectionJump::DaCoda => state.coda,
            _ => state.double_coda,
        }
    }

    fn valid_forward(&self, at: usize, jump: DirectionJump) -> Option<usize> {
        self.destination(jump).filter(|target| *target > at)
    }

    fn forward_fires(&self, state: &State, jump: DirectionJump) -> Option<usize> {
        if state.fired & jump_bit(jump) != 0 {
            return None;
        }
        let target = self.valid_forward(state.index, jump)?;
        if Plan::released(state, jump) || !self.held(state, jump) {
            return Some(target);
        }
        None
    }

    fn holds(&self, probe: &State, origin: usize, jump: DirectionJump) -> bool {
        self.jumps[probe.index].iter().any(|other| {
            if probe.fired & jump_bit(*other) != 0 {
                return false;
            }
            if other.is_return() {
                return self
                    .destination(*other)
                    .is_some_and(|target| target <= origin);
            }
            jump == DirectionJump::DaCoda
                && *other == DirectionJump::DaDoubleCoda
                && self.valid_forward(probe.index, *other).is_some()
        })
    }

    fn held(&self, state: &State, jump: DirectionJump) -> bool {
        let origin = state.index;
        let mut probe = state.clone();
        for _ in 0..MAX_STEPS {
            if self.holds(&probe, origin, jump) {
                return true;
            }
            let fired = self.jumps[probe.index]
                .iter()
                .filter(|other| other.is_return())
                .find_map(|other| {
                    self.return_fires(&probe, *other).map(|target| Firing {
                        jump: *other,
                        target,
                    })
                });
            match fired {
                Some(firing) => self.fire(&mut probe, firing.jump, firing.target),
                None => self.close(&mut probe),
            }
            if !self.advance(&mut probe) {
                return false;
            }
        }
        self.exhausted.set(true);
        false
    }

    fn fire(&self, state: &mut State, jump: DirectionJump, target: usize) {
        state.fired |= jump_bit(jump);
        match jump.releases() {
            Some(DirectionTarget::Fine) => state.fine = true,
            Some(DirectionTarget::Coda) => state.coda = true,
            Some(DirectionTarget::DoubleCoda) => state.double_coda = true,
            _ => {}
        }
        state.index = target;
        state.pass = 1;
        state.ending = None;
        state.bounced = false;
        state.start = self.section_start[target];
    }

    fn close(&self, state: &mut State) {
        let bar = &self.bars[state.index];
        if let Some(count) = bar.repeat_end
            && state.pass < u32::from(count)
        {
            state.pass += 1;
            state.index = state.start;
            state.ending = None;
            state.bounced = true;
            return;
        }
        state.index += 1;
    }

    fn leave(&self, state: &mut State) -> Leave {
        let index = state.index;
        if state.fine && self.targets[target_slot(DirectionTarget::Fine)] == Some(index) {
            return Leave::Stop;
        }
        for jump in &self.jumps[index] {
            let target = if jump.is_return() {
                self.return_fires(state, *jump)
            } else {
                self.forward_fires(state, *jump)
            };
            if let Some(target) = target {
                self.fire(state, *jump, target);
                return Leave::Moved;
            }
        }
        self.close(state);
        Leave::Moved
    }
}

pub fn play_order(score: &Score) -> PlayOrder {
    let plan = Plan::new(&score.master_bars);
    let mut state = State {
        index: 0,
        pass: 1,
        start: 0,
        ending: None,
        bounced: false,
        fired: 0,
        fine: false,
        coda: false,
        double_coda: false,
    };
    let mut bars = Vec::new();
    let mut start: i64 = 0;
    while bars.len() < MAX_PLAYED_BARS && plan.advance(&mut state) {
        let bar = &plan.bars[state.index];
        let length = sixty_fourths(bar);
        bars.push(PlayedBar {
            source: state.index,
            start: ScoreTime::new(Fraction::new(start, SIXTY_FOURTHS)).unwrap_or(ScoreTime::ZERO),
            length: bar.time_signature.bar_length(),
            pass: u8::try_from(state.pass).unwrap_or(u8::MAX),
        });
        start = start.saturating_add(length);
        if let Leave::Stop = plan.leave(&mut state) {
            break;
        }
    }
    let truncated = bars.len() >= MAX_PLAYED_BARS || plan.exhausted.get();
    PlayOrder { bars, truncated }
}
