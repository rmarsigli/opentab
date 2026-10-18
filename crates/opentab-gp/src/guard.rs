use opentab_model::{
    Accent, Bend, Clef, Dynamic, Finger, Fraction, Grace, Harmonic, InstrumentKind, NoteKind,
    Pitch, Slide, Stroke, Tap, Transpose, TremoloPicking, Vibrato,
};

use crate::codes::{
    CLEFS, DYNAMICS, FINGERS, GRACES, HARMONICS, STROKE_DIRECTIONS, TREMOLOS, VIBRATOS, name_of,
    slide_bits,
};

const NONE: &str = "none";

pub(crate) struct PitchInputs<'a> {
    pub kind: NoteKind,
    pub tuning: &'a [Pitch],
    pub capo: u8,
    pub transpose: Transpose,
}

pub(crate) fn pitch(inputs: &PitchInputs<'_>) -> String {
    match inputs.kind {
        NoteKind::Fretted { string, fret } => {
            let tuning: Vec<String> = inputs
                .tuning
                .iter()
                .map(|pitch| pitch.0.to_string())
                .collect();
            format!(
                "pitch:fretted {string} {fret};tuning {};capo {};transpose {} {}",
                tuning.join(" "),
                inputs.capo,
                inputs.transpose.chromatic(),
                inputs.transpose.octave()
            )
        }
        NoteKind::Percussion { articulation } => format!("pitch:percussion {articulation}"),
    }
}

pub(crate) fn tempo_position(position: Fraction) -> String {
    format!("tempo-position:{}/{}", position.numer(), position.denom())
}

pub(crate) fn stroke(stroke: Option<Stroke>) -> String {
    match stroke {
        Some(stroke) => format!(
            "stroke:{} {}",
            name_of(STROKE_DIRECTIONS, stroke.direction),
            stroke.duration.ticks()
        ),
        None => format!("stroke:{NONE}"),
    }
}

pub(crate) fn repeat(start: bool, end: Option<u8>) -> String {
    match end {
        Some(count) => format!("repeat:{start} {count}"),
        None => format!("repeat:{start} {NONE}"),
    }
}

pub(crate) fn accent(accent: Option<Accent>, staccato: bool) -> String {
    let accent = match accent {
        Some(Accent::Normal) => "normal",
        Some(Accent::Heavy) => "heavy",
        None => NONE,
    };
    format!("accent:{accent} {staccato}")
}

pub(crate) fn slide(slide: Option<Slide>) -> String {
    match slide {
        Some(slide) => format!("slide:{}", slide_bits(slide)),
        None => format!("slide:{NONE}"),
    }
}

pub(crate) fn bend(bend: Option<Bend>) -> String {
    match bend {
        Some(bend) => {
            let points = [
                bend.origin(),
                bend.middle_start(),
                bend.middle_end(),
                bend.destination(),
            ];
            let points: Vec<String> = points
                .iter()
                .map(|point| format!("{} {}", point.offset(), point.value()))
                .collect();
            format!("bend:{}", points.join(";"))
        }
        None => format!("bend:{NONE}"),
    }
}

pub(crate) fn harmonic(harmonic: Option<Harmonic>) -> String {
    match harmonic {
        Some(harmonic) => format!(
            "harmonic:{} {}/{}",
            name_of(HARMONICS, harmonic.kind),
            harmonic.fret.numer(),
            harmonic.fret.denom()
        ),
        None => format!("harmonic:{NONE}"),
    }
}

pub(crate) fn tap(tap: Option<Tap>) -> String {
    let tap = match tap {
        Some(Tap::Right) => "right",
        Some(Tap::Left) => "left",
        None => NONE,
    };
    format!("tap:{tap}")
}

pub(crate) fn chord(name: Option<&str>) -> String {
    match name {
        Some(name) => format!("chord:={name}"),
        None => format!("chord:{NONE}"),
    }
}

pub(crate) fn instrument(kind: InstrumentKind) -> String {
    match kind {
        InstrumentKind::Fretted => "instrument:fretted".to_string(),
        InstrumentKind::Percussion => "instrument:percussion".to_string(),
    }
}

pub(crate) fn clef(clef: Clef) -> String {
    format!("clef:{}", name_of(CLEFS, clef))
}

pub(crate) fn dynamic(dynamic: Dynamic) -> String {
    format!("dynamic:{}", name_of(DYNAMICS, dynamic))
}

pub(crate) fn grace(grace: Option<Grace>) -> String {
    format!(
        "grace:{}",
        grace.map_or(NONE, |grace| name_of(GRACES, grace))
    )
}

pub(crate) fn tremolo(tremolo: Option<TremoloPicking>) -> String {
    let name = tremolo.map_or(NONE, |tremolo| name_of(TREMOLOS, tremolo.value()));
    format!("tremolo:{name}")
}

pub(crate) fn fade_in(fade_in: bool) -> String {
    format!("fade-in:{fade_in}")
}

pub(crate) fn vibrato(vibrato: Option<Vibrato>) -> String {
    let name = vibrato.map_or(NONE, |vibrato| name_of(VIBRATOS, vibrato));
    format!("vibrato:{name}")
}

pub(crate) fn ghost(ghost: bool) -> String {
    format!("ghost:{ghost}")
}

pub(crate) fn left_finger(finger: Option<Finger>) -> String {
    format!("left-finger:{}", finger_name(finger))
}

pub(crate) fn right_finger(finger: Option<Finger>) -> String {
    format!("right-finger:{}", finger_name(finger))
}

fn finger_name(finger: Option<Finger>) -> &'static str {
    finger.map_or(NONE, |finger| name_of(FINGERS, finger))
}

pub(crate) fn minor(minor: bool) -> String {
    format!("minor:{minor}")
}

pub(crate) fn flag(name: &str, value: bool) -> String {
    format!("flag:{name} {value}")
}
