use crate::units::{STAFF_SPACE, Units};

pub const LANE_GAP: Units = Units(STAFF_SPACE.0 / 4);
pub const LANE_HEIGHT: Units = STAFF_SPACE;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Lane {
    Marks,
    Vibrato,
    LetRing,
    PalmMute,
}

pub const LANES: [Lane; 4] = [Lane::Marks, Lane::Vibrato, Lane::LetRing, Lane::PalmMute];

impl Lane {
    pub fn bit(self) -> u8 {
        match self {
            Lane::Marks => 1,
            Lane::Vibrato => 2,
            Lane::LetRing => 4,
            Lane::PalmMute => 8,
        }
    }

    pub fn height(self) -> Units {
        LANE_HEIGHT
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LaneSet(u8);

impl LaneSet {
    pub const EMPTY: LaneSet = LaneSet(0);

    pub fn with(self, lane: Lane) -> LaneSet {
        LaneSet(self.0 | lane.bit())
    }

    pub fn contains(self, lane: Lane) -> bool {
        self.0 & lane.bit() != 0
    }

    pub fn union(self, other: LaneSet) -> LaneSet {
        LaneSet(self.0 | other.0)
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Band {
    pub near: Units,
    pub lanes: LaneSet,
}

impl Band {
    pub fn merge(self, other: Band) -> Band {
        Band {
            near: self.near.max(other.near),
            lanes: self.lanes.union(other.lanes),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LanePlacement {
    pub lane: Lane,
    pub top: Units,
    pub bottom: Units,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct BandLayout {
    pub near: Units,
    pub lanes: Vec<LanePlacement>,
    pub reach: Units,
}

impl BandLayout {
    pub fn of(band: Band) -> BandLayout {
        let mut cursor = band.near.max(Units::ZERO);
        let mut lanes = Vec::new();
        for lane in LANES {
            if !band.lanes.contains(lane) {
                continue;
            }
            let bottom = -(cursor + LANE_GAP);
            let top = bottom - lane.height();
            lanes.push(LanePlacement { lane, top, bottom });
            cursor = -top;
        }
        BandLayout {
            near: band.near.max(Units::ZERO),
            lanes,
            reach: cursor,
        }
    }

    pub fn lane(&self, lane: Lane) -> Option<LanePlacement> {
        self.lanes
            .iter()
            .copied()
            .find(|placed| placed.lane == lane)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lanes_stack_upwards_above_the_near_reach_in_a_fixed_order() {
        let band = Band {
            near: Units(2000),
            lanes: LaneSet::EMPTY.with(Lane::PalmMute).with(Lane::Marks),
        };
        let layout = BandLayout::of(band);
        assert_eq!(
            layout.lanes,
            vec![
                LanePlacement {
                    lane: Lane::Marks,
                    top: Units(-2000 - 256 - 1024),
                    bottom: Units(-2256),
                },
                LanePlacement {
                    lane: Lane::PalmMute,
                    top: Units(-3280 - 256 - 1024),
                    bottom: Units(-3536),
                },
            ]
        );
        assert_eq!(layout.reach, Units(4560));
        assert_eq!(layout.lane(Lane::Vibrato), None);
        let empty = BandLayout::of(Band::default());
        assert_eq!(empty.reach, Units::ZERO);
        assert!(empty.lanes.is_empty());
    }

    #[test]
    fn bands_merge_by_maximum_reach_and_lane_union() {
        let first = Band {
            near: Units(100),
            lanes: LaneSet::EMPTY.with(Lane::LetRing),
        };
        let second = Band {
            near: Units(50),
            lanes: LaneSet::EMPTY.with(Lane::Vibrato),
        };
        let merged = first.merge(second);
        assert_eq!(merged.near, Units(100));
        assert!(merged.lanes.contains(Lane::LetRing));
        assert!(merged.lanes.contains(Lane::Vibrato));
        assert!(!merged.lanes.contains(Lane::Marks));
        assert!(!merged.lanes.is_empty());
    }
}
