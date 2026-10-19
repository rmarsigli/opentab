use opentab_model::{DrumKit, Note, NoteValue};

use crate::glyphs::{Glyph, NOTEHEAD_X_BLACK, NOTEHEAD_X_HALF, NOTEHEAD_X_WHOLE};
use crate::notation::head_glyph;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DrumClass {
    Kick,
    Snare,
    SideStick,
    HighTom,
    MidTom,
    LowTom,
    ClosedHiHat,
    OpenHiHat,
    PedalHiHat,
    Crash,
    Ride,
    RideBell,
    China,
    Splash,
    Cowbell,
    Tambourine,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DrumHead {
    Normal,
    Cross,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrumEntry {
    pub class: DrumClass,
    pub keys: &'static [u8],
    pub position: i64,
    pub head: DrumHead,
}

impl DrumEntry {
    pub fn fully_drawn(&self) -> bool {
        !matches!(self.class, DrumClass::Other | DrumClass::OpenHiHat)
    }
}

const fn entry(class: DrumClass, keys: &'static [u8], position: i64, head: DrumHead) -> DrumEntry {
    DrumEntry {
        class,
        keys,
        position,
        head,
    }
}

pub const DRUM_TABLE: [DrumEntry; 16] = [
    entry(DrumClass::Kick, &[35, 36], 7, DrumHead::Normal),
    entry(DrumClass::Snare, &[38, 40], 3, DrumHead::Normal),
    entry(DrumClass::SideStick, &[37], 3, DrumHead::Cross),
    entry(DrumClass::HighTom, &[48, 50], 1, DrumHead::Normal),
    entry(DrumClass::MidTom, &[45, 47], 2, DrumHead::Normal),
    entry(DrumClass::LowTom, &[41, 43], 5, DrumHead::Normal),
    entry(DrumClass::ClosedHiHat, &[42], -1, DrumHead::Cross),
    entry(DrumClass::OpenHiHat, &[46], -1, DrumHead::Cross),
    entry(DrumClass::PedalHiHat, &[44], 9, DrumHead::Cross),
    entry(DrumClass::Crash, &[49, 57], -2, DrumHead::Cross),
    entry(DrumClass::Ride, &[51, 59], 0, DrumHead::Cross),
    entry(DrumClass::RideBell, &[53], 0, DrumHead::Cross),
    entry(DrumClass::China, &[52], -4, DrumHead::Cross),
    entry(DrumClass::Splash, &[55], -3, DrumHead::Cross),
    entry(DrumClass::Cowbell, &[56], 1, DrumHead::Cross),
    entry(DrumClass::Tambourine, &[54], 4, DrumHead::Cross),
];

pub const DEFAULT_DRUM: DrumEntry = entry(DrumClass::Other, &[], 4, DrumHead::Normal);

pub fn drum_entry(key: Option<u8>) -> DrumEntry {
    key.and_then(|key| {
        DRUM_TABLE
            .iter()
            .find(|candidate| candidate.keys.contains(&key))
            .copied()
    })
    .unwrap_or(DEFAULT_DRUM)
}

pub fn drum_glyph(head: DrumHead, value: NoteValue, note: &Note) -> Glyph {
    match (head, value) {
        (DrumHead::Normal, _) => head_glyph(value, note),
        (DrumHead::Cross, NoteValue::Whole) => NOTEHEAD_X_WHOLE,
        (DrumHead::Cross, NoteValue::Half) => NOTEHEAD_X_HALF,
        (DrumHead::Cross, _) => NOTEHEAD_X_BLACK,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DrumStaffKit {
    pub keys: Vec<u8>,
}

impl DrumStaffKit {
    pub fn of(kit: &DrumKit) -> DrumStaffKit {
        let keys = kit
            .articulations()
            .iter()
            .map(|articulation| articulation.output_key)
            .collect();
        DrumStaffKit { keys }
    }

    pub fn key(&self, articulation: u16) -> Option<u8> {
        self.keys.get(usize::from(articulation)).copied()
    }

    pub fn entry(&self, articulation: u16) -> DrumEntry {
        drum_entry(self.key(articulation))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_general_midi_key_belongs_to_at_most_one_class() {
        for key in 0..=127_u8 {
            let owners = DRUM_TABLE
                .iter()
                .filter(|candidate| candidate.keys.contains(&key))
                .count();
            assert!(owners <= 1, "{key}");
        }
        let classes: Vec<DrumClass> = DRUM_TABLE.iter().map(|found| found.class).collect();
        let mut unique = classes.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), classes.len());
    }

    #[test]
    fn the_general_midi_drum_classes_sit_on_their_lines() {
        let expect = [
            (35, DrumClass::Kick, 7, DrumHead::Normal),
            (36, DrumClass::Kick, 7, DrumHead::Normal),
            (37, DrumClass::SideStick, 3, DrumHead::Cross),
            (38, DrumClass::Snare, 3, DrumHead::Normal),
            (40, DrumClass::Snare, 3, DrumHead::Normal),
            (41, DrumClass::LowTom, 5, DrumHead::Normal),
            (42, DrumClass::ClosedHiHat, -1, DrumHead::Cross),
            (43, DrumClass::LowTom, 5, DrumHead::Normal),
            (44, DrumClass::PedalHiHat, 9, DrumHead::Cross),
            (45, DrumClass::MidTom, 2, DrumHead::Normal),
            (46, DrumClass::OpenHiHat, -1, DrumHead::Cross),
            (47, DrumClass::MidTom, 2, DrumHead::Normal),
            (48, DrumClass::HighTom, 1, DrumHead::Normal),
            (49, DrumClass::Crash, -2, DrumHead::Cross),
            (50, DrumClass::HighTom, 1, DrumHead::Normal),
            (51, DrumClass::Ride, 0, DrumHead::Cross),
            (52, DrumClass::China, -4, DrumHead::Cross),
            (53, DrumClass::RideBell, 0, DrumHead::Cross),
            (54, DrumClass::Tambourine, 4, DrumHead::Cross),
            (55, DrumClass::Splash, -3, DrumHead::Cross),
            (56, DrumClass::Cowbell, 1, DrumHead::Cross),
            (57, DrumClass::Crash, -2, DrumHead::Cross),
            (59, DrumClass::Ride, 0, DrumHead::Cross),
        ];
        for (key, class, position, head) in expect {
            let found = drum_entry(Some(key));
            assert_eq!(
                (found.class, found.position, found.head),
                (class, position, head),
                "{key}"
            );
        }
        for key in [0, 27, 34, 39, 58, 60, 81, 127] {
            assert_eq!(drum_entry(Some(key)), DEFAULT_DRUM, "{key}");
        }
        assert_eq!(drum_entry(None), DEFAULT_DRUM);
        assert_eq!(
            (DEFAULT_DRUM.position, DEFAULT_DRUM.head),
            (4, DrumHead::Normal)
        );
    }

    #[test]
    fn only_unknown_instruments_and_the_open_hi_hat_lose_detail() {
        let lossy: Vec<DrumClass> = DRUM_TABLE
            .iter()
            .chain([&DEFAULT_DRUM])
            .filter(|found| !found.fully_drawn())
            .map(|found| found.class)
            .collect();
        assert_eq!(lossy, vec![DrumClass::OpenHiHat, DrumClass::Other]);
    }

    #[test]
    fn a_staff_kit_maps_articulation_indices_to_the_typed_output_keys() {
        let named = |name: &str, key: u8| opentab_model::DrumArticulation {
            name: name.to_string(),
            staff_line: 0,
            noteheads: ["a".to_string(), "b".to_string(), "c".to_string()],
            technique_placement: opentab_model::TechniquePlacement::Outside,
            technique_symbol: String::new(),
            input_keys: vec![1],
            rse_sound: String::new(),
            output_key: key,
        };
        let element = |count: usize| opentab_model::DrumElement {
            name: String::new(),
            kind: String::new(),
            soundbank: String::new(),
            count,
        };
        let typed = DrumKit::new(
            vec![element(2), element(0), element(2)],
            vec![
                named("a1", 38),
                named("a2", 42),
                named("b1", 0),
                named("b2", 36),
            ],
        )
        .unwrap();
        let kit = DrumStaffKit::of(&typed);
        assert_eq!(kit.keys, vec![38, 42, 0, 36]);
        assert_eq!(kit.key(1), Some(42));
        assert_eq!(kit.key(2), Some(0));
        assert_eq!(kit.key(4), None);
        assert_eq!(kit.entry(3).class, DrumClass::Kick);
        assert_eq!(kit.entry(2).class, DrumClass::Other);
        assert_eq!(kit.entry(9).class, DrumClass::Other);
        let bare = DrumStaffKit::default();
        for articulation in [0, 38, 42, 127, 128, u16::MAX] {
            assert_eq!(bare.key(articulation), None);
            assert_eq!(bare.entry(articulation), DEFAULT_DRUM);
        }
    }
}
