use opentab_model::{
    Fraction, KeySignature, Marker, MasterBar, MasterBarId, Tempo, TempoChange, TempoUnit,
    TimeSignature,
};

use crate::codes::{MINOR, TEMPO_POSITION_GRID, TEMPO_UNITS};
use crate::dom::{Element, Node};
use crate::error::GpifProblem;
use crate::guard;
use crate::location::Location;
use crate::number::{self, Decimal};
use crate::opaque::ParentPath;
use crate::read::context::{Context, Fault, Handled, Owner, Scope, Visit, missing_element};

const MAX_ALTERNATE_ENDING: u8 = 8;

pub(crate) struct MasterFrame {
    pub bars: Option<(String, Location)>,
    pub location: Location,
}

pub(crate) struct TrackOrder {
    pub text: String,
    pub location: Location,
}

pub(crate) fn read_master_bars(
    ctx: &mut Context,
    element: &Element,
    location: &Location,
) -> Result<Vec<MasterFrame>, Fault> {
    let scope = Scope::new(
        Owner::Document,
        ParentPath::owner().child(&element.name),
        location.clone(),
    );
    let mut frames = Vec::new();
    for (index, node) in element.children.iter().enumerate() {
        match node {
            Node::Element(child) if child.name == "MasterBar" => {
                frames.push(read_master_bar(ctx, child, &location.child(index))?);
            }
            _ => ctx.keep(&scope, index, node, None)?,
        }
    }
    Ok(frames)
}

#[derive(Default)]
struct Repeat {
    start: bool,
    end: Option<u8>,
}

fn read_master_bar(
    ctx: &mut Context,
    element: &Element,
    location: &Location,
) -> Result<MasterFrame, Fault> {
    let id: MasterBarId = ctx.alloc(location)?;
    let scope = Scope::new(Owner::Element(id.0), ParentPath::owner(), location.clone());
    ctx.keep_attributes(&scope, element, &[])?;
    let mut time = None;
    let mut key = KeySignature::default();
    let mut repeat = Repeat::default();
    let mut alternate_endings = 0u8;
    let mut double_bar = false;
    let mut marker = None;
    let mut bars = None;
    ctx.visit(&scope, element, |ctx, visit| {
        match visit.key.as_str() {
            "Key" => key = read_key(ctx, &scope.enter(visit), visit)?,
            "Time" => time = Some(read_time(visit)?),
            "Repeat" => return read_repeat(visit, &mut repeat),
            "Section" => marker = Some(read_section(ctx, &scope.enter(visit), visit.element)?),
            "AlternateEndings" => alternate_endings = read_alternate_endings(visit)?,
            "DoubleBar" => double_bar = true,
            "Bars" => bars = Some((visit.text(), visit.location.clone())),
            _ => return Ok(Handled::Unknown),
        }
        Ok(Handled::Mapped(&[]))
    })?;
    let time_signature = time.ok_or_else(|| Fault::at(location, missing_element("Time")))?;
    ctx.score.master_bars.push(MasterBar {
        id,
        time_signature,
        key_signature: key,
        repeat_start: repeat.start,
        repeat_end: repeat.end,
        alternate_endings,
        double_bar,
        marker,
        tempo_changes: Vec::new(),
    });
    Ok(MasterFrame {
        bars,
        location: location.clone(),
    })
}

fn read_time(visit: &Visit<'_>) -> Result<TimeSignature, Fault> {
    let text = visit.text();
    let (numerator, denominator) = text.trim().split_once('/').ok_or_else(|| visit.invalid())?;
    let numerator = number::integer::<u8>(numerator).ok_or_else(|| visit.invalid())?;
    let denominator = number::integer::<u8>(denominator).ok_or_else(|| visit.invalid())?;
    TimeSignature::new(numerator, denominator).map_err(|error| Fault::model(&visit.location, error))
}

fn read_key(ctx: &mut Context, scope: &Scope, key: &Visit<'_>) -> Result<KeySignature, Fault> {
    let mut accidentals = 0i8;
    let mut minor = false;
    ctx.visit(scope, key.element, |_, visit| match visit.key.as_str() {
        "AccidentalCount" => {
            accidentals = visit.integer()?;
            Ok(Handled::Mapped(&[]))
        }
        "Mode" => match visit.text().trim() {
            "Major" => Ok(Handled::Mapped(&[])),
            MINOR => {
                minor = true;
                Ok(Handled::Mapped(&[]))
            }
            _ => Ok(Handled::Replayed(guard::minor(false))),
        },
        _ => Ok(Handled::Unknown),
    })?;
    KeySignature::new(accidentals, minor).map_err(|error| Fault::model(&key.location, error))
}

fn read_repeat(visit: &Visit<'_>, repeat: &mut Repeat) -> Result<Handled, Fault> {
    let flag = |name: &str| -> Result<bool, Fault> {
        match visit.element.attribute(name) {
            Some(text) => number::boolean(text).ok_or_else(|| {
                visit.fault(GpifProblem::InvalidValue {
                    text: text.to_string(),
                })
            }),
            None => Ok(false),
        }
    };
    let start = flag("start")?;
    let end = flag("end")?;
    let count = match visit.element.attribute("count") {
        Some(text) => number::integer::<u8>(text).ok_or_else(|| {
            visit.fault(GpifProblem::InvalidNumber {
                text: text.to_string(),
            })
        })?,
        None => 0,
    };
    repeat.start = start;
    repeat.end = end.then_some(count);
    let complete = ["start", "end", "count"]
        .iter()
        .all(|name| visit.element.attribute(name).is_some());
    let canonical = complete
        && visit.element.attributes.len() == 3
        && visit.element.children.is_empty()
        && (start || end)
        && (end || count == 0);
    if canonical {
        Ok(Handled::Mapped(&["start", "end", "count"]))
    } else {
        Ok(Handled::Replayed(guard::repeat(repeat.start, repeat.end)))
    }
}

fn read_section(ctx: &mut Context, scope: &Scope, element: &Element) -> Result<Marker, Fault> {
    let mut marker = Marker {
        text: String::new(),
        letter: String::new(),
    };
    ctx.visit(scope, element, |_, visit| {
        match visit.key.as_str() {
            "Letter" => marker.letter = visit.text(),
            "Text" => marker.text = visit.text(),
            _ => return Ok(Handled::Unknown),
        }
        Ok(Handled::Mapped(&[]))
    })?;
    Ok(marker)
}

fn read_alternate_endings(visit: &Visit<'_>) -> Result<u8, Fault> {
    let mut mask = 0u8;
    for token in visit.text().split_whitespace() {
        let ending = number::integer::<u8>(token)
            .filter(|ending| (1..=MAX_ALTERNATE_ENDING).contains(ending))
            .ok_or_else(|| {
                visit.fault(GpifProblem::InvalidValue {
                    text: token.to_string(),
                })
            })?;
        mask |= 1 << (ending - 1);
    }
    Ok(mask)
}

pub(crate) fn read_master_track(
    ctx: &mut Context,
    element: &Element,
    location: &Location,
) -> Result<Option<TrackOrder>, Fault> {
    let scope = Scope::new(
        Owner::Document,
        ParentPath::owner().child(&element.name),
        location.clone(),
    );
    let mut order = None;
    ctx.visit(&scope, element, |ctx, visit| {
        match visit.key.as_str() {
            "Anacrusis" => ctx.score.anacrusis = true,
            "Tracks" => {
                order = Some(TrackOrder {
                    text: visit.text(),
                    location: visit.location.clone(),
                });
            }
            "Automations" => read_automations(ctx, &scope.enter(visit), visit.element)?,
            _ => return Ok(Handled::Unknown),
        }
        Ok(Handled::Mapped(&[]))
    })?;
    Ok(order)
}

fn read_automations(ctx: &mut Context, scope: &Scope, element: &Element) -> Result<(), Fault> {
    for (index, node) in element.children.iter().enumerate() {
        match node {
            Node::Element(child) if is_tempo(child) => {
                read_tempo(ctx, child, &scope.location.child(index))?;
            }
            _ => ctx.keep(scope, index, node, None)?,
        }
    }
    Ok(())
}

fn is_tempo(element: &Element) -> bool {
    element.name == "Automation"
        && element
            .child("Type")
            .is_some_and(|(_, kind)| kind.text().trim() == "Tempo")
}

fn tempo_position(
    automation: &Element,
    location: &Location,
    numerator: i64,
    denominator: i64,
) -> Result<Fraction, Fault> {
    let Some((index, element)) = automation.child("Position") else {
        return Ok(Fraction::from_integer(0));
    };
    let text = element.text();
    Decimal::parse(&text)
        .and_then(|decimal| decimal.round_to_grid(numerator, denominator, TEMPO_POSITION_GRID))
        .ok_or_else(|| {
            Fault::at(
                &location.child(index),
                GpifProblem::InvalidNumber {
                    text: text.trim().to_string(),
                },
            )
        })
}

fn read_tempo(ctx: &mut Context, element: &Element, location: &Location) -> Result<(), Fault> {
    let (bar_index, bar_element) = element
        .child("Bar")
        .ok_or_else(|| Fault::at(location, missing_element("Bar")))?;
    let bar_location = location.child(bar_index);
    let bar_text = bar_element.text();
    let bar = number::integer::<usize>(&bar_text).ok_or_else(|| {
        Fault::at(
            &bar_location,
            GpifProblem::InvalidNumber {
                text: bar_text.trim().to_string(),
            },
        )
    })?;
    let master_bars = ctx.score.master_bars.len();
    let Some(master) = ctx.score.master_bars.get(bar) else {
        return Err(Fault::at(
            &bar_location,
            GpifProblem::TempoBarOutOfRange { bar, master_bars },
        ));
    };
    let owner = Owner::Element(master.id.0);
    let numerator = i64::from(master.time_signature.numerator());
    let denominator = i64::from(master.time_signature.denominator());
    let ordinal = master.tempo_changes.len();
    let scope = Scope::new(owner, ParentPath::tempo(ordinal), location.clone());
    ctx.keep_attributes(&scope, element, &[])?;
    let position = tempo_position(element, location, numerator, denominator)?;
    let position_guard = guard::tempo_position(position);
    let mut value = None;
    ctx.visit(&scope, element, |_, visit| match visit.key.as_str() {
        "Type" | "Bar" => Ok(Handled::Mapped(&[])),
        "Value" => {
            value = Some(read_tempo_value(visit)?);
            Ok(Handled::Mapped(&[]))
        }
        _ => Ok(Handled::Replayed(position_guard.clone())),
    })?;
    let (bpm, unit) = value.ok_or_else(|| Fault::at(location, missing_element("Value")))?;
    if let Some(master) = ctx.score.master_bars.get_mut(bar) {
        master.tempo_changes.push(TempoChange {
            position,
            bpm,
            unit,
        });
    }
    Ok(())
}

fn read_tempo_value(visit: &Visit<'_>) -> Result<(Tempo, TempoUnit), Fault> {
    let text = visit.text();
    let mut tokens = text.split_whitespace();
    let bpm = tokens
        .next()
        .and_then(|token| token.parse::<f64>().ok())
        .ok_or_else(|| visit.invalid())?;
    let bpm = Tempo::new(bpm).map_err(|error| Fault::model(&visit.location, error))?;
    let unit = match tokens.next() {
        Some(token) => number::integer::<u8>(token)
            .and_then(|code| {
                TEMPO_UNITS
                    .iter()
                    .find(|(candidate, _)| *candidate == code)
                    .map(|(_, unit)| *unit)
            })
            .ok_or_else(|| visit.invalid())?,
        None => TempoUnit::Quarter,
    };
    if tokens.next().is_some() {
        return Err(visit.invalid());
    }
    Ok((bpm, unit))
}
