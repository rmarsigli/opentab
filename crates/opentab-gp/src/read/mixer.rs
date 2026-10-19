use opentab_model::{
    Automation, AutomationValue, Balance, Level, MasterBar, MidiBank, MidiChannel, Sound, SoundRef,
};

use crate::canon::{self, AutomationParts};
use crate::codes::{
    GENERATED_ROLE, PAN_AUTOMATION, SOUND_AUTOMATION, TEMPO_POSITION_GRID, VOLUME_AUTOMATION,
};
use crate::dom::{Element, Node};
use crate::guard;
use crate::number::{self, Decimal};
use crate::read::context::{Context, Fault, Handled, Scope, Visit};

const AUTOMATION: &str = "Automation";

pub(crate) enum Lane<'k> {
    Strip,
    Track(&'k [String]),
}

#[derive(Default)]
pub(crate) struct MidiFields {
    pub port: Option<u8>,
    pub primary: Option<MidiChannel>,
    pub secondary: Option<MidiChannel>,
    pub per_string: Option<bool>,
}

pub(crate) struct Strip {
    pub volume: Level,
    pub pan: Balance,
    pub automations: Vec<Automation>,
}

impl Default for Strip {
    fn default() -> Self {
        Strip {
            volume: Level::HALF,
            pan: Balance::CENTRE,
            automations: Vec::new(),
        }
    }
}

#[derive(Default)]
pub(crate) struct Sounds {
    pub program: u8,
    pub bank: MidiBank,
    pub sounds: Vec<Sound>,
}

#[derive(Default)]
struct Bare {
    program: Option<u8>,
    msb: Option<u8>,
    lsb: Option<u8>,
}

fn bare(sounds: &Element) -> Option<(u8, MidiBank)> {
    if !sounds.attributes.is_empty() {
        return None;
    }
    let [Node::Element(sound)] = sounds.children.as_slice() else {
        return None;
    };
    if sound.name != "Sound" || !sound.attributes.is_empty() {
        return None;
    }
    let [Node::Element(midi)] = sound.children.as_slice() else {
        return None;
    };
    if midi.name != "MIDI" || !midi.attributes.is_empty() {
        return None;
    }
    let mut found = Bare::default();
    for node in &midi.children {
        let Node::Element(field) = node else {
            return None;
        };
        if !field.attributes.is_empty() || field.has_elements() {
            return None;
        }
        let slot = match field.name.as_str() {
            "Program" => &mut found.program,
            "MSB" => &mut found.msb,
            "LSB" => &mut found.lsb,
            _ => return None,
        };
        if slot.is_some() {
            return None;
        }
        *slot = Some(number::integer::<u8>(&field.text())?);
    }
    let bank = MidiBank::new(found.msb.unwrap_or(0), found.lsb.unwrap_or(0)).ok()?;
    Some((found.program?, bank))
}

pub(crate) fn sound_keys(track: &Element) -> Vec<String> {
    let Some((_, sounds)) = track.child("Sounds") else {
        return Vec::new();
    };
    if bare(sounds).is_some() {
        return Vec::new();
    }
    sounds
        .elements()
        .filter(|(_, sound)| sound.name == "Sound")
        .map(|(_, sound)| {
            let text = |name: &str| sound.child(name).map(|(_, child)| child.text());
            canon::sound_key(
                &text("Path").unwrap_or_default(),
                &text("Name").unwrap_or_default(),
                &text("Role").unwrap_or_else(|| GENERATED_ROLE.to_string()),
            )
        })
        .collect()
}

pub(crate) fn read_sounds(
    ctx: &mut Context,
    scope: &Scope,
    element: &Element,
) -> Result<Sounds, Fault> {
    if let Some((program, bank)) = bare(element) {
        return Ok(Sounds {
            program,
            bank,
            sounds: Vec::new(),
        });
    }
    let mut sounds = Vec::new();
    ctx.visit(scope, element, |ctx, visit| {
        if visit.key != "Sound" {
            return Ok(Handled::Unknown);
        }
        let index = sounds.len();
        let inner = Scope::new(
            scope.owner,
            scope.parent.nth("Sound", index),
            visit.location.clone(),
        );
        sounds.push(read_sound(ctx, &inner, visit.element, index)?);
        Ok(Handled::Each)
    })?;
    let (program, bank) = sounds.first().map_or((0, MidiBank::default()), |first| {
        (first.program, first.bank)
    });
    Ok(Sounds {
        program,
        bank,
        sounds,
    })
}

#[derive(Default)]
struct SoundMidi {
    program: u8,
    msb: u8,
    lsb: u8,
}

fn read_sound(
    ctx: &mut Context,
    scope: &Scope,
    element: &Element,
    index: usize,
) -> Result<Sound, Fault> {
    ctx.keep_attributes(scope, element, &[])?;
    let name = element
        .child("Name")
        .map(|(_, child)| child.text())
        .unwrap_or_default();
    let mut midi = SoundMidi::default();
    ctx.visit(scope, element, |ctx, visit| match visit.key.as_str() {
        "Name" => Ok(Handled::Mapped(&[])),
        "Label" if visit.text() == name => Ok(Handled::Mapped(&[])),
        "Label" => Ok(Handled::Replayed(guard::sound_label(index, Some(&name)))),
        "MIDI" => {
            let inner = scope.enter(visit);
            ctx.visit(&inner, visit.element, |_, field| {
                match field.key.as_str() {
                    "Program" => midi.program = field.integer()?,
                    "MSB" => midi.msb = field.integer()?,
                    "LSB" => midi.lsb = field.integer()?,
                    _ => return Ok(Handled::Unknown),
                }
                Ok(Handled::Mapped(&[]))
            })?;
            Ok(Handled::Mapped(&[]))
        }
        _ => Ok(Handled::Unknown),
    })?;
    let bank =
        MidiBank::new(midi.msb, midi.lsb).map_err(|error| Fault::model(&scope.location, error))?;
    Ok(Sound {
        name,
        program: midi.program,
        bank,
    })
}

fn channel(visit: &Visit<'_>) -> Result<MidiChannel, Fault> {
    MidiChannel::new(visit.integer()?).map_err(|error| Fault::model(&visit.location, error))
}

pub(crate) fn read_midi_connection(
    ctx: &mut Context,
    scope: &Scope,
    element: &Element,
) -> Result<MidiFields, Fault> {
    let mut fields = MidiFields::default();
    ctx.visit(scope, element, |_, visit| {
        match visit.key.as_str() {
            "Port" => fields.port = Some(visit.integer()?),
            "PrimaryChannel" => fields.primary = Some(channel(visit)?),
            "SecondaryChannel" => fields.secondary = Some(channel(visit)?),
            "ForeOneChannelPerString" => {
                let flag = number::boolean(&visit.text()).ok_or_else(|| visit.invalid())?;
                fields.per_string = Some(flag);
            }
            _ => return Ok(Handled::Unknown),
        }
        Ok(Handled::Mapped(&[]))
    })?;
    Ok(fields)
}

pub(crate) fn read_rse(
    ctx: &mut Context,
    scope: &Scope,
    element: &Element,
    strip: &mut Strip,
) -> Result<(), Fault> {
    let mut seen = false;
    ctx.visit(scope, element, |ctx, visit| {
        if visit.key != "ChannelStrip" || seen {
            return Ok(Handled::Anchored);
        }
        seen = true;
        read_strip(ctx, &scope.enter(visit), visit.element, strip)?;
        Ok(Handled::Mapped(&[]))
    })
}

fn read_strip(
    ctx: &mut Context,
    scope: &Scope,
    element: &Element,
    strip: &mut Strip,
) -> Result<(), Fault> {
    let mut parameters = false;
    let mut automations = false;
    ctx.visit(scope, element, |ctx, visit| match visit.key.as_str() {
        "Parameters" if !parameters => {
            parameters = true;
            let mixer = canon::mixer(&visit.text());
            strip.volume = mixer.volume;
            strip.pan = mixer.pan;
            if *visit.element == canon::strip_parameters(mixer.volume, mixer.pan) {
                Ok(Handled::Mapped(&[]))
            } else {
                Ok(Handled::Unknown)
            }
        }
        "Automations" if !automations => {
            automations = true;
            read_lane(
                ctx,
                &scope.enter(visit),
                visit.element,
                &Lane::Strip,
                &mut strip.automations,
            )?;
            Ok(Handled::Mapped(&[]))
        }
        _ => Ok(Handled::Anchored),
    })
}

pub(crate) fn read_lane(
    ctx: &mut Context,
    scope: &Scope,
    element: &Element,
    lane: &Lane<'_>,
    out: &mut Vec<Automation>,
) -> Result<(), Fault> {
    for (index, node) in element.children.iter().enumerate() {
        if let Node::Element(child) = node
            && let Some(automation) = typed(&ctx.score.master_bars, child, lane)
        {
            out.push(automation);
            continue;
        }
        ctx.keep_anchored(scope, index, node)?;
    }
    Ok(())
}

fn text_of(element: &Element, name: &str) -> Option<String> {
    element.child(name).map(|(_, child)| child.text())
}

fn typed(master_bars: &[MasterBar], element: &Element, lane: &Lane<'_>) -> Option<Automation> {
    if element.name != AUTOMATION || !element.attributes.is_empty() {
        return None;
    }
    let kind = text_of(element, "Type")?;
    let linear = number::boolean(&text_of(element, "Linear")?)?;
    let bar = number::integer::<usize>(&text_of(element, "Bar")?)?;
    let master = master_bars.get(bar)?;
    let time = master.time_signature;
    let position = Decimal::parse(&text_of(element, "Position")?)?.round_to_grid(
        i64::from(time.numerator()),
        i64::from(time.denominator()),
        TEMPO_POSITION_GRID,
    )?;
    let visible = number::boolean(&text_of(element, "Visible")?)?;
    let raw = text_of(element, "Value")?;
    let (value, value_element) = match (lane, kind.as_str()) {
        (Lane::Strip, VOLUME_AUTOMATION) => {
            let level = Level::new(number::millionths(&raw)?).ok()?;
            (
                AutomationValue::Volume(level),
                Element::leaf("Value", &canon::millionths(level.millionths())),
            )
        }
        (Lane::Strip, PAN_AUTOMATION) => {
            let balance = Balance::new(number::millionths(&raw)?).ok()?;
            (
                AutomationValue::Pan(balance),
                Element::leaf("Value", &canon::millionths(balance.millionths())),
            )
        }
        (Lane::Track(keys), SOUND_AUTOMATION) => {
            let mut matches = keys
                .iter()
                .enumerate()
                .filter(|(_, key)| **key == raw)
                .map(|(index, _)| index);
            let index = matches.next()?;
            if matches.next().is_some() {
                return None;
            }
            (
                AutomationValue::Sound(SoundRef(index)),
                canon::sound_value(&raw),
            )
        }
        _ => return None,
    };
    let generated = canon::automation(AutomationParts {
        kind: &kind,
        linear,
        bar,
        position: canon::automation_position(position, time)?,
        visible,
        value: value_element,
    });
    (generated == *element).then_some(Automation {
        master_bar: master.id,
        position,
        linear,
        visible,
        value,
    })
}
