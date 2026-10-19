use opentab_model::{
    Automation, DrumKit, Instrument, InstrumentKind, InstrumentSet, KeySignature, Pitch, Staff,
    StaffId, Track, TrackId, TrackMidi, Transpose, Tuning,
};

use crate::canon;
use crate::codes::{DRUM_ELEMENTS, INSTRUMENT_SETS, lookup};
use crate::dom::{Element, Node};
use crate::error::GpifProblem;
use crate::guard;
use crate::location::Location;
use crate::number;
use crate::opaque::ParentPath;
use crate::read::beat::read_bar;
use crate::read::context::{Context, Fault, Handled, Owner, Scope, Visit, missing_element};
use crate::read::drums::{KitRead, read_kit};
use crate::read::master::{MasterFrame, TrackOrder};
use crate::read::mixer::{
    Lane, MidiFields, Sounds, Strip, connection, read_lane, read_midi_connection, read_rse,
    read_sounds, sound_keys,
};
use crate::read::pools::Pools;

pub(crate) struct StaffFrame<'t> {
    pub kind: InstrumentKind,
    pub tuning: &'t [Pitch],
    pub capo: u8,
    pub track_tuning: &'t [Pitch],
    pub track_capo: u8,
    pub transpose: Transpose,
    pub chords: &'t [ChordItem],
    pub key: KeySignature,
}

pub(crate) struct ChordItem {
    pub id: String,
    pub name: String,
}

struct BarColumn<'f> {
    tokens: Vec<&'f str>,
    location: Location,
    key: KeySignature,
}

struct InstrumentFields {
    kind: InstrumentKind,
    set: Option<InstrumentSet>,
    drum_kit: Option<DrumKit>,
}

struct TrackFields<'e> {
    name: String,
    short_name: String,
    instrument: InstrumentFields,
    transpose: Transpose,
    sounds: Sounds,
    midi: MidiFields,
    strip: Strip,
    automations: Vec<Automation>,
    staves: Option<(&'e Element, Location)>,
}

struct Visited {
    sounds: bool,
    rse: bool,
    midi: bool,
    automations: bool,
}

struct StaffHead {
    tuning: Option<Vec<Pitch>>,
    capo: Option<u8>,
    chords: Vec<ChordItem>,
}

pub(crate) fn read_tracks(
    ctx: &mut Context,
    pools: &mut Pools<'_>,
    order: Option<TrackOrder>,
    frames: &[MasterFrame],
) -> Result<(), Fault> {
    let mut offsets = Vec::with_capacity(pools.tracks.items.len());
    let mut total = 0usize;
    for (element, _) in &pools.tracks.items {
        offsets.push(total);
        total = total.saturating_add(staff_count(element));
    }
    let mut columns = Vec::with_capacity(frames.len());
    for (index, frame) in frames.iter().enumerate() {
        let (text, location) = frame
            .bars
            .as_ref()
            .ok_or_else(|| Fault::at(&frame.location, missing_element("Bars")))?;
        let found = text.split_whitespace().count();
        if found != total {
            return Err(Fault::at(
                location,
                GpifProblem::StaffCountMismatch {
                    expected: total,
                    found,
                },
            ));
        }
        let key = ctx
            .score
            .master_bars
            .get(index)
            .map(|master| master.key_signature)
            .unwrap_or_default();
        columns.push(BarColumn {
            tokens: text.split_whitespace().collect(),
            location: location.clone(),
            key,
        });
    }
    let mut listed = vec![false; pools.tracks.items.len()];
    let mut sequence = Vec::with_capacity(listed.len());
    if let Some(order) = &order {
        for token in order.text.split_whitespace() {
            let index = pools.tracks.resolve(token, &order.location)?;
            match listed.get_mut(index) {
                Some(seen) if *seen => {
                    return Err(Fault::at(
                        &order.location,
                        GpifProblem::DuplicateTrackReference { id: index },
                    ));
                }
                Some(seen) => *seen = true,
                None => {}
            }
            sequence.push(index);
        }
    }
    for (index, seen) in listed.iter().enumerate() {
        if !seen {
            sequence.push(index);
        }
    }
    for index in sequence {
        let Some((element, track_location)) = pools.tracks.items.get(index).cloned() else {
            continue;
        };
        pools.tracks.mark(index);
        let offset = offsets.get(index).copied().unwrap_or_default();
        let track = read_track(ctx, pools, element, &track_location, offset, &columns)?;
        ctx.score.tracks.push(track);
    }
    Ok(())
}

fn staff_count(track: &Element) -> usize {
    track.child("Staves").map_or(0, |(_, staves)| {
        staves
            .elements()
            .filter(|(_, staff)| staff.name == "Staff")
            .count()
    })
}

fn read_track(
    ctx: &mut Context,
    pools: &mut Pools<'_>,
    element: &Element,
    location: &Location,
    offset: usize,
    columns: &[BarColumn<'_>],
) -> Result<Track, Fault> {
    let id: TrackId = ctx.alloc(location)?;
    let scope = Scope::new(Owner::Element(id.0), ParentPath::owner(), location.clone());
    ctx.keep_attributes(&scope, element, &["id"])?;
    let keys = sound_keys(element);
    let kind_hint = element
        .child("InstrumentSet")
        .and_then(|(_, set)| set.child("Type"))
        .and_then(|(_, kind)| lookup(INSTRUMENT_SETS, &kind.text()))
        .map_or(InstrumentKind::Fretted, InstrumentSet::kind);
    let mut fields = TrackFields {
        name: String::new(),
        short_name: String::new(),
        instrument: InstrumentFields {
            kind: InstrumentKind::Fretted,
            set: None,
            drum_kit: None,
        },
        transpose: Transpose::default(),
        sounds: Sounds::default(),
        midi: MidiFields::default(),
        strip: Strip::default(),
        automations: Vec::new(),
        staves: None,
    };
    let mut visited = Visited {
        sounds: false,
        rse: false,
        midi: false,
        automations: false,
    };
    ctx.visit(&scope, element, |ctx, visit| {
        match visit.key.as_str() {
            "Name" => fields.name = visit.text(),
            "ShortName" => fields.short_name = visit.text(),
            "InstrumentSet" => {
                fields.instrument = read_instrument_set(ctx, &scope.enter(visit), visit.element)?;
            }
            "Transpose" => fields.transpose = read_transpose(ctx, &scope.enter(visit), visit)?,
            "Sounds" if !visited.sounds => {
                visited.sounds = true;
                fields.sounds = read_sounds(ctx, &scope.enter(visit), visit.element)?;
            }
            "RSE" if !visited.rse => {
                visited.rse = true;
                read_rse(ctx, &scope.enter(visit), visit.element, &mut fields.strip)?;
            }
            "MidiConnection" if !visited.midi => {
                visited.midi = true;
                let read = read_midi_connection(ctx, &scope.enter(visit), visit, kind_hint)?;
                fields.midi = read.fields;
                return Ok(read.handled);
            }
            "Automations" if !visited.automations => {
                visited.automations = true;
                read_lane(
                    ctx,
                    &scope.enter(visit),
                    visit.element,
                    &Lane::Track(&keys),
                    &mut fields.automations,
                )?;
            }
            "Staves" => fields.staves = Some((visit.element, visit.location.clone())),
            _ => return Ok(Handled::Anchored),
        }
        Ok(Handled::Mapped(&[]))
    })?;
    let midi = TrackMidi {
        bank: fields.sounds.bank,
        volume: fields.strip.volume,
        pan: fields.strip.pan,
        ..connection(&fields.midi, fields.instrument.kind)
    };
    let mut automations = fields.strip.automations;
    automations.extend(fields.automations);
    let mut track = Track {
        id,
        name: fields.name,
        short_name: fields.short_name,
        instrument: Instrument {
            kind: fields.instrument.kind,
            midi_program: fields.sounds.program,
            set: fields.instrument.set,
        },
        drum_kit: fields.instrument.drum_kit,
        midi,
        sounds: fields.sounds.sounds,
        tuning: Tuning {
            strings: Vec::new(),
        },
        capo: 0,
        transpose: fields.transpose,
        staves: Vec::new(),
        automations,
    };
    if let Some((staves, staves_location)) = fields.staves {
        let staves_scope = Scope::new(
            Owner::Element(id.0),
            ParentPath::owner().child("Staves"),
            staves_location.clone(),
        );
        let mut ordinal = 0usize;
        for (index, node) in staves.children.iter().enumerate() {
            match node {
                Node::Element(staff) if staff.name == "Staff" => {
                    let staff_location = staves_location.child(index);
                    let global = offset.saturating_add(ordinal);
                    let staff = read_staff(
                        ctx,
                        pools,
                        &mut track,
                        staff,
                        &staff_location,
                        global,
                        columns,
                    )?;
                    track.staves.push(staff);
                    ordinal += 1;
                }
                _ => ctx.keep(&staves_scope, index, node, None)?,
            }
        }
    }
    Ok(track)
}

fn read_instrument_set(
    ctx: &mut Context,
    scope: &Scope,
    element: &Element,
) -> Result<InstrumentFields, Fault> {
    let hint = element
        .child("Type")
        .and_then(|(_, kind)| lookup(INSTRUMENT_SETS, &kind.text()))
        .map_or(InstrumentKind::Fretted, InstrumentSet::kind);
    let mut fields = InstrumentFields {
        kind: InstrumentKind::Fretted,
        set: None,
        drum_kit: None,
    };
    let mut elements_seen = false;
    ctx.visit(scope, element, |_, visit| match visit.key.as_str() {
        "Type" => {
            let set = lookup(INSTRUMENT_SETS, &visit.text());
            let kind = set.map_or(InstrumentKind::Fretted, InstrumentSet::kind);
            fields.set = set;
            fields.kind = kind;
            match set {
                Some(known) if *visit.element == canon::instrument_type(known) => {
                    Ok(Handled::Mapped(&[]))
                }
                _ => Ok(Handled::Replayed(guard::instrument(kind, set))),
            }
        }
        DRUM_ELEMENTS if !elements_seen => {
            elements_seen = true;
            let read = if hint == InstrumentKind::Percussion {
                read_kit(visit.element)
            } else {
                KitRead::Untyped
            };
            match read {
                KitRead::Canonical(kit) => {
                    fields.drum_kit = Some(kit);
                    Ok(Handled::Mapped(&[]))
                }
                KitRead::Lenient(kit) => {
                    let guard = guard::drum_kit(hint, Some(&kit));
                    fields.drum_kit = Some(kit);
                    Ok(Handled::Replayed(guard))
                }
                KitRead::Untyped => Ok(Handled::Replayed(guard::drum_kit(hint, None))),
            }
        }
        _ => Ok(Handled::Unknown),
    })?;
    Ok(fields)
}

fn read_transpose(
    ctx: &mut Context,
    scope: &Scope,
    transpose: &Visit<'_>,
) -> Result<Transpose, Fault> {
    let mut chromatic = 0i8;
    let mut octave = 0i8;
    ctx.visit(scope, transpose.element, |_, visit| {
        match visit.key.as_str() {
            "Chromatic" => chromatic = visit.integer()?,
            "Octave" => octave = visit.integer()?,
            _ => return Ok(Handled::Unknown),
        }
        Ok(Handled::Mapped(&[]))
    })?;
    Transpose::new(chromatic, octave).map_err(|error| Fault::model(&transpose.location, error))
}

fn read_staff(
    ctx: &mut Context,
    pools: &mut Pools<'_>,
    track: &mut Track,
    element: &Element,
    location: &Location,
    global: usize,
    columns: &[BarColumn<'_>],
) -> Result<Staff, Fault> {
    let id: StaffId = ctx.alloc(location)?;
    let scope = Scope::new(Owner::Element(id.0), ParentPath::owner(), location.clone());
    ctx.keep_attributes(&scope, element, &[])?;
    let first = track.staves.is_empty();
    let mut head = StaffHead {
        tuning: None,
        capo: None,
        chords: Vec::new(),
    };
    ctx.visit(&scope, element, |ctx, visit| match visit.key.as_str() {
        "Properties" => {
            let track_head = TrackHead {
                first,
                tuning: &track.tuning.strings,
                capo: track.capo,
            };
            read_staff_properties(
                ctx,
                &scope.enter(visit),
                visit.element,
                &track_head,
                &mut head,
            )?;
            Ok(Handled::Mapped(&[]))
        }
        _ => Ok(Handled::Unknown),
    })?;
    let own_tuning = if first {
        if let Some(tuning) = head.tuning.take() {
            track.tuning.strings = tuning;
        }
        if let Some(capo) = head.capo {
            track.capo = capo;
        }
        None
    } else {
        head.tuning.take()
    };
    let mut frame = StaffFrame {
        kind: track.instrument.kind,
        tuning: own_tuning.as_deref().unwrap_or(&track.tuning.strings),
        capo: head.capo.unwrap_or(track.capo),
        track_tuning: &track.tuning.strings,
        track_capo: track.capo,
        transpose: track.transpose,
        chords: &head.chords,
        key: KeySignature::default(),
    };
    let mut bars = Vec::with_capacity(columns.len());
    for column in columns {
        frame.key = column.key;
        let token = column.tokens.get(global).copied().unwrap_or_default();
        let bar = read_bar(ctx, pools, &frame, token, &column.location, bars.last())?;
        bars.push(bar);
    }
    Ok(Staff { id, bars })
}

struct TrackHead<'t> {
    first: bool,
    tuning: &'t [Pitch],
    capo: u8,
}

fn read_staff_properties(
    ctx: &mut Context,
    scope: &Scope,
    element: &Element,
    track: &TrackHead<'_>,
    head: &mut StaffHead,
) -> Result<(), Fault> {
    let first = track.first;
    let strings = element
        .elements()
        .find(|(_, child)| child.key() == "Property@Tuning")
        .and_then(|(_, property)| property.child("Pitches"))
        .map_or(track.tuning.len(), |(_, pitches)| {
            pitches.text().split_whitespace().count()
        });
    ctx.visit(scope, element, |ctx, visit| match visit.key.as_str() {
        "Property@Tuning" => {
            if !first && *visit.element == canon::tuning_property(track.tuning) {
                return Ok(Handled::Mapped(&["name"]));
            }
            if !first {
                head.tuning = own_value(visit, "Pitches", read_pitches)?;
                return Ok(Handled::Unknown);
            }
            let inner = scope.enter(visit);
            ctx.visit(&inner, visit.element, |_, field| match field.key.as_str() {
                "Pitches" => {
                    head.tuning = Some(read_pitches(field.element, &field.location)?);
                    Ok(Handled::Mapped(&[]))
                }
                _ => Ok(Handled::Unknown),
            })?;
            Ok(Handled::Mapped(&["name"]))
        }
        "Property@CapoFret" => {
            if !first && *visit.element == canon::capo_property(track.capo) {
                return Ok(Handled::Mapped(&["name"]));
            }
            if !first {
                head.capo = own_value(visit, "Fret", read_fret)?;
                return Ok(Handled::Unknown);
            }
            let inner = scope.enter(visit);
            ctx.visit(&inner, visit.element, |_, field| match field.key.as_str() {
                "Fret" => {
                    head.capo = Some(read_fret(field.element, &field.location)?);
                    Ok(Handled::Mapped(&[]))
                }
                _ => Ok(Handled::Unknown),
            })?;
            Ok(Handled::Mapped(&["name"]))
        }
        "Property@DiagramCollection" => {
            head.chords = chord_items(visit.element);
            Ok(canon::strip_generated_items(visit.element, strings))
        }
        _ => Ok(Handled::Unknown),
    })
}

fn own_value<T>(
    visit: &Visit<'_>,
    name: &str,
    read: fn(&Element, &Location) -> Result<T, Fault>,
) -> Result<Option<T>, Fault> {
    match visit.element.child(name) {
        Some((index, child)) => read(child, &visit.location.child(index)).map(Some),
        None => Ok(None),
    }
}

fn invalid_number(location: &Location, text: &str) -> Fault {
    Fault::at(
        location,
        GpifProblem::InvalidNumber {
            text: text.trim().to_string(),
        },
    )
}

fn read_fret(element: &Element, location: &Location) -> Result<u8, Fault> {
    let text = element.text();
    number::integer(&text).ok_or_else(|| invalid_number(location, &text))
}

fn read_pitches(element: &Element, location: &Location) -> Result<Vec<Pitch>, Fault> {
    let text = element.text();
    let mut pitches = Vec::new();
    for token in text.split_whitespace() {
        let pitch = number::integer::<u8>(token).ok_or_else(|| invalid_number(location, token))?;
        pitches.push(Pitch(pitch));
    }
    pitches.reverse();
    Ok(pitches)
}

pub(crate) fn chord_items(collection: &Element) -> Vec<ChordItem> {
    let Some((_, items)) = collection.child("Items") else {
        return Vec::new();
    };
    items
        .elements()
        .filter(|(_, item)| item.name == "Item")
        .filter_map(|(_, item)| {
            Some(ChordItem {
                id: item.attribute("id")?.to_string(),
                name: item.attribute("name")?.to_string(),
            })
        })
        .collect()
}
