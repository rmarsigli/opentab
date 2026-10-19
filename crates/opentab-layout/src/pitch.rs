use opentab_model::{HarmonicKind, KeySignature, Note, NoteKind, Track};

use crate::structure::ClefSign;

pub const STEPS_PER_OCTAVE: i64 = 7;
pub const LETTERS: usize = 7;
pub const SEMITONES_PER_OCTAVE: i64 = 12;
pub const LETTER_SEMITONES: [i64; 7] = [0, 2, 4, 5, 7, 9, 11];
pub const SHARP_ORDER: [usize; 7] = [3, 0, 4, 1, 5, 2, 6];
pub const FLAT_ORDER: [usize; 7] = [6, 2, 5, 1, 4, 0, 3];
pub const SHARP_SPELLING: [(usize, i64); 12] = [
    (0, 0),
    (0, 1),
    (1, 0),
    (1, 1),
    (2, 0),
    (3, 0),
    (3, 1),
    (4, 0),
    (4, 1),
    (5, 0),
    (5, 1),
    (6, 0),
];
pub const FLAT_SPELLING: [(usize, i64); 12] = [
    (0, 0),
    (1, -1),
    (1, 0),
    (2, -1),
    (2, 0),
    (3, 0),
    (4, -1),
    (4, 0),
    (5, -1),
    (5, 0),
    (6, -1),
    (6, 0),
];
pub const TREBLE_TOP_STEP: i64 = 5 * STEPS_PER_OCTAVE + 3;
pub const BASS_TOP_STEP: i64 = 3 * STEPS_PER_OCTAVE + 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Spelling {
    pub step: i64,
    pub alter: i64,
}

impl Spelling {
    pub fn letter(self) -> usize {
        usize::try_from(self.step.rem_euclid(STEPS_PER_OCTAVE)).unwrap_or_default()
    }

    pub fn octave(self) -> i64 {
        self.step.div_euclid(STEPS_PER_OCTAVE)
    }

    pub fn midi(self) -> i64 {
        (self.octave() + 1) * SEMITONES_PER_OCTAVE + LETTER_SEMITONES[self.letter()] + self.alter
    }
}

pub const NATURAL_HARMONICS: [(i64, i64, i64); 9] = [
    (12, 1, 12),
    (7, 1, 19),
    (19, 1, 19),
    (5, 1, 24),
    (24, 1, 24),
    (4, 1, 28),
    (9, 1, 28),
    (16, 1, 28),
    (16, 5, 31),
];

pub fn natural_harmonic(track: &Track, note: &Note) -> Option<i64> {
    let harmonic = note.effects.harmonic?;
    if harmonic.kind != HarmonicKind::Natural {
        return None;
    }
    let NoteKind::Fretted { string, .. } = note.kind else {
        return None;
    };
    let open = track.tuning.strings.get(usize::from(string))?;
    let fret = (*harmonic.fret.numer(), *harmonic.fret.denom());
    let (_, _, partial) = NATURAL_HARMONICS
        .iter()
        .find(|(numerator, denominator, _)| (*numerator, *denominator) == fret)?;
    Some(i64::from(open.0) + i64::from(track.capo) + partial)
}

pub fn sounding_midi(track: &Track, note: &Note) -> Option<i64> {
    let fretted = i64::from(track.pitch_of(note)?.0);
    Some(natural_harmonic(track, note).unwrap_or(fretted))
}

pub fn written_midi(track: &Track, note: &Note) -> Option<i64> {
    let sounding = sounding_midi(track, note)?;
    Some(
        sounding
            - i64::from(track.transpose.chromatic())
            - SEMITONES_PER_OCTAVE * i64::from(track.transpose.octave()),
    )
}

pub fn key_alter(key: KeySignature, letter: usize) -> i64 {
    let count = usize::from(key.accidentals().unsigned_abs()).min(LETTERS);
    if key.accidentals() > 0 && SHARP_ORDER[..count].contains(&letter) {
        1
    } else if key.accidentals() < 0 && FLAT_ORDER[..count].contains(&letter) {
        -1
    } else {
        0
    }
}

fn from_letter(midi: i64, letter: usize, alter: i64) -> Spelling {
    let octave = (midi - alter - LETTER_SEMITONES[letter]).div_euclid(SEMITONES_PER_OCTAVE) - 1;
    Spelling {
        step: octave * STEPS_PER_OCTAVE + i64::try_from(letter).unwrap_or_default(),
        alter,
    }
}

pub fn spell(midi: i64, key: KeySignature) -> Spelling {
    let class = midi.rem_euclid(SEMITONES_PER_OCTAVE);
    let table = if key.accidentals() < 0 {
        &FLAT_SPELLING
    } else {
        &SHARP_SPELLING
    };
    let (letter, alter) = table[usize::try_from(class).unwrap_or_default()];
    from_letter(midi, letter, alter)
}

pub const FIFTH: i64 = 7;
pub const MAX_KEY_ACCIDENTALS: i64 = 7;

pub fn written_key(key: KeySignature, chromatic: i8) -> KeySignature {
    let semitones = (-i64::from(chromatic)).rem_euclid(SEMITONES_PER_OCTAVE);
    if semitones == 0 {
        return key;
    }
    let shift = (semitones * FIFTH).rem_euclid(SEMITONES_PER_OCTAVE);
    let up = i64::from(key.accidentals()) + shift;
    let down = up - SEMITONES_PER_OCTAVE;
    let chosen = if up.abs() <= down.abs() { up } else { down };
    let clamped = chosen.clamp(-MAX_KEY_ACCIDENTALS, MAX_KEY_ACCIDENTALS);
    i8::try_from(clamped)
        .ok()
        .and_then(|accidentals| KeySignature::new(accidentals, key.minor()).ok())
        .unwrap_or(key)
}

pub fn top_step(clef: ClefSign) -> i64 {
    match clef {
        ClefSign::Bass => BASS_TOP_STEP,
        ClefSign::Treble | ClefSign::Treble8vb | ClefSign::Neutral | ClefSign::Tab => {
            TREBLE_TOP_STEP
        }
    }
}

pub fn position(clef: ClefSign, step: i64) -> i64 {
    top_step(clef) - step
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(accidentals: i8) -> KeySignature {
        KeySignature::new(accidentals, false).unwrap()
    }

    #[test]
    fn every_pitch_in_every_key_spells_back_to_itself_with_one_accidental_at_most() {
        for accidentals in -7..=7 {
            for midi in -60..200 {
                let spelled = spell(midi, key(accidentals));
                assert_eq!(spelled.midi(), midi, "{midi} in {accidentals}");
                assert!(spelled.alter.abs() <= 1, "{midi} in {accidentals}");
            }
        }
    }

    #[test]
    fn flat_keys_spell_with_flats_and_every_other_key_with_sharps() {
        let c = spell(60, key(0));
        assert_eq!((c.step, c.alter), (4 * 7, 0));
        assert_eq!(spell(61, key(0)), Spelling { step: 28, alter: 1 });
        assert_eq!(
            spell(61, key(-1)),
            Spelling {
                step: 29,
                alter: -1
            }
        );
        assert_eq!(spell(66, key(2)), Spelling { step: 31, alter: 1 });
        assert_eq!(spell(65, key(6)), Spelling { step: 31, alter: 0 });
        assert_eq!(spell(59, key(-6)), Spelling { step: 27, alter: 0 });
        assert_eq!(spell(60, key(7)), Spelling { step: 28, alter: 0 });
        assert_eq!(spell(66, key(6)), Spelling { step: 31, alter: 1 });
        assert_eq!(
            spell(66, key(-6)),
            Spelling {
                step: 32,
                alter: -1
            }
        );
        assert_eq!(
            spell(70, key(-1)),
            Spelling {
                step: 34,
                alter: -1
            }
        );
    }

    #[test]
    fn keys_set_the_alteration_of_their_letters_in_circle_order() {
        let sharps: Vec<usize> = (0..7)
            .filter(|letter| key_alter(key(3), *letter) == 1)
            .collect();
        assert_eq!(sharps, vec![0, 3, 4]);
        let flats: Vec<usize> = (0..7)
            .filter(|letter| key_alter(key(-2), *letter) == -1)
            .collect();
        assert_eq!(flats, vec![2, 6]);
        assert!((0..7).all(|letter| key_alter(key(0), letter) == 0));
        assert!((0..7).all(|letter| key_alter(key(7), letter) == 1));
        assert!((0..7).all(|letter| key_alter(key(-7), letter) == -1));
    }

    #[test]
    fn natural_harmonics_sound_their_partial_above_the_open_string() {
        use opentab_model::{
            ElementId, Fraction, Harmonic, Instrument, InstrumentKind, NoteEffects, NoteId, Pitch,
            TrackMidi, Transpose, Tuning,
        };
        let track = Track {
            id: opentab_model::TrackId(ElementId(1)),
            name: String::new(),
            short_name: String::new(),
            instrument: Instrument {
                kind: InstrumentKind::Fretted,
                midi_program: 0,
                set: None,
            },
            midi: TrackMidi::default_for(InstrumentKind::Fretted),
            sounds: Vec::new(),
            tuning: Tuning {
                strings: vec![Pitch(64), Pitch(59)],
            },
            capo: 2,
            transpose: Transpose::new(0, -1).unwrap(),
            staves: Vec::new(),
            automations: Vec::new(),
        };
        let note = |fret: u8, harmonic: Option<(HarmonicKind, i64, i64)>| Note {
            id: NoteId(ElementId(2)),
            kind: NoteKind::Fretted { string: 1, fret },
            effects: NoteEffects {
                harmonic: harmonic.map(|(kind, numerator, denominator)| Harmonic {
                    kind,
                    fret: Fraction::new(numerator, denominator),
                }),
                ..NoteEffects::default()
            },
        };
        let natural = |numerator, denominator| {
            written_midi(
                &track,
                &note(7, Some((HarmonicKind::Natural, numerator, denominator))),
            )
        };
        assert_eq!(natural(12, 1), Some(59 + 2 + 12 + 12));
        assert_eq!(natural(7, 1), Some(59 + 2 + 19 + 12));
        assert_eq!(natural(5, 1), Some(59 + 2 + 24 + 12));
        assert_eq!(natural(16, 5), Some(59 + 2 + 31 + 12));
        assert_eq!(natural(9, 1), Some(59 + 2 + 28 + 12));
        assert_eq!(natural(11, 1), Some(59 + 2 + 7 + 12));
        let artificial = note(7, Some((HarmonicKind::Artificial, 19, 1)));
        assert_eq!(written_midi(&track, &artificial), Some(59 + 2 + 7 + 12));
        assert_eq!(written_midi(&track, &note(7, None)), Some(59 + 2 + 7 + 12));
    }

    #[test]
    fn a_transposing_staff_writes_its_key_shifted_by_fifths() {
        let written =
            |accidentals: i8, chromatic: i8| written_key(key(accidentals), chromatic).accidentals();
        assert_eq!(written(0, 0), 0);
        assert_eq!(written(0, -2), 2);
        assert_eq!(written(0, 5), 1);
        assert_eq!(written(0, -3), -3);
        assert_eq!(written(-3, -2), -1);
        assert_eq!(written(5, -2), -5);
        assert_eq!(written(0, 12), 0);
        assert_eq!(written(7, 0), 7);
        assert_eq!(written(-6, 12), -6);
        for accidentals in -7..=7 {
            for chromatic in -12..=12 {
                let shifted = written(accidentals, chromatic);
                assert!((-7..=7).contains(&shifted));
                let tonic = |fifths: i64| (fifths * 7).rem_euclid(12);
                if shifted.abs() < 7 {
                    assert_eq!(
                        tonic(i64::from(shifted)),
                        (tonic(i64::from(accidentals)) - i64::from(chromatic)).rem_euclid(12),
                        "{accidentals} {chromatic}"
                    );
                }
            }
        }
    }

    #[test]
    fn staff_positions_count_half_spaces_down_from_the_top_line() {
        assert_eq!(position(ClefSign::Treble, spell(77, key(0)).step), 0);
        assert_eq!(position(ClefSign::Treble, spell(64, key(0)).step), 8);
        assert_eq!(position(ClefSign::Treble8vb, spell(71, key(0)).step), 4);
        assert_eq!(position(ClefSign::Treble, spell(60, key(0)).step), 10);
        assert_eq!(position(ClefSign::Bass, spell(57, key(0)).step), 0);
        assert_eq!(position(ClefSign::Bass, spell(43, key(0)).step), 8);
        assert_eq!(position(ClefSign::Bass, spell(50, key(0)).step), 4);
    }
}
