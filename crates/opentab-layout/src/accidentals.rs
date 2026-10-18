use std::collections::BTreeMap;

use opentab_model::KeySignature;

use crate::glyphs::{
    ACCIDENTAL_DOUBLE_FLAT, ACCIDENTAL_DOUBLE_SHARP, ACCIDENTAL_FLAT, ACCIDENTAL_NATURAL,
    ACCIDENTAL_SHARP, Glyph,
};
use crate::pitch::{Spelling, key_alter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Accidental {
    DoubleFlat,
    Flat,
    Natural,
    Sharp,
    DoubleSharp,
}

impl Accidental {
    pub fn of(alter: i64) -> Accidental {
        match alter {
            value if value <= -2 => Accidental::DoubleFlat,
            -1 => Accidental::Flat,
            0 => Accidental::Natural,
            1 => Accidental::Sharp,
            _ => Accidental::DoubleSharp,
        }
    }

    pub fn glyph(self) -> Glyph {
        match self {
            Accidental::DoubleFlat => ACCIDENTAL_DOUBLE_FLAT,
            Accidental::Flat => ACCIDENTAL_FLAT,
            Accidental::Natural => ACCIDENTAL_NATURAL,
            Accidental::Sharp => ACCIDENTAL_SHARP,
            Accidental::DoubleSharp => ACCIDENTAL_DOUBLE_SHARP,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Carry {
    key: KeySignature,
    altered: BTreeMap<i64, i64>,
}

impl Carry {
    pub fn new(key: KeySignature) -> Carry {
        Carry {
            key,
            altered: BTreeMap::new(),
        }
    }

    pub fn current(&self, step: i64) -> i64 {
        match self.altered.get(&step) {
            Some(alter) => *alter,
            None => {
                let letter = Spelling { step, alter: 0 }.letter();
                key_alter(self.key, letter)
            }
        }
    }

    pub fn mark(&mut self, spelling: Spelling, tied: bool) -> Option<Accidental> {
        if tied || self.current(spelling.step) == spelling.alter {
            return None;
        }
        self.altered.insert(spelling.step, spelling.alter);
        Some(Accidental::of(spelling.alter))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pitch::spell;

    fn key(accidentals: i8) -> KeySignature {
        KeySignature::new(accidentals, false).unwrap()
    }

    fn run(accidentals: i8, notes: &[(i64, bool)]) -> Vec<Option<Accidental>> {
        let mut carry = Carry::new(key(accidentals));
        notes
            .iter()
            .map(|(midi, tied)| carry.mark(spell(*midi, key(accidentals)), *tied))
            .collect()
    }

    #[test]
    fn notes_of_the_key_need_nothing_and_others_carry_through_the_bar() {
        assert_eq!(
            run(
                1,
                &[
                    (66, false),
                    (67, false),
                    (65, false),
                    (65, false),
                    (66, false)
                ]
            ),
            vec![
                None,
                None,
                Some(Accidental::Natural),
                None,
                Some(Accidental::Sharp)
            ]
        );
        assert_eq!(
            run(0, &[(61, false), (61, false), (60, false), (73, false)]),
            vec![
                Some(Accidental::Sharp),
                None,
                Some(Accidental::Natural),
                Some(Accidental::Sharp)
            ]
        );
        assert_eq!(
            run(-2, &[(70, false), (71, false), (70, false), (63, false)]),
            vec![
                None,
                Some(Accidental::Natural),
                Some(Accidental::Flat),
                None
            ]
        );
    }

    #[test]
    fn a_tied_note_shows_nothing_and_leaves_the_carry_alone() {
        assert_eq!(
            run(0, &[(61, true), (61, false), (61, false)]),
            vec![None, Some(Accidental::Sharp), None]
        );
        assert_eq!(
            run(2, &[(65, true), (65, false)]),
            vec![None, Some(Accidental::Natural)]
        );
    }

    #[test]
    fn every_alteration_has_its_glyph() {
        let glyphs: Vec<&str> = (-3..=3)
            .map(|alter| Accidental::of(alter).glyph().name)
            .collect();
        assert_eq!(
            glyphs,
            vec![
                "accidentalDoubleFlat",
                "accidentalDoubleFlat",
                "accidentalFlat",
                "accidentalNatural",
                "accidentalSharp",
                "accidentalDoubleSharp",
                "accidentalDoubleSharp"
            ]
        );
    }
}
