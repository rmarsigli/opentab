use opentab_model::{
    Automation, AutomationValue, Balance, Level, MasterBar, Track, TrackMidi, automations_in_order,
    check_program,
};

use crate::canon::{self, AutomationParts};
use crate::codes::{
    GENERATED_ROLE, PAN_AUTOMATION, PAN_PARAMETER, SOUND_AUTOMATION, STRIP_DEFAULTS,
    VOLUME_AUTOMATION, VOLUME_PARAMETER,
};
use crate::dom::{Element, Node};
use crate::error::Refusal;
use crate::read::same_connection;
use crate::write::payload::Item;
use crate::write::xml::{self, Fail, Within};

const RSE: &str = "RSE";
const CHANNEL_STRIP: &str = "RSE/ChannelStrip";
const MIDI_CONNECTION: &str = "MidiConnection";

fn under(items: &[Item], root: &str) -> bool {
    items.iter().any(|item| {
        item.parent == root
            || item
                .parent
                .strip_prefix(root)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

fn kept_text(items: &[Item], parent: &str, name: &str) -> Option<String> {
    items
        .iter()
        .filter(|item| item.parent == parent)
        .filter_map(Item::element)
        .find(|element| element.name == name)
        .map(Element::text)
}

pub(crate) fn check_track(track: &Track) -> Result<(), Refusal> {
    let instrument = &track.instrument;
    if instrument
        .set
        .is_some_and(|set| set.kind() != instrument.kind)
    {
        return Err(Refusal::InstrumentSet);
    }
    let programs = std::iter::once(instrument.midi_program)
        .chain(track.sounds.iter().map(|sound| sound.program));
    for program in programs {
        check_program(program).map_err(|_| Refusal::Program { program })?;
    }
    if let Some(first) = track.sounds.first()
        && (first.program != instrument.midi_program || first.bank != track.midi.bank)
    {
        return Err(Refusal::FirstSound);
    }
    if !automations_in_order(&track.automations) {
        return Err(Refusal::AutomationOrder);
    }
    Ok(())
}

pub(crate) fn sounds_element(track: &Track) -> Result<Element, Fail> {
    let mut sounds = Element::new("Sounds");
    if track.sounds.is_empty() {
        return Ok(canon::bare_sounds(
            track.instrument.midi_program,
            track.midi.bank,
        ));
    }
    for (index, sound) in track.sounds.iter().enumerate() {
        let path = || format!("sounds[{index}].name");
        let element = Element::new("Sound")
            .with_child(xml::text("Name", &sound.name).within(path)?)
            .with_child(xml::text("Label", &sound.name).within(path)?)
            .with_child(canon::sound_midi(sound.program, sound.bank, true));
        sounds.push(element);
    }
    Ok(sounds)
}

pub(crate) fn midi_connection(track: &Track, items: &[Item]) -> Option<Element> {
    let midi = &track.midi;
    let same = same_connection(midi, &TrackMidi::default_for(track.instrument.kind));
    if same && !under(items, MIDI_CONNECTION) {
        return None;
    }
    Some(
        Element::new(MIDI_CONNECTION)
            .with_child(Element::leaf("Port", &midi.port.to_string()))
            .with_child(Element::leaf(
                "PrimaryChannel",
                &midi.primary.get().to_string(),
            ))
            .with_child(Element::leaf(
                "SecondaryChannel",
                &midi.secondary.get().to_string(),
            ))
            .with_child(Element::leaf(
                "ForeOneChannelPerString",
                &midi.per_string.to_string(),
            )),
    )
}

fn sound_keys(track: &Track, items: &[Item]) -> Vec<String> {
    track
        .sounds
        .iter()
        .enumerate()
        .map(|(index, sound)| {
            let parent = format!("Sounds/Sound#{index}");
            canon::sound_key(
                &kept_text(items, &parent, "Path").unwrap_or_default(),
                &sound.name,
                &kept_text(items, &parent, "Role").unwrap_or_else(|| GENERATED_ROLE.to_string()),
            )
        })
        .collect()
}

struct Site<'s> {
    master_bars: &'s [MasterBar],
    keys: &'s [String],
}

fn automation_element(site: &Site<'_>, automation: &Automation) -> Result<Element, Refusal> {
    let bar = site
        .master_bars
        .iter()
        .position(|master| master.id == automation.master_bar)
        .ok_or(Refusal::AutomationBar {
            master_bar: automation.master_bar.0.0,
        })?;
    let time = site
        .master_bars
        .get(bar)
        .map(|master| master.time_signature)
        .ok_or(Refusal::AutomationBar {
            master_bar: automation.master_bar.0.0,
        })?;
    let (kind, value) = match automation.value {
        AutomationValue::Volume(level) => (
            VOLUME_AUTOMATION,
            Element::leaf("Value", &canon::millionths(level.millionths())),
        ),
        AutomationValue::Pan(balance) => (
            PAN_AUTOMATION,
            Element::leaf("Value", &canon::millionths(balance.millionths())),
        ),
        AutomationValue::Sound(sound) => {
            let key = site
                .keys
                .get(sound.0)
                .ok_or(Refusal::SoundRef { index: sound.0 })?;
            if site.keys.iter().filter(|other| *other == key).count() > 1 {
                return Err(Refusal::AmbiguousSound { index: sound.0 });
            }
            xml::check_text(key)?;
            (SOUND_AUTOMATION, canon::sound_value(key))
        }
    };
    let position = canon::automation_position(
        automation.position,
        time,
        canon::position_unit(canon::kind_of(&automation.value)),
    )
    .ok_or(Refusal::AutomationPosition {
        numer: *automation.position.numer(),
        denom: *automation.position.denom(),
    })?;
    Ok(canon::automation(AutomationParts {
        kind,
        linear: automation.linear,
        bar,
        position,
        visible: automation.visible,
        value,
    }))
}

fn lane(site: &Site<'_>, track: &Track, sound: bool) -> Result<Option<Element>, Fail> {
    let mut element = Element::new("Automations");
    for (index, automation) in track.automations.iter().enumerate() {
        if matches!(automation.value, AutomationValue::Sound(_)) != sound {
            continue;
        }
        let child = automation_element(site, automation)
            .map_err(Fail::new)
            .within(|| format!("automations[{index}]"))?;
        element.push(child);
    }
    Ok(element.has_elements().then_some(element))
}

pub(crate) struct Lanes {
    pub strip: Option<Element>,
    pub sounds: Option<Element>,
}

pub(crate) fn lanes(
    track: &Track,
    master_bars: &[MasterBar],
    items: &[Item],
) -> Result<Lanes, Fail> {
    let keys = sound_keys(track, items);
    let site = Site {
        master_bars,
        keys: &keys,
    };
    Ok(Lanes {
        strip: lane(&site, track, false)?,
        sounds: lane(&site, track, true)?,
    })
}

fn default_mixer(midi: &TrackMidi) -> bool {
    midi.volume == Level::HALF && midi.pan == Balance::CENTRE
}

fn set_token(tokens: &mut Vec<String>, at: usize, value: u32) {
    for (index, token) in STRIP_DEFAULTS.iter().enumerate().skip(tokens.len()) {
        if index <= at {
            tokens.push(token.to_string());
        }
    }
    if let Some(slot) = tokens.get_mut(at) {
        *slot = canon::millionths(value);
    }
}

pub(crate) fn rse_element(
    track: &Track,
    strip: Option<Element>,
    items: &[Item],
) -> Option<Element> {
    let defaults = default_mixer(&track.midi);
    if defaults && strip.is_none() && !under(items, RSE) {
        return None;
    }
    let mut rse = Element::new(RSE);
    if !defaults || strip.is_some() || under(items, CHANNEL_STRIP) {
        let kept_parameters = items.iter().any(|item| {
            item.parent == CHANNEL_STRIP
                && item
                    .element()
                    .is_some_and(|element| element.name == "Parameters")
        });
        let mut channel = Element::new("ChannelStrip");
        if !defaults && !kept_parameters {
            channel.push(canon::strip_parameters(track.midi.volume, track.midi.pan));
        }
        if let Some(strip) = strip {
            channel.push(strip);
        }
        rse.push(channel);
    }
    Some(rse)
}

fn child_mut<'e>(element: &'e mut Element, name: &str) -> Option<&'e mut Element> {
    element.children.iter_mut().find_map(|node| match node {
        Node::Element(child) if child.name == name => Some(child),
        _ => None,
    })
}

pub(crate) fn patch_parameters(track: &mut Element, midi: &TrackMidi) {
    let Some(parameters) = child_mut(track, RSE)
        .and_then(|rse| child_mut(rse, "ChannelStrip"))
        .and_then(|strip| child_mut(strip, "Parameters"))
    else {
        return;
    };
    let text = parameters.text();
    let current = canon::mixer(&text);
    if current.volume == midi.volume && current.pan == midi.pan {
        return;
    }
    let mut tokens: Vec<String> = text.split_whitespace().map(str::to_string).collect();
    if current.volume != midi.volume {
        set_token(&mut tokens, VOLUME_PARAMETER, midi.volume.millionths());
    }
    if current.pan != midi.pan {
        set_token(&mut tokens, PAN_PARAMETER, midi.pan.millionths());
    }
    parameters.children = vec![Node::Text(tokens.join(" "))];
}
