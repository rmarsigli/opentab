use opentab_model::{Note, TremoloPicking};

use super::{ArticulationPlan, BeatPlan, FingerPlan, offset_rect};
use crate::rhythm::{FittedStem, StemDirection, glyph_box};
use crate::tab::{
    FINGER_EM, baseline_below_centre, digit_height, finger_text, measure, tremolo_glyph,
};
use crate::units::{Point, Rect, STAFF_SPACE, Units};

pub const STANDARD_FINGER_GAP: Units = Units(STAFF_SPACE.0 / 6);
pub const OPEN_TREMOLO_GAP: Units = Units(STAFF_SPACE.0 / 2);

pub fn tremolo(
    beat: &BeatPlan,
    tremolo: Option<TremoloPicking>,
    stem: Option<&FittedStem>,
) -> Option<ArticulationPlan> {
    let tremolo = tremolo?;
    if beat.heads.is_empty() {
        return None;
    }
    let glyph = tremolo_glyph(tremolo);
    let rect = glyph_box(glyph, beat.size);
    if let Some(fitted) = stem {
        return Some(ArticulationPlan {
            glyph,
            dx: fitted.anchor.dx,
            y: fitted.anchor.tip + (fitted.end - fitted.anchor.tip).half(),
        });
    }
    let head = beat.heads.iter().min_by_key(|head| head.y)?;
    let lowest = beat.heads.iter().map(|head| head.y).max()?;
    let half = glyph_box(head.glyph, beat.size).max.y;
    let y = match beat.direction {
        StemDirection::Up => head.y - half - OPEN_TREMOLO_GAP - rect.max.y,
        StemDirection::Down => lowest + half + OPEN_TREMOLO_GAP - rect.min.y,
    };
    Some(ArticulationPlan {
        glyph,
        dx: head.dx + head.width.half(),
        y,
    })
}

pub fn finger_rect(plan: &FingerPlan) -> Rect {
    Rect::new(
        Point {
            x: plan.right - plan.width,
            y: plan.baseline - digit_height(FINGER_EM),
        },
        Point {
            x: plan.right,
            y: plan.baseline,
        },
    )
}

pub fn fingers(beat: &BeatPlan, notes: &[Note]) -> Vec<FingerPlan> {
    let left = beat
        .heads
        .iter()
        .map(|head| head.accidental.map_or(head.dx, |mark| mark.dx.min(head.dx)))
        .min()
        .unwrap_or_default();
    let mut heads: Vec<_> = beat.heads.iter().collect();
    heads.sort_by_key(|head| head.y);
    let mut placed: Vec<FingerPlan> = Vec::new();
    for head in heads {
        let Some(finger) = notes
            .iter()
            .find(|note| note.id == head.id)
            .and_then(|note| note.effects.left_hand_finger)
        else {
            continue;
        };
        let text = finger_text(finger);
        let mut plan = FingerPlan {
            text,
            right: left - STANDARD_FINGER_GAP,
            baseline: head.y + baseline_below_centre(FINGER_EM),
            width: measure(text, FINGER_EM),
        };
        while let Some(blocker) = placed
            .iter()
            .find(|other| finger_rect(other).overlaps(&finger_rect(&plan)))
        {
            plan.right = blocker.right - blocker.width - STANDARD_FINGER_GAP;
        }
        placed.push(plan);
    }
    placed
}

pub fn guitar_rects(beat: &BeatPlan) -> Vec<Rect> {
    let mut rects: Vec<Rect> = beat.fingers.iter().map(finger_rect).collect();
    if let Some(mark) = beat.tremolo {
        rects.push(offset_rect(
            glyph_box(mark.glyph, beat.size),
            Point {
                x: mark.dx,
                y: mark.y,
            },
        ));
    }
    rects
}
