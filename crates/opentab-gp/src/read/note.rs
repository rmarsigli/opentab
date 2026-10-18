use opentab_model::{
    Accent, Beat, Bend, BendPoint, Harmonic, InstrumentKind, KeySignature, Note, NoteEffects,
    NoteId, NoteKind, Tap,
};

use crate::canon;
use crate::codes::{
    ACCENT_BIT, FINGERS, GHOST, HARMONICS, HEAVY_ACCENT_BIT, SLIDES, STACCATO_BIT, VIBRATOS,
    accent_bits, lookup,
};
use crate::dom::Element;
use crate::error::GpifProblem;
use crate::guard::{self, PitchInputs};
use crate::location::Location;
use crate::number::{self, Decimal};
use crate::opaque::ParentPath;
use crate::read::context::{Context, Fault, Handled, Owner, Scope, Visit};
use crate::read::pools::Pools;
use crate::read::property::{is_flag, single_text};
use crate::read::track::StaffFrame;

const BEND_PROPERTIES: [&str; 8] = [
    "Bended",
    "BendOriginOffset",
    "BendOriginValue",
    "BendMiddleOffset1",
    "BendMiddleOffset2",
    "BendMiddleValue",
    "BendDestinationOffset",
    "BendDestinationValue",
];
const HARMONIC_PROPERTIES: [&str; 3] = ["Harmonic", "HarmonicType", "HarmonicFret"];
const TAP_PROPERTIES: [&str; 2] = ["Tapped", "LeftHandTapped"];
const PITCH_PROPERTIES: [&str; 3] = ["Midi", "ConcertPitch", "TransposedPitch"];
const POSITION_PROPERTIES: [&str; 2] = ["String", "Fret"];
const MAX_BEND_OFFSET: i64 = 100;
const BEND_VALUE_FACTOR: i128 = 2;

enum Group<T> {
    Absent,
    Mapped(T),
    Unmappable,
}

impl<T: Copy> Group<T> {
    fn value(&self) -> Option<T> {
        match self {
            Group::Mapped(value) => Some(*value),
            _ => None,
        }
    }

    fn handled(&self, guard: String) -> Handled {
        match self {
            Group::Mapped(_) => Handled::Mapped(&["name"]),
            _ => Handled::Replayed(guard),
        }
    }
}

struct NoteFrame<'f> {
    kind: NoteKind,
    previous: Option<&'f Beat>,
    key: KeySignature,
    pitch: PitchInputs<'f>,
    bend: Group<Bend>,
    harmonic: Group<Harmonic>,
    tap: Group<Tap>,
}

struct Properties<'e> {
    items: Vec<(&'e str, &'e Element, Location)>,
}

impl<'e> Properties<'e> {
    fn of(note: &'e Element, location: &Location) -> Properties<'e> {
        let mut items = Vec::new();
        if let Some((index, container)) = note.child("Properties") {
            for (position, property) in container.elements() {
                if property.name == "Property"
                    && let Some(name) = property.attribute("name")
                {
                    items.push((name, property, location.child(index).child(position)));
                }
            }
        }
        Properties { items }
    }

    fn get(&self, name: &str) -> Option<(&'e Element, &Location)> {
        self.items
            .iter()
            .find(|(candidate, _, _)| *candidate == name)
            .map(|(_, property, location)| (*property, location))
    }

    fn any(&self, names: &[&str]) -> bool {
        names.iter().any(|name| self.get(name).is_some())
    }
}

pub(crate) fn read_note(
    ctx: &mut Context,
    pools: &mut Pools<'_>,
    staff: &StaffFrame<'_>,
    token: &str,
    at: &Location,
    previous: Option<&Beat>,
) -> Result<Note, Fault> {
    let (element, location) = pools.notes.fetch(token, at)?;
    let id: NoteId = ctx.alloc(&location)?;
    let scope = Scope::new(Owner::Element(id.0), ParentPath::owner(), location.clone());
    ctx.keep_attributes(&scope, element, &["id"])?;
    let properties = Properties::of(element, &location);
    let kind = note_kind(element, &location, &properties, staff)?;
    let frame = NoteFrame {
        kind,
        previous,
        key: staff.key,
        pitch: PitchInputs {
            kind,
            tuning: staff.tuning,
            capo: staff.capo,
            transpose: staff.transpose,
        },
        bend: bend_group(&properties),
        harmonic: harmonic_group(&properties),
        tap: tap_group(&properties),
    };
    let mut effects = NoteEffects {
        bend: frame.bend.value(),
        harmonic: frame.harmonic.value(),
        tap: frame.tap.value(),
        ..NoteEffects::default()
    };
    ctx.visit(&scope, element, |ctx, visit| {
        read_note_child(ctx, &scope, visit, &frame, &mut effects)
    })?;
    Ok(Note { id, kind, effects })
}

fn note_kind(
    element: &Element,
    location: &Location,
    properties: &Properties<'_>,
    staff: &StaffFrame<'_>,
) -> Result<NoteKind, Fault> {
    if staff.kind == InstrumentKind::Percussion {
        let articulation = match element.child("InstrumentArticulation") {
            Some((index, child)) => {
                let text = child.text();
                number::integer::<u16>(&text).ok_or_else(|| {
                    Fault::at(
                        &location.child(index),
                        GpifProblem::InvalidNumber {
                            text: text.trim().to_string(),
                        },
                    )
                })?
            }
            None => 0,
        };
        return Ok(NoteKind::Percussion { articulation });
    }
    let string: u32 = property_integer(properties, "String", "String", location)?;
    let fret: u8 = property_integer(properties, "Fret", "Fret", location)?;
    let strings = staff.tuning.len();
    let (_, string_location) = properties.get("String").unwrap_or((element, location));
    let out_of_range = || {
        Fault::at(
            string_location,
            GpifProblem::StringOutOfRange { string, strings },
        )
    };
    let index = usize::try_from(string).map_err(|_| out_of_range())?;
    let reversed = strings
        .checked_sub(1)
        .and_then(|highest| highest.checked_sub(index))
        .ok_or_else(out_of_range)?;
    let string = u8::try_from(reversed).map_err(|_| out_of_range())?;
    Ok(NoteKind::Fretted { string, fret })
}

fn property_integer<T: std::str::FromStr>(
    properties: &Properties<'_>,
    name: &str,
    child: &str,
    note: &Location,
) -> Result<T, Fault> {
    let Some((property, location)) = properties.get(name) else {
        return Err(Fault::at(
            note,
            GpifProblem::MissingElement {
                name: format!("Property[@name=\"{name}\"]"),
            },
        ));
    };
    let text = single_text(property, child).ok_or_else(|| {
        Fault::at(
            location,
            GpifProblem::MissingElement {
                name: child.to_string(),
            },
        )
    })?;
    number::integer(&text).ok_or_else(|| {
        Fault::at(
            location,
            GpifProblem::InvalidNumber {
                text: text.trim().to_string(),
            },
        )
    })
}

fn read_note_child(
    ctx: &mut Context,
    scope: &Scope,
    visit: &Visit<'_>,
    frame: &NoteFrame<'_>,
    effects: &mut NoteEffects,
) -> Result<Handled, Fault> {
    match visit.key.as_str() {
        "Vibrato" => match lookup(VIBRATOS, &visit.text()) {
            Some(vibrato) => effects.vibrato = Some(vibrato),
            None => return Ok(Handled::Replayed(guard::vibrato(None))),
        },
        "AntiAccent" => {
            if visit.text().trim() != GHOST {
                return Ok(Handled::Replayed(guard::ghost(false)));
            }
            effects.ghost = true;
        }
        "LeftFingering" => match lookup(FINGERS, &visit.text()) {
            Some(finger) => effects.left_hand_finger = Some(finger),
            None => return Ok(Handled::Replayed(guard::left_finger(None))),
        },
        "RightFingering" => match lookup(FINGERS, &visit.text()) {
            Some(finger) => effects.right_hand_finger = Some(finger),
            None => return Ok(Handled::Replayed(guard::right_finger(None))),
        },
        "LetRing" => effects.let_ring = true,
        "Tie" => {
            effects.tie_origin = tie_flag(visit, "origin")?;
            effects.tie_destination = tie_flag(visit, "destination")?;
            return Ok(Handled::Mapped(&["origin", "destination"]));
        }
        "Accent" => return read_accent(visit, effects),
        "InstrumentArticulation" => {
            if !matches!(frame.kind, NoteKind::Percussion { .. }) {
                return Ok(Handled::Replayed(guard::note_kind(frame.kind)));
            }
        }
        "Properties" => {
            let inner = scope.enter(visit);
            ctx.visit(&inner, visit.element, |_, property| {
                Ok(note_property(property, frame, effects))
            })?;
        }
        _ => return Ok(Handled::Unknown),
    }
    Ok(Handled::Mapped(&[]))
}

fn tie_flag(visit: &Visit<'_>, name: &str) -> Result<bool, Fault> {
    match visit.element.attribute(name) {
        Some(text) => number::boolean(text).ok_or_else(|| {
            visit.fault(GpifProblem::InvalidValue {
                text: text.to_string(),
            })
        }),
        None => Ok(false),
    }
}

fn read_accent(visit: &Visit<'_>, effects: &mut NoteEffects) -> Result<Handled, Fault> {
    let bits: u32 = visit.integer()?;
    effects.staccato = bits & STACCATO_BIT != 0;
    effects.accent = match (bits & ACCENT_BIT != 0, bits & HEAVY_ACCENT_BIT != 0) {
        (true, false) => Some(Accent::Normal),
        (false, true) => Some(Accent::Heavy),
        _ => None,
    };
    let canonical = accent_bits(effects.accent, effects.staccato);
    if canonical != 0 && canonical == bits {
        Ok(Handled::Mapped(&[]))
    } else {
        Ok(Handled::Replayed(guard::accent(
            effects.accent,
            effects.staccato,
        )))
    }
}

fn note_property(
    property: &Visit<'_>,
    frame: &NoteFrame<'_>,
    effects: &mut NoteEffects,
) -> Handled {
    let Some(name) = property.key.strip_prefix("Property@") else {
        return Handled::Unknown;
    };
    if PITCH_PROPERTIES.contains(&name) {
        let computed = canon::pitch_property(name, &frame.pitch, frame.key);
        if computed.as_ref() == Some(property.element) {
            return Handled::Mapped(&["name"]);
        }
        return Handled::Replayed(guard::pitch(&frame.pitch));
    }
    if POSITION_PROPERTIES.contains(&name) {
        return match frame.kind {
            NoteKind::Fretted { .. } => Handled::Mapped(&["name"]),
            NoteKind::Percussion { .. } => Handled::Replayed(guard::pitch(&frame.pitch)),
        };
    }
    if BEND_PROPERTIES.contains(&name) {
        return frame.bend.handled(guard::bend(None));
    }
    if HARMONIC_PROPERTIES.contains(&name) {
        return frame.harmonic.handled(guard::harmonic(None));
    }
    if TAP_PROPERTIES.contains(&name) {
        return frame.tap.handled(guard::tap(None));
    }
    if name == "HopoDestination" {
        return Handled::Replayed(guard::hopo(frame.previous, frame.kind));
    }
    let flag = match name {
        "HopoOrigin" => &mut effects.hammer_on_pull_off,
        "Muted" => &mut effects.dead,
        "PalmMuted" => &mut effects.palm_mute,
        "Slide" => return read_slide(property, effects),
        _ => return Handled::Unknown,
    };
    if is_flag(property.element) {
        *flag = true;
        Handled::Mapped(&["name"])
    } else {
        Handled::Replayed(guard::flag(name, false))
    }
}

fn read_slide(property: &Visit<'_>, effects: &mut NoteEffects) -> Handled {
    let slide = single_text(property.element, "Flags")
        .and_then(|text| number::integer::<u32>(&text))
        .and_then(|bits| {
            SLIDES
                .iter()
                .find(|(candidate, _)| *candidate == bits)
                .map(|(_, slide)| *slide)
        });
    match slide {
        Some(slide) => {
            effects.slide = Some(slide);
            Handled::Mapped(&["name"])
        }
        None => Handled::Replayed(guard::slide(None)),
    }
}

fn float_of(properties: &Properties<'_>, name: &str) -> Option<Decimal> {
    let (property, _) = properties.get(name)?;
    single_text(property, "Float").and_then(|text| Decimal::parse(&text))
}

fn bend_point(properties: &Properties<'_>, offset: &str, value: &str) -> Option<BendPoint> {
    let offset = float_of(properties, offset)?
        .to_integer()
        .filter(|offset| (0..=MAX_BEND_OFFSET).contains(offset))?;
    let value = float_of(properties, value)?.scaled_integer(BEND_VALUE_FACTOR)?;
    BendPoint::new(u8::try_from(offset).ok()?, i16::try_from(value).ok()?).ok()
}

fn bend_group(properties: &Properties<'_>) -> Group<Bend> {
    if !properties.any(&BEND_PROPERTIES) {
        return Group::Absent;
    }
    let mapped = (|| {
        let (bended, _) = properties.get("Bended")?;
        if !is_flag(bended) {
            return None;
        }
        let origin = bend_point(properties, "BendOriginOffset", "BendOriginValue")?;
        let middle_start = bend_point(properties, "BendMiddleOffset1", "BendMiddleValue")?;
        let middle_end = bend_point(properties, "BendMiddleOffset2", "BendMiddleValue")?;
        let destination = bend_point(properties, "BendDestinationOffset", "BendDestinationValue")?;
        Bend::new(origin, middle_start, middle_end, destination).ok()
    })();
    mapped.map_or(Group::Unmappable, Group::Mapped)
}

fn harmonic_group(properties: &Properties<'_>) -> Group<Harmonic> {
    if !properties.any(&HARMONIC_PROPERTIES) {
        return Group::Absent;
    }
    let mapped = (|| {
        let (enabled, _) = properties.get("Harmonic")?;
        if !is_flag(enabled) {
            return None;
        }
        let (kind, _) = properties.get("HarmonicType")?;
        let kind = lookup(HARMONICS, &single_text(kind, "HType")?)?;
        let (fret, _) = properties.get("HarmonicFret")?;
        let fret = Decimal::parse(&single_text(fret, "HFret")?)?;
        if fret.is_negative() {
            return None;
        }
        Some(Harmonic {
            kind,
            fret: fret.to_fraction()?,
        })
    })();
    mapped.map_or(Group::Unmappable, Group::Mapped)
}

fn tap_group(properties: &Properties<'_>) -> Group<Tap> {
    let right = properties.get("Tapped");
    let left = properties.get("LeftHandTapped");
    match (right, left) {
        (None, None) => Group::Absent,
        (Some((right, _)), None) if is_flag(right) => Group::Mapped(Tap::Right),
        (None, Some((left, _))) if is_flag(left) => Group::Mapped(Tap::Left),
        _ => Group::Unmappable,
    }
}
