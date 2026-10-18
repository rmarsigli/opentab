use opentab_model::{
    Instrument, InstrumentKind, Pitch, Staff, StaffId, Track, TrackId, Transpose, Tuning,
};

use crate::codes::DRUM_KIT;
use crate::dom::{Element, Node};
use crate::error::GpifProblem;
use crate::guard;
use crate::location::Location;
use crate::number;
use crate::opaque::ParentPath;
use crate::read::beat::read_bar;
use crate::read::context::{Context, Fault, Handled, Owner, Scope, Visit, missing_element};
use crate::read::master::{MasterFrame, TrackOrder};
use crate::read::pools::Pools;

pub(crate) struct StaffFrame<'t> {
    pub kind: InstrumentKind,
    pub tuning: &'t [Pitch],
    pub capo: u8,
    pub transpose: Transpose,
    pub chords: &'t [ChordItem],
}

pub(crate) struct ChordItem {
    pub id: String,
    pub name: String,
}

struct BarColumn {
    tokens: Vec<String>,
    location: Location,
}

struct TrackFields<'e> {
    name: String,
    short_name: String,
    kind: InstrumentKind,
    midi_program: u8,
    transpose: Transpose,
    staves: Option<(&'e Element, Location)>,
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
    for frame in frames {
        let (text, location) = frame
            .bars
            .as_ref()
            .ok_or_else(|| Fault::at(&frame.location, missing_element("Bars")))?;
        let tokens: Vec<String> = text.split_whitespace().map(str::to_string).collect();
        if tokens.len() != total {
            return Err(Fault::at(
                location,
                GpifProblem::StaffCountMismatch {
                    expected: total,
                    found: tokens.len(),
                },
            ));
        }
        columns.push(BarColumn {
            tokens,
            location: location.clone(),
        });
    }
    let tokens: Vec<(String, Location)> = match &order {
        Some(order) => order
            .text
            .split_whitespace()
            .map(|token| (token.to_string(), order.location.clone()))
            .collect(),
        None => (0..pools.tracks.items.len())
            .map(|index| (index.to_string(), pools.tracks.location.clone()))
            .collect(),
    };
    let mut listed = Vec::with_capacity(tokens.len());
    for (token, location) in &tokens {
        let index = pools.tracks.resolve(token, location)?;
        if listed.contains(&index) {
            return Err(Fault::at(
                location,
                GpifProblem::DuplicateTrackReference { id: index },
            ));
        }
        listed.push(index);
        let (element, track_location) = pools.tracks.fetch(token, location)?;
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
    columns: &[BarColumn],
) -> Result<Track, Fault> {
    let id: TrackId = ctx.alloc(location)?;
    let scope = Scope::new(Owner::Element(id.0), ParentPath::owner(), location.clone());
    ctx.keep_attributes(&scope, element, &["id"]);
    let mut fields = TrackFields {
        name: String::new(),
        short_name: String::new(),
        kind: InstrumentKind::Fretted,
        midi_program: 0,
        transpose: Transpose::default(),
        staves: None,
    };
    ctx.visit(&scope, element, |ctx, visit| {
        match visit.key.as_str() {
            "Name" => fields.name = visit.text(),
            "ShortName" => fields.short_name = visit.text(),
            "InstrumentSet" => {
                fields.kind = read_instrument_set(ctx, &scope.enter(visit), visit.element)?;
            }
            "Transpose" => fields.transpose = read_transpose(ctx, &scope.enter(visit), visit)?,
            "Sounds" => {
                if let Some(program) = read_sounds(ctx, &scope.enter(visit), visit.element)? {
                    fields.midi_program = program;
                }
            }
            "Staves" => fields.staves = Some((visit.element, visit.location.clone())),
            _ => return Ok(Handled::Unknown),
        }
        Ok(Handled::Mapped(&[]))
    })?;
    let mut track = Track {
        id,
        name: fields.name,
        short_name: fields.short_name,
        instrument: Instrument {
            kind: fields.kind,
            midi_program: fields.midi_program,
        },
        tuning: Tuning {
            strings: Vec::new(),
        },
        capo: 0,
        transpose: fields.transpose,
        staves: Vec::new(),
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
                _ => ctx.keep(&staves_scope, index, node, None),
            }
        }
    }
    Ok(track)
}

fn read_instrument_set(
    ctx: &mut Context,
    scope: &Scope,
    element: &Element,
) -> Result<InstrumentKind, Fault> {
    let mut kind = InstrumentKind::Fretted;
    ctx.visit(scope, element, |_, visit| match visit.key.as_str() {
        "Type" => {
            if visit.text().trim() == DRUM_KIT {
                kind = InstrumentKind::Percussion;
            }
            Ok(Handled::Replayed(guard::instrument(kind)))
        }
        _ => Ok(Handled::Unknown),
    })?;
    Ok(kind)
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

fn read_sounds(ctx: &mut Context, scope: &Scope, element: &Element) -> Result<Option<u8>, Fault> {
    let mut program = None;
    let mut first = true;
    ctx.visit(scope, element, |ctx, visit| {
        if visit.key != "Sound" || !first {
            return Ok(Handled::Unknown);
        }
        first = false;
        let sound = Scope::new(
            scope.owner,
            scope.parent.nth("Sound", 0),
            visit.location.clone(),
        );
        ctx.keep_attributes(&sound, visit.element, &[]);
        ctx.visit(&sound, visit.element, |ctx, child| {
            match child.key.as_str() {
                "MIDI" => {
                    let midi = sound.enter(child);
                    ctx.visit(&midi, child.element, |_, field| match field.key.as_str() {
                        "Program" => {
                            program = Some(field.integer()?);
                            Ok(Handled::Mapped(&[]))
                        }
                        _ => Ok(Handled::Unknown),
                    })?;
                    Ok(Handled::Mapped(&[]))
                }
                _ => Ok(Handled::Unknown),
            }
        })?;
        Ok(Handled::Custom)
    })?;
    Ok(program)
}

fn read_staff(
    ctx: &mut Context,
    pools: &mut Pools<'_>,
    track: &mut Track,
    element: &Element,
    location: &Location,
    global: usize,
    columns: &[BarColumn],
) -> Result<Staff, Fault> {
    let id: StaffId = ctx.alloc(location)?;
    let scope = Scope::new(Owner::Element(id.0), ParentPath::owner(), location.clone());
    ctx.keep_attributes(&scope, element, &[]);
    let first = track.staves.is_empty();
    let mut head = StaffHead {
        tuning: None,
        capo: None,
        chords: Vec::new(),
    };
    ctx.visit(&scope, element, |ctx, visit| match visit.key.as_str() {
        "Properties" => {
            read_staff_properties(ctx, &scope.enter(visit), visit.element, first, &mut head)?;
            Ok(Handled::Mapped(&[]))
        }
        _ => Ok(Handled::Unknown),
    })?;
    if let Some(tuning) = head.tuning {
        track.tuning.strings = tuning;
    }
    if let Some(capo) = head.capo {
        track.capo = capo;
    }
    let frame = StaffFrame {
        kind: track.instrument.kind,
        tuning: &track.tuning.strings,
        capo: track.capo,
        transpose: track.transpose,
        chords: &head.chords,
    };
    let mut bars = Vec::with_capacity(columns.len());
    for column in columns {
        let token = column.tokens.get(global).map_or("", String::as_str);
        bars.push(read_bar(ctx, pools, &frame, token, &column.location)?);
    }
    Ok(Staff { id, bars })
}

fn read_staff_properties(
    ctx: &mut Context,
    scope: &Scope,
    element: &Element,
    first: bool,
    head: &mut StaffHead,
) -> Result<(), Fault> {
    ctx.visit(scope, element, |ctx, visit| match visit.key.as_str() {
        "Property@Tuning" if first => {
            let inner = scope.enter(visit);
            ctx.visit(&inner, visit.element, |_, field| match field.key.as_str() {
                "Pitches" => {
                    head.tuning = Some(read_pitches(field)?);
                    Ok(Handled::Mapped(&[]))
                }
                _ => Ok(Handled::Unknown),
            })?;
            Ok(Handled::Mapped(&["name"]))
        }
        "Property@CapoFret" if first => {
            let inner = scope.enter(visit);
            ctx.visit(&inner, visit.element, |_, field| match field.key.as_str() {
                "Fret" => {
                    head.capo = Some(field.integer()?);
                    Ok(Handled::Mapped(&[]))
                }
                _ => Ok(Handled::Unknown),
            })?;
            Ok(Handled::Mapped(&["name"]))
        }
        "Property@DiagramCollection" => {
            head.chords = chord_items(visit.element);
            Ok(Handled::Unknown)
        }
        _ => Ok(Handled::Unknown),
    })
}

fn read_pitches(visit: &Visit<'_>) -> Result<Vec<Pitch>, Fault> {
    let text = visit.text();
    let mut pitches = Vec::new();
    for token in text.split_whitespace() {
        let pitch = number::integer::<u8>(token).ok_or_else(|| {
            visit.fault(GpifProblem::InvalidNumber {
                text: token.to_string(),
            })
        })?;
        pitches.push(Pitch(pitch));
    }
    pitches.reverse();
    Ok(pitches)
}

fn chord_items(collection: &Element) -> Vec<ChordItem> {
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
