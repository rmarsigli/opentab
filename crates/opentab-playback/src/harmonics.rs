use opentab_model::{Fraction, Harmonic, HarmonicKind};

const MICRO: i64 = 1_000_000;
const HALF_FRET_MICROS: i64 = MICRO / 2;

#[derive(Debug, Clone, Copy)]
struct Node {
    fret_micros: i64,
    semitones: u16,
}

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

pub(crate) fn harmonic_semitones(fret: Fraction) -> Option<u16> {
    let denom = i128::from(*fret.denom());
    let limit = i128::from(HALF_FRET_MICROS).checked_mul(denom)?;
    NODES
        .iter()
        .filter_map(|node| distance(fret, *node).map(|gap| (gap, *node)))
        .filter(|(gap, _)| *gap <= limit)
        .min_by_key(|(gap, node)| (*gap, node.semitones))
        .map(|(_, node)| node.semitones)
}

pub(crate) fn sounding(open: u16, fretted: u16, harmonic: Harmonic) -> u16 {
    let base = match harmonic.kind {
        HarmonicKind::Natural => open,
        HarmonicKind::Artificial
        | HarmonicKind::Pinch
        | HarmonicKind::Tap
        | HarmonicKind::Semi
        | HarmonicKind::Feedback => fretted,
    };
    harmonic_semitones(harmonic.fret).map_or(fretted, |semitones| base.saturating_add(semitones))
}
