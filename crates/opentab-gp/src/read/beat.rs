use opentab_model::{
    Bar, BarId, Beat, BeatEffects, BeatId, BendPoint, Clef, Duration, NoteValue, Stroke,
    StrokeTicks, TremoloBar, TremoloPicking, Tuplet, Voice, VoiceId, VoiceSlot,
};

use crate::canon;
use crate::codes::{
    BRUSH_DURATION, BRUSH_START_TIME, CLEFS, DYNAMICS, FADE_IN, GRACES, NOTE_VALUES,
    STROKE_DIRECTIONS, TREMOLOS, WHAMMY_ATTRIBUTES, lookup,
};
use crate::dom::Element;
use crate::error::GpifProblem;
use crate::guard;
use crate::location::Location;
use crate::number::{self, Decimal};
use crate::opaque::ParentPath;
use crate::read::context::{Context, Fault, Handled, Owner, Scope, Visit, missing_element};
use crate::read::note::read_note;
use crate::read::pools::Pools;
use crate::read::property::{find, is_flag, single_text};
use crate::read::track::StaffFrame;

const EMPTY_SLOT: &str = "-1";
const MAX_WHAMMY_OFFSET: i64 = 100;
const WHAMMY_VALUE_FACTOR: i128 = 2;

enum BrushState {
    Absent,
    Mapped(Stroke),
    Unmappable,
}

impl BrushState {
    fn stroke(&self) -> Option<Stroke> {
        match self {
            BrushState::Mapped(stroke) => Some(*stroke),
            _ => None,
        }
    }

    fn handled(&self, known: &'static [&'static str]) -> Handled {
        match self {
            BrushState::Mapped(_) => Handled::Mapped(known),
            _ => Handled::Replayed(guard::stroke(None)),
        }
    }
}

struct Reference {
    text: String,
    location: Location,
}

pub(crate) fn read_bar(
    ctx: &mut Context,
    pools: &mut Pools<'_>,
    staff: &StaffFrame<'_>,
    token: &str,
    at: &Location,
    previous_bar: Option<&Bar>,
) -> Result<Bar, Fault> {
    let (element, location) = pools.bars.fetch(token, at)?;
    let id: BarId = ctx.alloc(&location)?;
    let scope = Scope::new(Owner::Element(id.0), ParentPath::owner(), location.clone());
    ctx.keep_attributes(&scope, element, &["id"])?;
    let mut clef = Clef::Treble;
    let mut voices = None;
    ctx.visit(&scope, element, |_, visit| match visit.key.as_str() {
        "Clef" => match lookup(CLEFS, &visit.text()) {
            Some(found) => {
                clef = found;
                Ok(Handled::Mapped(&[]))
            }
            None => Ok(Handled::Replayed(guard::clef(clef))),
        },
        "Voices" => {
            voices = Some(Reference {
                text: visit.text(),
                location: visit.location.clone(),
            });
            Ok(Handled::Mapped(&[]))
        }
        _ => Ok(Handled::Unknown),
    })?;
    let mut bar = Bar {
        id,
        clef,
        voices: Vec::new(),
    };
    let Some(voices) = voices else {
        return Ok(bar);
    };
    for (slot, token) in voices.text.split_whitespace().enumerate() {
        if token == EMPTY_SLOT {
            continue;
        }
        let slot = u8::try_from(slot).unwrap_or(u8::MAX);
        let slot = VoiceSlot::new(slot).map_err(|error| Fault::model(&voices.location, error))?;
        let carried = guard::carried(previous_bar, slot);
        bar.voices.push(read_voice(
            ctx,
            pools,
            staff,
            token,
            &voices.location,
            slot,
            carried,
        )?);
    }
    Ok(bar)
}

fn read_voice(
    ctx: &mut Context,
    pools: &mut Pools<'_>,
    staff: &StaffFrame<'_>,
    token: &str,
    at: &Location,
    slot: VoiceSlot,
    carried: Option<&Beat>,
) -> Result<Voice, Fault> {
    let (element, location) = pools.voices.fetch(token, at)?;
    let id: VoiceId = ctx.alloc(&location)?;
    let scope = Scope::new(Owner::Element(id.0), ParentPath::owner(), location.clone());
    ctx.keep_attributes(&scope, element, &["id"])?;
    let mut beats = None;
    ctx.visit(&scope, element, |_, visit| match visit.key.as_str() {
        "Beats" => {
            beats = Some(Reference {
                text: visit.text(),
                location: visit.location.clone(),
            });
            Ok(Handled::Mapped(&[]))
        }
        _ => Ok(Handled::Unknown),
    })?;
    let mut voice = Voice {
        id,
        slot,
        beats: Vec::new(),
    };
    if let Some(beats) = beats {
        for token in beats.text.split_whitespace() {
            let previous = voice.beats.last().or(carried);
            let beat = read_beat(ctx, pools, staff, token, &beats.location, previous)?;
            voice.beats.push(beat);
        }
    }
    Ok(voice)
}

struct BeatFields {
    effects: BeatEffects,
    rhythm: Option<Reference>,
    notes: Option<Reference>,
}

fn read_beat(
    ctx: &mut Context,
    pools: &mut Pools<'_>,
    staff: &StaffFrame<'_>,
    token: &str,
    at: &Location,
    previous: Option<&Beat>,
) -> Result<Beat, Fault> {
    let (element, location) = pools.beats.fetch(token, at)?;
    let id: BeatId = ctx.alloc(&location)?;
    let scope = Scope::new(Owner::Element(id.0), ParentPath::owner(), location.clone());
    ctx.keep_attributes(&scope, element, &["id"])?;
    let brush = brush_state(element);
    let mut fields = BeatFields {
        effects: BeatEffects {
            stroke: brush.stroke(),
            ..BeatEffects::default()
        },
        rhythm: None,
        notes: None,
    };
    ctx.visit(&scope, element, |ctx, visit| {
        read_beat_child(ctx, &scope, visit, staff, &brush, &mut fields)
    })?;
    let rhythm = fields
        .rhythm
        .ok_or_else(|| Fault::at(&location, missing_element("Rhythm")))?;
    let duration = read_rhythm(ctx, pools, &scope, &rhythm)?;
    let mut beat = Beat {
        id,
        duration,
        notes: Vec::new(),
        effects: fields.effects,
    };
    if let Some(notes) = fields.notes {
        for token in notes.text.split_whitespace() {
            let slot = beat.notes.len();
            beat.notes.push(read_note(
                ctx,
                pools,
                staff,
                token,
                &notes.location,
                previous,
                slot,
            )?);
        }
    }
    Ok(beat)
}

fn read_beat_child(
    ctx: &mut Context,
    scope: &Scope,
    visit: &Visit<'_>,
    staff: &StaffFrame<'_>,
    brush: &BrushState,
    fields: &mut BeatFields,
) -> Result<Handled, Fault> {
    let effects = &mut fields.effects;
    match visit.key.as_str() {
        "GraceNotes" => match lookup(GRACES, &visit.text()) {
            Some(grace) => effects.grace = Some(grace),
            None => return Ok(Handled::Replayed(guard::grace(None))),
        },
        "Dynamic" => match lookup(DYNAMICS, &visit.text()) {
            Some(dynamic) => effects.dynamic = dynamic,
            None => return Ok(Handled::Replayed(guard::dynamic(effects.dynamic))),
        },
        "Rhythm" => {
            let reference = visit.element.attribute("ref").ok_or_else(|| {
                visit.fault(GpifProblem::MissingAttribute {
                    name: "ref".to_string(),
                })
            })?;
            fields.rhythm = Some(Reference {
                text: reference.to_string(),
                location: visit.location.clone(),
            });
            return Ok(Handled::Mapped(&["ref"]));
        }
        "Whammy" => {
            let bar = whammy(visit.element);
            effects.tremolo_bar = bar;
            return Ok(match bar {
                Some(bar) if canon::whammy(bar) == *visit.element => {
                    Handled::Mapped(&WHAMMY_ATTRIBUTES)
                }
                _ => Handled::Replayed(guard::tremolo_bar(bar)),
            });
        }
        "Tremolo" => match lookup(TREMOLOS, &visit.text()).map(TremoloPicking::new) {
            Some(Ok(tremolo)) => effects.tremolo_picking = Some(tremolo),
            _ => return Ok(Handled::Replayed(guard::tremolo(None))),
        },
        "Fadding" => {
            if visit.text().trim() != FADE_IN {
                return Ok(Handled::Replayed(guard::fade_in(false)));
            }
            effects.fade_in = true;
        }
        "FreeText" => {
            let text = visit.text();
            ctx.charge(text.len(), &visit.location)?;
            effects.text = Some(text);
        }
        "Chord" => {
            let id = visit.text();
            let name = staff
                .chords
                .iter()
                .find(|item| item.id == id.trim())
                .map(|item| item.name.clone());
            let canonical = name.as_ref().and_then(|name| {
                staff
                    .chords
                    .iter()
                    .find(|item| item.name == *name)
                    .map(|item| canon::chord(&item.id))
            });
            let handled = if canonical.as_ref() == Some(visit.element) {
                Handled::Mapped(&[])
            } else {
                Handled::Replayed(guard::chord(name.as_deref()))
            };
            if let Some(name) = &name {
                ctx.charge(name.len(), &visit.location)?;
            }
            effects.chord_name = name;
            return Ok(handled);
        }
        "Notes" => {
            fields.notes = Some(Reference {
                text: visit.text(),
                location: visit.location.clone(),
            });
        }
        "Properties" => {
            let inner = scope.enter(visit);
            ctx.visit(&inner, visit.element, |_, property| {
                Ok(beat_property(property, brush, effects))
            })?;
        }
        "XProperties" => {
            let inner = scope.enter(visit);
            ctx.visit(&inner, visit.element, |_, property| {
                Ok(match property.key.strip_prefix("XProperty@") {
                    Some(BRUSH_DURATION) => brush.handled(&["id"]),
                    Some(BRUSH_START_TIME) => Handled::Replayed(guard::stroke(brush.stroke())),
                    _ => Handled::Unknown,
                })
            })?;
        }
        _ => return Ok(Handled::Unknown),
    }
    Ok(Handled::Mapped(&[]))
}

fn whammy_point(element: &Element, offset: &str, value: &str) -> Option<BendPoint> {
    let offset = Decimal::parse(element.attribute(offset)?)?
        .to_integer()
        .filter(|offset| (0..=MAX_WHAMMY_OFFSET).contains(offset))?;
    let value = Decimal::parse(element.attribute(value)?)?.scaled_integer(WHAMMY_VALUE_FACTOR)?;
    BendPoint::new(u8::try_from(offset).ok()?, i16::try_from(value).ok()?).ok()
}

fn whammy(element: &Element) -> Option<TremoloBar> {
    TremoloBar::new(
        whammy_point(element, "originOffset", "originValue")?,
        whammy_point(element, "middleOffset1", "middleValue")?,
        whammy_point(element, "middleOffset2", "middleValue")?,
        whammy_point(element, "destinationOffset", "destinationValue")?,
    )
    .ok()
}

fn beat_property(property: &Visit<'_>, brush: &BrushState, effects: &mut BeatEffects) -> Handled {
    let (flag, name) = match property.key.as_str() {
        "Property@Brush" => return brush.handled(&["name"]),
        "Property@Slapped" => (&mut effects.slap, "Slapped"),
        "Property@Popped" => (&mut effects.pop, "Popped"),
        _ => return Handled::Unknown,
    };
    if is_flag(property.element) {
        *flag = true;
        Handled::Mapped(&["name"])
    } else {
        Handled::Replayed(guard::flag(name, false))
    }
}

fn brush_state(beat: &Element) -> BrushState {
    let direction = find(beat, "Properties", "Property@Brush");
    let ticks = find(beat, "XProperties", &format!("XProperty@{BRUSH_DURATION}"));
    let Some(direction) = direction else {
        return match ticks {
            Some(_) => BrushState::Unmappable,
            None => BrushState::Absent,
        };
    };
    let Some(direction) =
        single_text(direction, "Direction").and_then(|text| lookup(STROKE_DIRECTIONS, &text))
    else {
        return BrushState::Unmappable;
    };
    let duration = match ticks {
        None => StrokeTicks::default(),
        Some(ticks) => {
            let parsed = single_text(ticks, "Int")
                .and_then(|text| number::integer::<u16>(&text))
                .map(StrokeTicks::new);
            match parsed {
                Some(Ok(duration)) => duration,
                _ => return BrushState::Unmappable,
            }
        }
    };
    BrushState::Mapped(Stroke {
        direction,
        duration,
    })
}

struct RhythmFields {
    value: Option<NoteValue>,
    dots: u8,
    tuplet: Option<Tuplet>,
}

fn read_rhythm(
    ctx: &mut Context,
    pools: &mut Pools<'_>,
    beat: &Scope,
    reference: &Reference,
) -> Result<Duration, Fault> {
    let (element, location) = pools.rhythms.fetch(&reference.text, &reference.location)?;
    let scope = Scope::new(beat.owner, ParentPath::rhythm(), location.clone());
    ctx.keep_attributes(&scope, element, &["id"])?;
    let mut fields = RhythmFields {
        value: None,
        dots: 0,
        tuplet: None,
    };
    ctx.visit(&scope, element, |_, visit| match visit.key.as_str() {
        "NoteValue" => {
            fields.value = Some(lookup(NOTE_VALUES, &visit.text()).ok_or_else(|| visit.invalid())?);
            Ok(Handled::Mapped(&[]))
        }
        "AugmentationDot" => {
            fields.dots = attribute_integer(visit, "count")?;
            Ok(Handled::Mapped(&["count"]))
        }
        "PrimaryTuplet" => {
            let num = attribute_integer(visit, "num")?;
            let den = attribute_integer(visit, "den")?;
            fields.tuplet =
                Some(Tuplet::new(num, den).map_err(|error| Fault::model(&visit.location, error))?);
            Ok(Handled::Mapped(&["num", "den"]))
        }
        _ => Ok(Handled::Unknown),
    })?;
    let value = fields
        .value
        .ok_or_else(|| Fault::at(&location, missing_element("NoteValue")))?;
    Duration::new(value, fields.dots, fields.tuplet).map_err(|error| Fault::model(&location, error))
}

fn attribute_integer<T: std::str::FromStr>(visit: &Visit<'_>, name: &str) -> Result<T, Fault> {
    let text = visit.element.attribute(name).ok_or_else(|| {
        visit.fault(GpifProblem::MissingAttribute {
            name: name.to_string(),
        })
    })?;
    number::integer(text).ok_or_else(|| {
        visit.fault(GpifProblem::InvalidNumber {
            text: text.to_string(),
        })
    })
}
