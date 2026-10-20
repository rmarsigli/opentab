use std::collections::BTreeSet;

use opentab_model::{DirectionJump, DirectionTarget, MasterBar};

const PASS_LIMIT: u32 = 8;

#[derive(Debug, Clone, Copy)]
enum Goal {
    Top,
    Mark(DirectionTarget),
}

fn goal_of(jump: DirectionJump) -> Goal {
    use DirectionJump as J;
    use DirectionTarget as T;
    match jump {
        J::DaCapo | J::DaCapoAlFine | J::DaCapoAlCoda | J::DaCapoAlDoubleCoda => Goal::Top,
        J::DaSegno | J::DaSegnoAlFine | J::DaSegnoAlCoda | J::DaSegnoAlDoubleCoda => {
            Goal::Mark(T::Segno)
        }
        J::DaSegnoSegno
        | J::DaSegnoSegnoAlFine
        | J::DaSegnoSegnoAlCoda
        | J::DaSegnoSegnoAlDoubleCoda => Goal::Mark(T::SegnoSegno),
        J::DaCoda => Goal::Mark(T::Coda),
        J::DaDoubleCoda => Goal::Mark(T::DoubleCoda),
    }
}

fn armed_by(jump: DirectionJump) -> Option<DirectionTarget> {
    use DirectionJump as J;
    use DirectionTarget as T;
    match jump {
        J::DaCapoAlFine | J::DaSegnoAlFine | J::DaSegnoSegnoAlFine => Some(T::Fine),
        J::DaCapoAlCoda | J::DaSegnoAlCoda | J::DaSegnoSegnoAlCoda => Some(T::Coda),
        J::DaCapoAlDoubleCoda | J::DaSegnoAlDoubleCoda | J::DaSegnoSegnoAlDoubleCoda => {
            Some(T::DoubleCoda)
        }
        J::DaCapo | J::DaSegno | J::DaSegnoSegno | J::DaCoda | J::DaDoubleCoda => None,
    }
}
const WALK_LIMIT: usize = 1 << 22;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Visit {
    pub bar: usize,
    pub pass: u32,
}

#[derive(Debug, Clone)]
struct Cursor {
    bar: usize,
    pass: u32,
    section: usize,
    group: Option<u8>,
    after_bounce: bool,
}

#[derive(Debug, Clone, Default)]
struct Memory {
    fired: BTreeSet<DirectionJump>,
    released: BTreeSet<DirectionTarget>,
}

enum Exit {
    End,
    Continue,
}

struct Score<'b> {
    bars: &'b [MasterBar],
}

impl Score<'_> {
    fn kept(&self, bar: usize, slot: usize) -> bool {
        let name = self.bars[bar].directions.jumps[slot];
        let later_here = self.bars[bar].directions.jumps[slot + 1..].contains(&name);
        let later_bar = self.bars[bar + 1..]
            .iter()
            .any(|later| later.directions.jumps.contains(&name));
        !later_here && !later_bar
    }

    fn jumps_of(&self, bar: usize) -> Vec<DirectionJump> {
        (0..self.bars[bar].directions.jumps.len())
            .filter(|slot| self.kept(bar, *slot))
            .map(|slot| self.bars[bar].directions.jumps[slot])
            .collect()
    }

    fn holder(&self, target: DirectionTarget) -> Option<usize> {
        self.bars
            .iter()
            .rposition(|bar| bar.directions.targets.contains(&target))
    }

    fn goal(&self, jump: DirectionJump) -> Option<usize> {
        match goal_of(jump) {
            Goal::Top => Some(0),
            Goal::Mark(target) => self.holder(target),
        }
    }

    fn arrive(&self, cursor: &mut Cursor) -> bool {
        let bar = &self.bars[cursor.bar];
        if bar.repeat_start {
            if !cursor.after_bounce {
                cursor.pass = 1;
                cursor.section = cursor.bar;
            }
            if bar.alternate_endings == 0 {
                cursor.group = None;
            }
        }
        cursor.after_bounce = false;
        if bar.alternate_endings != 0 {
            cursor.group = Some(bar.alternate_endings);
        }
        let Some(group) = cursor.group else {
            return true;
        };
        cursor.pass <= PASS_LIMIT && (group >> (cursor.pass - 1)) & 1 == 1
    }

    fn next_visit(&self, cursor: &mut Cursor) -> bool {
        while cursor.bar < self.bars.len() {
            if self.arrive(cursor) {
                return true;
            }
            cursor.bar += 1;
        }
        false
    }

    fn section_repeats(&self, cursor: &Cursor) -> bool {
        let mut bar = cursor.bar;
        while bar < self.bars.len() {
            if bar > cursor.bar && self.bars[bar].repeat_start {
                return false;
            }
            if let Some(count) = self.bars[bar].repeat_end {
                return cursor.pass < u32::from(count);
            }
            bar += 1;
        }
        false
    }

    fn opening_section(&self, target: usize) -> usize {
        (0..=target)
            .rev()
            .find(|bar| self.bars[*bar].repeat_start)
            .unwrap_or(0)
    }

    fn take(&self, cursor: &mut Cursor, memory: &mut Memory, jump: DirectionJump, goal: usize) {
        memory.fired.insert(jump);
        if let Some(released) = armed_by(jump) {
            memory.released.insert(released);
        }
        *cursor = Cursor {
            bar: goal,
            pass: 1,
            section: self.opening_section(goal),
            group: None,
            after_bounce: false,
        };
    }

    fn return_goal(&self, cursor: &Cursor, memory: &Memory, jump: DirectionJump) -> Option<usize> {
        if memory.fired.contains(&jump) || self.section_repeats(cursor) {
            None
        } else {
            self.goal(jump)
        }
    }

    fn coda_goal(&self, from: usize, jump: DirectionJump) -> Option<usize> {
        self.goal(jump).filter(|goal| *goal > from)
    }

    fn armed_goal(&self, from: usize, memory: &Memory, jump: DirectionJump) -> Option<usize> {
        let armed = match jump {
            DirectionJump::DaCoda => DirectionTarget::Coda,
            _ => DirectionTarget::DoubleCoda,
        };
        if memory.fired.contains(&jump) || !memory.released.contains(&armed) {
            None
        } else {
            self.coda_goal(from, jump)
        }
    }

    fn bounce_or_step(&self, cursor: &mut Cursor) {
        let bar = &self.bars[cursor.bar];
        match bar.repeat_end {
            Some(count) if cursor.pass < u32::from(count) => {
                cursor.pass += 1;
                cursor.bar = cursor.section;
                cursor.group = None;
                cursor.after_bounce = true;
            }
            _ => cursor.bar += 1,
        }
    }

    fn blocks(&self, at: usize, memory: &Memory, origin: usize, jump: DirectionJump) -> bool {
        for other in self.jumps_of(at) {
            if memory.fired.contains(&other) {
                continue;
            }
            let returning = !matches!(other, DirectionJump::DaCoda | DirectionJump::DaDoubleCoda);
            if returning && self.goal(other).is_some_and(|goal| goal <= origin) {
                return true;
            }
            if jump == DirectionJump::DaCoda
                && other == DirectionJump::DaDoubleCoda
                && self.coda_goal(at, other).is_some()
            {
                return true;
            }
        }
        false
    }

    fn held_back(&self, cursor: &Cursor, memory: &Memory, jump: DirectionJump) -> bool {
        let origin = cursor.bar;
        let mut probe = cursor.clone();
        let mut seen = memory.clone();
        for _ in 0..WALK_LIMIT {
            if self.blocks(probe.bar, &seen, origin, jump) {
                return true;
            }
            let mut moved = false;
            for other in self.jumps_of(probe.bar) {
                let goal = match other {
                    DirectionJump::DaCoda | DirectionJump::DaDoubleCoda => {
                        self.armed_goal(probe.bar, &seen, other)
                    }
                    _ => self.return_goal(&probe, &seen, other),
                };
                if let Some(goal) = goal {
                    self.take(&mut probe, &mut seen, other, goal);
                    moved = true;
                    break;
                }
            }
            if !moved {
                self.bounce_or_step(&mut probe);
            }
            if !self.next_visit(&mut probe) {
                return false;
            }
        }
        false
    }

    fn depart(&self, cursor: &mut Cursor, memory: &mut Memory) -> Exit {
        let here = cursor.bar;
        if memory.released.contains(&DirectionTarget::Fine)
            && self.holder(DirectionTarget::Fine) == Some(here)
        {
            return Exit::End;
        }
        for jump in self.jumps_of(here) {
            let goal = match jump {
                DirectionJump::DaCoda | DirectionJump::DaDoubleCoda => {
                    let armed = match jump {
                        DirectionJump::DaCoda => DirectionTarget::Coda,
                        _ => DirectionTarget::DoubleCoda,
                    };
                    if memory.fired.contains(&jump) {
                        None
                    } else {
                        self.coda_goal(here, jump).filter(|_| {
                            memory.released.contains(&armed)
                                || !self.held_back(cursor, memory, jump)
                        })
                    }
                }
                _ => self.return_goal(cursor, memory, jump),
            };
            if let Some(goal) = goal {
                self.take(cursor, memory, jump, goal);
                return Exit::Continue;
            }
        }
        self.bounce_or_step(cursor);
        Exit::Continue
    }
}

pub fn reference_order(bars: &[MasterBar], limit: usize) -> Vec<Visit> {
    let score = Score { bars };
    let mut cursor = Cursor {
        bar: 0,
        pass: 1,
        section: 0,
        group: None,
        after_bounce: false,
    };
    let mut memory = Memory::default();
    let mut visits = Vec::new();
    while visits.len() < limit && score.next_visit(&mut cursor) {
        visits.push(Visit {
            bar: cursor.bar,
            pass: cursor.pass,
        });
        if let Exit::End = score.depart(&mut cursor, &mut memory) {
            break;
        }
    }
    visits
}
