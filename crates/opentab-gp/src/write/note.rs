use opentab_model::{
    Beat, Bend, BendPoint, Finger, Fraction, Harmonic, KeySignature, Note, NoteEffects, NoteKind,
    Tap,
};

use crate::canon::{self, CONCERT_PITCH, MIDI, TRANSPOSED_PITCH};
use crate::codes::{FINGERS, GHOST, HARMONICS, SLIDES, VIBRATOS, accent_bits, name_of};
use crate::dom::Element;
use crate::error::Refusal;
use crate::guard::PitchInputs;
use crate::write::payload::{self, Env, Item, NoteEnv};
use crate::write::state::{Owners, StaffContext};
use crate::write::track::{apply_own, check_kind};
use crate::write::xml::{Fail, Within};

const FLOAT_DECIMALS: u32 = 6;
const MAX_DECIMALS: u32 = 30;
const PROPERTIES: &str = "Properties";

fn flag(name: &str) -> Element {
    Element::property(name, Element::new("Enable"))
}

fn float(name: &str, text: String) -> Element {
    Element::property(name, Element::leaf("Float", &text))
}

fn halves(value: i16) -> String {
    let sign = if value < 0 { "-" } else { "" };
    let magnitude = value.unsigned_abs();
    let fraction = if magnitude % 2 == 1 { "5" } else { "0" };
    format!("{sign}{}.{fraction}00000", magnitude / 2)
}

fn offset(point: BendPoint) -> String {
    format!("{}.000000", point.offset())
}

fn bend_properties(bend: Bend, out: &mut Vec<Element>) {
    out.push(flag("Bended"));
    let destination = bend.destination();
    out.push(float("BendDestinationOffset", offset(destination)));
    out.push(float("BendDestinationValue", halves(destination.value())));
    out.push(float("BendMiddleOffset1", offset(bend.middle_start())));
    out.push(float("BendMiddleOffset2", offset(bend.middle_end())));
    out.push(float("BendMiddleValue", halves(bend.middle_value())));
    let origin = bend.origin();
    out.push(float("BendOriginOffset", offset(origin)));
    out.push(float("BendOriginValue", halves(origin.value())));
}

fn decimal(value: Fraction) -> Option<String> {
    let numer = i128::from(*value.numer());
    let denom = i128::from(*value.denom());
    if numer < 0 || denom <= 0 {
        return None;
    }
    let mut rest = denom;
    let mut twos = 0u32;
    let mut fives = 0u32;
    while rest % 2 == 0 {
        rest /= 2;
        twos += 1;
    }
    while rest % 5 == 0 {
        rest /= 5;
        fives += 1;
    }
    if rest != 1 {
        return None;
    }
    let places = twos.max(fives).max(FLOAT_DECIMALS);
    if places > MAX_DECIMALS {
        return None;
    }
    let scale = 10i128.checked_pow(places)?;
    let scaled = numer.checked_mul(scale)? / denom;
    let whole = scaled / scale;
    let fraction = scaled % scale;
    let width = usize::try_from(places).ok()?;
    Some(format!("{whole}.{fraction:0width$}"))
}

fn harmonic_properties(harmonic: Harmonic, out: &mut Vec<Element>) -> Result<(), Refusal> {
    let fret = decimal(harmonic.fret).ok_or(Refusal::HarmonicFret {
        numer: *harmonic.fret.numer(),
        denom: *harmonic.fret.denom(),
    })?;
    out.push(flag("Harmonic"));
    out.push(Element::property(
        "HarmonicFret",
        Element::leaf("HFret", &fret),
    ));
    out.push(Element::property(
        "HarmonicType",
        Element::leaf("HType", name_of(HARMONICS, harmonic.kind)),
    ));
    Ok(())
}

fn slide_flags(effects: &NoteEffects) -> Option<u32> {
    let slide = effects.slide?;
    SLIDES
        .iter()
        .find(|(_, candidate)| *candidate == slide)
        .map(|(bits, _)| *bits)
}

fn has_guarded(items: &[Item], key: &str) -> bool {
    items.iter().any(|item| {
        item.guard.is_some()
            && item.parent == PROPERTIES
            && item.element().is_some_and(|element| element.key() == key)
    })
}

fn pitch_property(
    name: &str,
    inputs: &PitchInputs<'_>,
    key: KeySignature,
    items: &[Item],
    out: &mut Vec<Element>,
) -> Result<(), Refusal> {
    if has_guarded(items, &format!("Property@{name}")) {
        return Ok(());
    }
    let property = canon::pitch_property(name, inputs, key).ok_or(Refusal::Pitch)?;
    out.push(property);
    Ok(())
}

fn properties(
    note: &Note,
    inputs: &PitchInputs<'_>,
    key: KeySignature,
    items: &[Item],
    strings: usize,
) -> Result<Vec<Element>, Fail> {
    let effects = &note.effects;
    let mut out = Vec::new();
    if let Some(bend) = effects.bend {
        bend_properties(bend, &mut out);
    }
    let fretted = match note.kind {
        NoteKind::Fretted { string, fret } => {
            if usize::from(string) >= strings {
                return Err(Fail::new(Refusal::String { string, strings })).within(field("kind"));
            }
            Some((string, fret))
        }
        NoteKind::Percussion { .. } => None,
    };
    if let Some((_, fret)) = fretted {
        pitch_property(CONCERT_PITCH, inputs, key, items, &mut out).within(field("kind"))?;
        out.push(Element::property(
            "Fret",
            Element::leaf("Fret", &fret.to_string()),
        ));
    }
    if let Some(harmonic) = effects.harmonic {
        harmonic_properties(harmonic, &mut out).within(field("effects.harmonic"))?;
    }
    if effects.hammer_on_pull_off {
        out.push(flag("HopoOrigin"));
    }
    if effects.tap == Some(Tap::Left) {
        out.push(flag("LeftHandTapped"));
    }
    if fretted.is_some() {
        pitch_property(MIDI, inputs, key, items, &mut out).within(field("kind"))?;
    }
    if effects.dead {
        out.push(flag("Muted"));
    }
    if effects.palm_mute {
        out.push(flag("PalmMuted"));
    }
    if let Some(flags) = slide_flags(effects) {
        out.push(Element::property(
            "Slide",
            Element::leaf("Flags", &flags.to_string()),
        ));
    }
    if let Some((string, _)) = fretted {
        let reversed = strings - 1 - usize::from(string);
        out.push(Element::property(
            "String",
            Element::leaf("String", &reversed.to_string()),
        ));
    }
    if effects.tap == Some(Tap::Right) {
        out.push(flag("Tapped"));
    }
    if fretted.is_some() {
        pitch_property(TRANSPOSED_PITCH, inputs, key, items, &mut out).within(field("kind"))?;
    }
    Ok(out)
}

fn finger(name: &str, finger: Option<Finger>) -> Option<Element> {
    finger.map(|finger| Element::leaf(name, name_of(FINGERS, finger)))
}

fn field(name: &'static str) -> impl FnOnce() -> String {
    move || name.to_string()
}

pub(crate) struct NoteSite<'s> {
    pub staff: &'s StaffContext,
    pub key: KeySignature,
    pub previous: Option<&'s Beat>,
}

pub(crate) fn write_note(
    owners: &mut Owners<'_>,
    site: &NoteSite<'_>,
    note: &Note,
    id: usize,
) -> Result<Element, Fail> {
    let staff = site.staff;
    let key = site.key;
    let percussion = matches!(note.kind, NoteKind::Percussion { .. });
    check_kind(staff.kind, percussion).within(field("kind"))?;
    let effects = &note.effects;
    if effects.right_hand_finger == Some(Finger::Open) {
        return Err(Fail::new(Refusal::RightHandOpen)).within(field("effects.right_hand_finger"));
    }
    let env = NoteEnv {
        note,
        inputs: PitchInputs {
            kind: note.kind,
            tuning: &staff.tuning,
            capo: staff.capo,
            transpose: staff.transpose,
        },
        previous: site.previous,
    };
    let inputs = &env.inputs;
    let items = payload::surviving(owners.items(note.id.0)?, &Env::Note(&env))?;
    let mut element = Element::new("Note").with_attribute("id", &id.to_string());
    if let Some(vibrato) = effects.vibrato {
        element.push(Element::leaf("Vibrato", name_of(VIBRATOS, vibrato)));
    }
    if effects.ghost {
        element.push(Element::leaf("AntiAccent", GHOST));
    }
    if effects.let_ring {
        element.push(Element::new("LetRing"));
    }
    let bits = accent_bits(effects.accent, effects.staccato);
    if bits != 0 {
        element.push(Element::leaf("Accent", &bits.to_string()));
    }
    if effects.tie_origin || effects.tie_destination {
        element.push(
            Element::new("Tie")
                .with_attribute("origin", &effects.tie_origin.to_string())
                .with_attribute("destination", &effects.tie_destination.to_string()),
        );
    }
    if let Some(left) = finger("LeftFingering", effects.left_hand_finger) {
        element.push(left);
    }
    if let Some(right) = finger("RightFingering", effects.right_hand_finger) {
        element.push(right);
    }
    if let NoteKind::Percussion { articulation } = note.kind {
        element.push(Element::leaf(
            "InstrumentArticulation",
            &articulation.to_string(),
        ));
    }
    let generated = properties(note, inputs, key, &items, staff.tuning.len())?;
    if !generated.is_empty() {
        let mut container = Element::new(PROPERTIES);
        for property in generated {
            container.push(property);
        }
        element.push(container);
    }
    apply_own(&mut element, items)?;
    Ok(element)
}
