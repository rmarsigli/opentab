use opentab_model::TripletFeel;

use crate::PlaybackError;
use crate::exact::Exact;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Swing {
    level: Exact,
    split: Exact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Swung {
    pub(crate) start: Exact,
    pub(crate) length: Exact,
}

const EIGHTH: (i128, i128) = (1, 8);
const SIXTEENTH: (i128, i128) = (1, 16);
const TRIPLET: (i128, i128) = (2, 3);
const DOTTED: (i128, i128) = (3, 4);
const SCOTTISH: (i128, i128) = (1, 4);

impl Swing {
    pub(crate) fn of(feel: TripletFeel) -> Result<Option<Swing>, PlaybackError> {
        let (level, split) = match feel {
            TripletFeel::None => return Ok(None),
            TripletFeel::TripletEighth => (EIGHTH, TRIPLET),
            TripletFeel::TripletSixteenth => (SIXTEENTH, TRIPLET),
            TripletFeel::DottedEighth => (EIGHTH, DOTTED),
            TripletFeel::DottedSixteenth => (SIXTEENTH, DOTTED),
            TripletFeel::ScottishEighth => (EIGHTH, SCOTTISH),
            TripletFeel::ScottishSixteenth => (SIXTEENTH, SCOTTISH),
        };
        Ok(Some(Swing {
            level: Exact::new(level.0, level.1)?,
            split: Exact::new(split.0, split.1)?,
        }))
    }

    fn swings(self, length: Exact) -> Result<bool, PlaybackError> {
        let units = length.div(self.level)?;
        Ok(!units.is_negative() && units.denom() == 1 && units.numer() > 0)
    }

    fn warp(self, at: Exact) -> Result<Exact, PlaybackError> {
        let span = self.level.mul(Exact::integer(2))?;
        let pairs = Exact::integer(at.div(span)?.floor());
        let base = pairs.mul(span)?;
        let within = at.sub(base)?;
        let long = self.split.mul(span)?;
        let moved = if within <= self.level {
            within.mul(long)?.div(self.level)?
        } else {
            let short = span.sub(long)?;
            long.add(within.sub(self.level)?.mul(short)?.div(self.level)?)?
        };
        base.add(moved)
    }

    pub(crate) fn apply(
        swing: Option<Swing>,
        offset: Exact,
        length: Exact,
    ) -> Result<Swung, PlaybackError> {
        let plain = Swung {
            start: offset,
            length,
        };
        let Some(swing) = swing else {
            return Ok(plain);
        };
        if !swing.swings(length)? {
            return Ok(plain);
        }
        let start = swing.warp(offset)?;
        let end = swing.warp(offset.add(length)?)?;
        Ok(Swung {
            start,
            length: end.sub(start)?,
        })
    }
}
