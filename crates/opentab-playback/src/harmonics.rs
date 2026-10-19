use opentab_model::{Fraction, Harmonic, HarmonicKind};

use crate::perform::Profile;

const MICRO: i64 = 1_000_000;
const HALF_FRET_MICROS: i64 = MICRO / 2;

#[derive(Debug, Clone, Copy)]
struct Node {
    fret_micros: i64,
    semitones: u16,
}

#[derive(Debug, Clone, Copy)]
struct GpFret {
    numer: i64,
    denom: i64,
    semitones: u16,
}

const fn gp_fret(numer: i64, denom: i64, semitones: u16) -> GpFret {
    GpFret {
        numer,
        denom,
        semitones,
    }
}

const GP_FRETS: [GpFret; 17] = [
    gp_fret(12, 5, 36),
    gp_fret(27, 10, 34),
    gp_fret(16, 5, 31),
    gp_fret(4, 1, 28),
    gp_fret(5, 1, 24),
    gp_fret(29, 5, 34),
    gp_fret(7, 1, 19),
    gp_fret(41, 5, 36),
    gp_fret(9, 1, 28),
    gp_fret(48, 5, 34),
    gp_fret(12, 1, 12),
    gp_fret(147, 10, 34),
    gp_fret(16, 1, 28),
    gp_fret(17, 1, 36),
    gp_fret(19, 1, 19),
    gp_fret(217, 10, 34),
    gp_fret(24, 1, 24),
];

const fn node(fret_micros: i64, semitones: u16) -> Node {
    Node {
        fret_micros,
        semitones,
    }
}

const NODES: [Node; 21] = [
    node(2_311_741, 36),
    node(2_668_709, 34),
    node(3_156_413, 31),
    node(3_863_137, 28),
    node(4_980_450, 24),
    node(5_825_122, 34),
    node(7_019_550, 19),
    node(8_136_863, 36),
    node(8_843_587, 28),
    node(9_688_259, 34),
    node(12_000_000, 12),
    node(14_668_709, 34),
    node(15_863_137, 28),
    node(16_980_450, 36),
    node(19_019_550, 19),
    node(21_688_259, 34),
    node(24_000_000, 24),
    node(27_863_137, 28),
    node(31_019_550, 31),
    node(33_688_259, 34),
    node(36_000_000, 36),
];

fn distance(fret: Fraction, node: Node) -> Option<i128> {
    let numer = i128::from(*fret.numer()).checked_mul(i128::from(MICRO))?;
    let scaled =
        numer.checked_sub(i128::from(node.fret_micros).checked_mul(i128::from(*fret.denom()))?)?;
    scaled.checked_abs()
}

fn gp_semitones(fret: Fraction) -> Option<u16> {
    GP_FRETS
        .iter()
        .find(|node| {
            i128::from(*fret.numer()) * i128::from(node.denom)
                == i128::from(node.numer) * i128::from(*fret.denom())
        })
        .map(|node| node.semitones)
}

pub(crate) fn semitones(fret: Fraction, profile: Profile) -> Option<u16> {
    match profile {
        Profile::Gp => gp_semitones(fret),
        Profile::Musical => nearest_semitones(fret),
    }
}

fn nearest_semitones(fret: Fraction) -> Option<u16> {
    let denom = i128::from(*fret.denom());
    let limit = i128::from(HALF_FRET_MICROS).checked_mul(denom)?;
    NODES
        .iter()
        .filter_map(|node| distance(fret, *node).map(|gap| (gap, *node)))
        .filter(|(gap, _)| *gap <= limit)
        .min_by_key(|(gap, node)| (*gap, node.semitones))
        .map(|(_, node)| node.semitones)
}

pub(crate) fn sounding(open: u16, fretted: u16, harmonic: Harmonic, profile: Profile) -> u16 {
    let base = match harmonic.kind {
        HarmonicKind::Natural => open,
        HarmonicKind::Artificial
        | HarmonicKind::Pinch
        | HarmonicKind::Tap
        | HarmonicKind::Semi
        | HarmonicKind::Feedback => fretted,
    };
    semitones(harmonic.fret, profile).map_or(fretted, |offset| base.saturating_add(offset))
}
