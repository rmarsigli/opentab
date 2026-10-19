use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectionTarget {
    Segno,
    SegnoSegno,
    Coda,
    DoubleCoda,
    Fine,
}

pub const DIRECTION_TARGETS: [DirectionTarget; 5] = [
    DirectionTarget::Segno,
    DirectionTarget::SegnoSegno,
    DirectionTarget::Coda,
    DirectionTarget::DoubleCoda,
    DirectionTarget::Fine,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectionJump {
    DaCapo,
    DaCapoAlFine,
    DaCapoAlCoda,
    DaCapoAlDoubleCoda,
    DaSegno,
    DaSegnoAlFine,
    DaSegnoAlCoda,
    DaSegnoAlDoubleCoda,
    DaSegnoSegno,
    DaSegnoSegnoAlFine,
    DaSegnoSegnoAlCoda,
    DaSegnoSegnoAlDoubleCoda,
    DaCoda,
    DaDoubleCoda,
}

pub const DIRECTION_JUMPS: [DirectionJump; 14] = [
    DirectionJump::DaCapo,
    DirectionJump::DaCapoAlFine,
    DirectionJump::DaCapoAlCoda,
    DirectionJump::DaCapoAlDoubleCoda,
    DirectionJump::DaSegno,
    DirectionJump::DaSegnoAlFine,
    DirectionJump::DaSegnoAlCoda,
    DirectionJump::DaSegnoAlDoubleCoda,
    DirectionJump::DaSegnoSegno,
    DirectionJump::DaSegnoSegnoAlFine,
    DirectionJump::DaSegnoSegnoAlCoda,
    DirectionJump::DaSegnoSegnoAlDoubleCoda,
    DirectionJump::DaCoda,
    DirectionJump::DaDoubleCoda,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JumpDestination {
    Start,
    Target(DirectionTarget),
}

impl DirectionJump {
    pub fn destination(self) -> JumpDestination {
        match self {
            DirectionJump::DaCapo
            | DirectionJump::DaCapoAlFine
            | DirectionJump::DaCapoAlCoda
            | DirectionJump::DaCapoAlDoubleCoda => JumpDestination::Start,
            DirectionJump::DaSegno
            | DirectionJump::DaSegnoAlFine
            | DirectionJump::DaSegnoAlCoda
            | DirectionJump::DaSegnoAlDoubleCoda => JumpDestination::Target(DirectionTarget::Segno),
            DirectionJump::DaSegnoSegno
            | DirectionJump::DaSegnoSegnoAlFine
            | DirectionJump::DaSegnoSegnoAlCoda
            | DirectionJump::DaSegnoSegnoAlDoubleCoda => {
                JumpDestination::Target(DirectionTarget::SegnoSegno)
            }
            DirectionJump::DaCoda => JumpDestination::Target(DirectionTarget::Coda),
            DirectionJump::DaDoubleCoda => JumpDestination::Target(DirectionTarget::DoubleCoda),
        }
    }

    pub fn releases(self) -> Option<DirectionTarget> {
        match self {
            DirectionJump::DaCapoAlFine
            | DirectionJump::DaSegnoAlFine
            | DirectionJump::DaSegnoSegnoAlFine => Some(DirectionTarget::Fine),
            DirectionJump::DaCapoAlCoda
            | DirectionJump::DaSegnoAlCoda
            | DirectionJump::DaSegnoSegnoAlCoda => Some(DirectionTarget::Coda),
            DirectionJump::DaCapoAlDoubleCoda
            | DirectionJump::DaSegnoAlDoubleCoda
            | DirectionJump::DaSegnoSegnoAlDoubleCoda => Some(DirectionTarget::DoubleCoda),
            DirectionJump::DaCapo
            | DirectionJump::DaSegno
            | DirectionJump::DaSegnoSegno
            | DirectionJump::DaCoda
            | DirectionJump::DaDoubleCoda => None,
        }
    }

    pub fn is_return(self) -> bool {
        !matches!(self, DirectionJump::DaCoda | DirectionJump::DaDoubleCoda)
    }

    pub fn needs(self) -> Vec<DirectionTarget> {
        let destination = match self.destination() {
            JumpDestination::Start => None,
            JumpDestination::Target(target) => Some(target),
        };
        destination.into_iter().chain(self.releases()).collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Directions {
    pub targets: Vec<DirectionTarget>,
    pub jumps: Vec<DirectionJump>,
}

impl Directions {
    pub fn is_empty(&self) -> bool {
        self.targets.is_empty() && self.jumps.is_empty()
    }

    pub fn holds(&self, target: DirectionTarget) -> bool {
        self.targets.contains(&target)
    }
}
