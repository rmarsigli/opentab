use std::collections::BTreeMap;

use crate::oracle_allowances::TempoAt;
use crate::smf::{SmfKind, SmfText, read_smf};

pub const CONDUCTOR_TRACK: usize = 0;
const DATA_ENTRY: u8 = 6;
const RPN_MSB: u8 = 101;
const RPN_LSB: u8 = 100;
const PITCH_BEND_RANGE: u16 = 0;
const SEVEN_BITS: u32 = 7;
const NOTE_OFF_CLASS: u8 = 0;
const PROGRAM_CLASS: u8 = 1;
const CONTROL_CLASS: u8 = 2;
const BEND_CLASS: u8 = 3;
const NOTE_ON_CLASS: u8 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Onset {
    pub track: usize,
    pub channel: u8,
    pub key: u8,
    pub velocity: u8,
    pub tick: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Note {
    pub onset: Onset,
    pub length: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BendAt {
    pub tick: u64,
    pub value: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TrackValue {
    pub track: usize,
    pub channel: u8,
    pub tick: u64,
    pub value: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Signature {
    Time { numerator: u8, denominator: u8 },
    Key { accidentals: i8, minor: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SignatureAt {
    pub tick: u64,
    pub signature: Signature,
}

#[derive(Debug, Clone, Default)]
pub struct Side {
    pub division: u16,
    pub tracks: usize,
    pub notes: Vec<Note>,
    pub unreleased: Vec<Onset>,
    pub stray_offs: Vec<Onset>,
    pub bends: BTreeMap<Selection, Vec<BendAt>>,
    pub programs: Vec<TrackValue>,
    pub ranges: Vec<TrackValue>,
    pub sequence: Vec<Placement>,
    pub controllers: Vec<String>,
    pub tempi: Vec<TempoAt>,
    pub times: Vec<SignatureAt>,
    pub keys: BTreeMap<usize, Vec<SignatureAt>>,
    pub texts: Vec<SmfText>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Slot {
    pub track: usize,
    pub channel: u8,
    pub key: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Selection {
    pub track: usize,
    pub channel: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Placement {
    pub track: usize,
    pub tick: u64,
    pub class: u8,
}

pub fn side(bytes: &[u8]) -> Side {
    let smf = read_smf(bytes);
    let mut side = Side {
        division: smf.division,
        tracks: smf
            .events
            .iter()
            .map(|event| event.track + 1)
            .max()
            .unwrap_or(0),
        texts: smf.texts.clone(),
        ..Side::default()
    };
    let mut open: BTreeMap<Slot, Vec<Onset>> = BTreeMap::new();
    let mut selects: BTreeMap<Selection, Vec<(u8, u8)>> = BTreeMap::new();
    let mut events = smf.events.clone();
    events.sort_by_key(|event| (event.track, event.tick));
    for event in events {
        let track = event.track;
        let channel = event.channel;
        let tick = event.tick;
        let class = match event.kind {
            SmfKind::NoteOff { .. } => Some(NOTE_OFF_CLASS),
            SmfKind::Program { .. } => Some(PROGRAM_CLASS),
            SmfKind::Control { .. } => Some(CONTROL_CLASS),
            SmfKind::PitchBend { .. } => Some(BEND_CLASS),
            SmfKind::NoteOn { .. } => Some(NOTE_ON_CLASS),
            _ => None,
        };
        if let Some(class) = class {
            side.sequence.push(Placement { track, tick, class });
        }
        match event.kind {
            SmfKind::NoteOn { key, velocity } => {
                open.entry(Slot {
                    track,
                    channel,
                    key,
                })
                .or_default()
                .push(Onset {
                    track,
                    channel,
                    key,
                    velocity,
                    tick,
                });
            }
            SmfKind::NoteOff { key } => {
                let slot = Slot {
                    track,
                    channel,
                    key,
                };
                let started = open
                    .get_mut(&slot)
                    .filter(|starts| !starts.is_empty())
                    .map(|starts| starts.remove(0));
                match started {
                    Some(onset) => side.notes.push(Note {
                        onset,
                        length: tick.saturating_sub(onset.tick),
                    }),
                    None => side.stray_offs.push(Onset {
                        track,
                        channel,
                        key,
                        velocity: 0,
                        tick,
                    }),
                }
            }
            SmfKind::PitchBend { value } => side
                .bends
                .entry(Selection { track, channel })
                .or_default()
                .push(BendAt { tick, value }),
            SmfKind::Program { program } => side.programs.push(TrackValue {
                track,
                channel,
                tick,
                value: program,
            }),
            SmfKind::Control { controller, value } => {
                let selection = Selection { track, channel };
                if controller == RPN_MSB || controller == RPN_LSB {
                    selects
                        .entry(selection)
                        .or_default()
                        .push((controller, value));
                } else if controller == DATA_ENTRY {
                    let selected = selects.remove(&selection).unwrap_or_default();
                    let parameter = match selected.as_slice() {
                        [(RPN_MSB, msb), (RPN_LSB, lsb)] => {
                            Some((u16::from(*msb) << SEVEN_BITS) | u16::from(*lsb))
                        }
                        _ => None,
                    };
                    if parameter == Some(PITCH_BEND_RANGE) {
                        side.ranges.push(TrackValue {
                            track,
                            channel,
                            tick,
                            value,
                        });
                    } else {
                        side.controllers
                            .push(format!("data entry after {selected:?} at {tick}"));
                    }
                } else {
                    side.controllers
                        .push(format!("controller {controller} = {value} at {tick}"));
                }
            }
            SmfKind::Tempo { micros } if track == CONDUCTOR_TRACK => {
                side.tempi.push(TempoAt { tick, micros });
            }
            SmfKind::Tempo { .. } => side
                .controllers
                .push(format!("a tempo on track {track} at {tick}")),
            SmfKind::TimeSignature {
                numerator,
                denominator,
            } if track == CONDUCTOR_TRACK => side.times.push(SignatureAt {
                tick,
                signature: Signature::Time {
                    numerator,
                    denominator,
                },
            }),
            SmfKind::KeySignature { accidentals, minor } if track != CONDUCTOR_TRACK => {
                side.keys.entry(track).or_default().push(SignatureAt {
                    tick,
                    signature: Signature::Key { accidentals, minor },
                });
            }
            SmfKind::TimeSignature { .. } | SmfKind::KeySignature { .. } => side
                .controllers
                .push(format!("a signature on track {track} at {tick}")),
            SmfKind::Other => {}
        }
    }
    side.unreleased = open.into_values().flatten().collect();
    side.unreleased.sort();
    side.stray_offs.sort();
    side.tempi.sort();
    side.programs.sort();
    side.ranges.sort();
    for (selection, selected) in selects {
        side.controllers.push(format!(
            "an RPN selection without data entry: {selection:?} {selected:?}"
        ));
    }
    side
}
