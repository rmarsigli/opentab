use opentab_model::{
    DIRECTION_JUMPS, DIRECTION_TARGETS, DirectionJump, DirectionTarget, Directions, ElementId,
    Fraction, KeySignature, MasterBar, MasterBarId, Score, Tempo, TempoChange, TempoUnit,
    TimeSignature,
};

pub const TARGET_NAMES: [&str; 5] = ["Segno", "SegnoSegno", "Coda", "DoubleCoda", "Fine"];
pub const JUMP_NAMES: [&str; 14] = [
    "DaCapo",
    "DaCapoAlFine",
    "DaCapoAlCoda",
    "DaCapoAlDoubleCoda",
    "DaSegno",
    "DaSegnoAlFine",
    "DaSegnoAlCoda",
    "DaSegnoAlDoubleCoda",
    "DaSegnoSegno",
    "DaSegnoSegnoAlFine",
    "DaSegnoSegnoAlCoda",
    "DaSegnoSegnoAlDoubleCoda",
    "DaCoda",
    "DaDoubleCoda",
];

pub fn master_bar(index: usize, numerator: u8, denominator: u8) -> MasterBar {
    MasterBar {
        id: MasterBarId(ElementId(index as u64)),
        time_signature: TimeSignature::new(numerator, denominator).expect("time signature"),
        key_signature: KeySignature::default(),
        repeat_start: false,
        repeat_end: None,
        alternate_endings: 0,
        triplet_feel: Default::default(),
        double_bar: false,
        marker: None,
        directions: Directions::default(),
        tempo_changes: Vec::new(),
    }
}

pub fn plain_score(bars: usize) -> Score {
    let mut score = Score::new();
    score.master_bars = (0..bars).map(|index| master_bar(index, 4, 4)).collect();
    score
}

fn target(name: &str) -> Option<DirectionTarget> {
    TARGET_NAMES
        .iter()
        .position(|candidate| *candidate == name)
        .map(|index| DIRECTION_TARGETS[index])
}

fn jump(name: &str) -> Option<DirectionJump> {
    JUMP_NAMES
        .iter()
        .position(|candidate| *candidate == name)
        .map(|index| DIRECTION_JUMPS[index])
}

pub fn apply_token(bar: &mut MasterBar, token: &str) {
    if token == "[S]" {
        bar.repeat_start = true;
    } else if let Some(count) = token
        .strip_prefix("[E")
        .and_then(|rest| rest.strip_suffix(']'))
    {
        bar.repeat_end = Some(count.parse().expect("repeat count"));
    } else if let Some(endings) = token
        .strip_prefix('{')
        .and_then(|rest| rest.strip_suffix('}'))
    {
        for ending in endings.split(',') {
            let ending: u8 = ending.trim().parse().expect("ending");
            bar.alternate_endings |= 1 << (ending - 1);
        }
    } else if let Some(target) = target(token) {
        bar.directions.targets.push(target);
    } else if let Some(jump) = jump(token) {
        bar.directions.jumps.push(jump);
    } else {
        panic!("unknown token {token}");
    }
}

pub fn score_from(bars: usize, input: &str) -> Score {
    let mut score = plain_score(bars);
    for part in input
        .split(';')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let spaced = part.replace(']', "] ").replace('{', " {");
        let mut tokens = spaced.split_whitespace();
        let index: usize = tokens
            .next()
            .and_then(|text| text.parse().ok())
            .expect("bar index");
        for token in tokens {
            apply_token(&mut score.master_bars[index], token);
        }
    }
    score
}

pub fn numbers(text: &str) -> Vec<usize> {
    text.split_whitespace()
        .map(|number| number.parse().expect("bar number"))
        .collect()
}

pub fn tempo(position: Fraction, bpm: f64, unit: TempoUnit, linear: bool) -> TempoChange {
    TempoChange {
        position,
        bpm: Tempo::new(bpm).expect("tempo"),
        unit,
        linear,
    }
}

pub fn frac(numer: i64, denom: i64) -> Fraction {
    Fraction::new(numer, denom)
}
