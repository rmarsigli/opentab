use opentab_model::{DirectionJump, DirectionTarget, Directions, NoteValue, Score};

use crate::fixtures::FixtureError;
use crate::fixtures::sketch::{Sketch, TrackSetup, beat, duration, fretted, voice};

const STRING: u8 = 2;
const PLAIN: Mark = Mark {
    bar: 0,
    repeat_start: false,
    repeat_end: None,
    endings: 0,
    targets: &[],
    jumps: &[],
};

struct Mark {
    bar: usize,
    repeat_start: bool,
    repeat_end: Option<u8>,
    endings: u8,
    targets: &'static [DirectionTarget],
    jumps: &'static [DirectionJump],
}

const fn at(bar: usize) -> Mark {
    Mark { bar, ..PLAIN }
}

const fn targets(bar: usize, targets: &'static [DirectionTarget]) -> Mark {
    Mark {
        bar,
        targets,
        ..PLAIN
    }
}

const fn jumps(bar: usize, jumps: &'static [DirectionJump]) -> Mark {
    Mark {
        bar,
        jumps,
        ..PLAIN
    }
}

const fn opens(mark: Mark) -> Mark {
    Mark {
        repeat_start: true,
        ..mark
    }
}

const fn closes(mark: Mark, count: u8) -> Mark {
    Mark {
        repeat_end: Some(count),
        ..mark
    }
}

const fn ending(mark: Mark, endings: u8) -> Mark {
    Mark { endings, ..mark }
}

fn directed(title: &str, bars: usize, marks: &[Mark]) -> Result<Score, FixtureError> {
    let mut sketch = Sketch::new(title);
    sketch.master_bars(bars, 4, 4)?;
    for mark in marks {
        let master = sketch
            .score
            .master_bars
            .get_mut(mark.bar)
            .ok_or_else(|| FixtureError::new(format!("no master bar {}", mark.bar)))?;
        master.repeat_start |= mark.repeat_start;
        master.repeat_end = master.repeat_end.or(mark.repeat_end);
        master.alternate_endings |= mark.endings;
        master.directions = Directions {
            targets: [master.directions.targets.as_slice(), mark.targets].concat(),
            jumps: [master.directions.jumps.as_slice(), mark.jumps].concat(),
        };
    }
    let track = sketch.track(TrackSetup::guitar("Guitar", "Gtr"))?;
    let whole = duration(NoteValue::Whole)?;
    for bar in 0..bars {
        let fret =
            u8::try_from(bar).map_err(|_| FixtureError::new(format!("bar {bar} has no fret")))?;
        sketch.fill(
            track,
            bar,
            vec![voice(0, vec![beat(whole, vec![fretted(STRING, fret)])])],
        )?;
    }
    Ok(sketch.finish())
}

pub fn dc_fine() -> Result<Score, FixtureError> {
    directed(
        "D.C. al Fine",
        8,
        &[
            opens(at(1)),
            closes(at(2), 2),
            targets(3, &[DirectionTarget::Fine]),
            jumps(4, &[DirectionJump::DaCapo]),
            jumps(6, &[DirectionJump::DaCapoAlFine]),
        ],
    )
}

pub fn ds_coda() -> Result<Score, FixtureError> {
    directed(
        "D.S. al Coda",
        6,
        &[
            opens(targets(1, &[DirectionTarget::Segno])),
            closes(at(2), 2),
            jumps(3, &[DirectionJump::DaSegnoAlCoda, DirectionJump::DaCoda]),
            targets(4, &[DirectionTarget::Coda]),
        ],
    )
}

pub fn dss_double_coda() -> Result<Score, FixtureError> {
    directed(
        "D.S.S. al Double Coda",
        8,
        &[
            targets(1, &[DirectionTarget::SegnoSegno]),
            jumps(2, &[DirectionJump::DaCoda]),
            jumps(3, &[DirectionJump::DaDoubleCoda]),
            jumps(4, &[DirectionJump::DaSegnoSegnoAlDoubleCoda]),
            targets(5, &[DirectionTarget::Coda]),
            targets(6, &[DirectionTarget::DoubleCoda]),
        ],
    )
}

pub fn repeat_jumps() -> Result<Score, FixtureError> {
    directed(
        "Jumps inside repeats",
        7,
        &[
            opens(targets(1, &[DirectionTarget::Segno])),
            jumps(2, &[DirectionJump::DaSegno]),
            closes(at(3), 2),
            opens(targets(4, &[DirectionTarget::SegnoSegno])),
            closes(jumps(5, &[DirectionJump::DaSegnoSegno]), 2),
        ],
    )
}

pub fn endings() -> Result<Score, FixtureError> {
    directed(
        "Jumps in alternate endings",
        7,
        &[
            opens(at(0)),
            ending(closes(jumps(1, &[DirectionJump::DaCapo]), 2), 0b01),
            ending(at(2), 0b10),
            opens(at(3)),
            ending(closes(at(4), 2), 0b01),
            ending(jumps(5, &[DirectionJump::DaCapo]), 0b10),
        ],
    )
}

pub fn coda_held() -> Result<Score, FixtureError> {
    directed(
        "To Coda held back",
        7,
        &[
            jumps(1, &[DirectionJump::DaCoda]),
            targets(3, &[DirectionTarget::Coda]),
            jumps(5, &[DirectionJump::DaCapoAlCoda]),
        ],
    )
}

pub fn coda_free() -> Result<Score, FixtureError> {
    directed(
        "To Coda on the first visit",
        8,
        &[
            jumps(1, &[DirectionJump::DaCoda]),
            jumps(2, &[DirectionJump::DaSegno]),
            jumps(3, &[DirectionJump::DaCapoAlCoda]),
            targets(4, &[DirectionTarget::Segno]),
            targets(5, &[DirectionTarget::Coda]),
            jumps(6, &[DirectionJump::DaCoda]),
        ],
    )
}

pub fn fine_ahead() -> Result<Score, FixtureError> {
    directed(
        "To Coda before al Fine",
        7,
        &[
            jumps(1, &[DirectionJump::DaCoda]),
            targets(3, &[DirectionTarget::Coda]),
            targets(4, &[DirectionTarget::Fine]),
            jumps(5, &[DirectionJump::DaCapoAlFine]),
        ],
    )
}

pub fn dangling() -> Result<Score, FixtureError> {
    directed(
        "Jumps without targets",
        5,
        &[
            jumps(1, &[DirectionJump::DaCoda]),
            jumps(2, &[DirectionJump::DaSegno]),
            jumps(3, &[DirectionJump::DaCapoAlCoda]),
        ],
    )
}

pub fn duplicates() -> Result<Score, FixtureError> {
    directed(
        "Duplicated targets",
        7,
        &[
            targets(1, &[DirectionTarget::Segno]),
            targets(2, &[DirectionTarget::Fine]),
            jumps(3, &[DirectionJump::DaSegnoAlFine]),
            targets(4, &[DirectionTarget::Segno]),
            targets(5, &[DirectionTarget::Fine]),
        ],
    )
}
