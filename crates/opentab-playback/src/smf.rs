use std::collections::{BTreeMap, VecDeque};

use opentab_model::Score;

use crate::midi::{ConductorEvent, ConductorKind, MidiMessage, TimedEvent};
use crate::perform::{Performance, PlaybackOptions, Profile, perform};
use crate::{PlaybackError, ScoreTime};

const HEADER: &[u8; 4] = b"MThd";
const TRACK: &[u8; 4] = b"MTrk";
const HEADER_LENGTH: u32 = 6;
const FORMAT_ONE: u16 = 1;
const QUARTERS_PER_WHOLE: i128 = 4;
const MAX_DIVISION: u16 = 0x7fff;
const META: u8 = 0xff;
const TEXT: u8 = 0x01;
const TRACK_NAME: u8 = 0x03;
const END_OF_TRACK: u8 = 0x2f;
const TEMPO: u8 = 0x51;
const TIME_SIGNATURE: u8 = 0x58;
const KEY_SIGNATURE: u8 = 0x59;
const CLOCKS_PER_CLICK: u8 = 24;
const THIRTY_SECONDS_PER_QUARTER: u8 = 8;
const MAX_TEMPO: u32 = 0x00ff_ffff;
const MAX_DELTA: u32 = 0x0fff_ffff;
const NOTE_OFF: u8 = 0x80;
const NOTE_ON: u8 = 0x90;
const CONTROL: u8 = 0xb0;
const PROGRAM: u8 = 0xc0;
const PITCH_BEND: u8 = 0xe0;
const BANK_MSB: u8 = 0;
const BANK_LSB: u8 = 32;
const RPN_MSB: u8 = 101;
const RPN_LSB: u8 = 100;
const DATA_ENTRY: u8 = 6;
const SEVEN_BITS: u32 = 7;
const LOW_SEVEN: u32 = 0x7f;
const CONTINUES: u8 = 0x80;
const NAME_GROUP: u8 = 0;
const KEY_GROUP: u8 = 1;
const EVENT_GROUP: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ppq(u16);

impl Ppq {
    pub const GP: Ppq = Ppq(480);
    pub const MUSICAL: Ppq = Ppq(960);

    pub fn new(ticks: u16) -> Option<Ppq> {
        (1..=MAX_DIVISION).contains(&ticks).then_some(Ppq(ticks))
    }

    pub fn get(self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrackLayout {
    PerTrack,
    PerStaff,
}

pub fn to_ticks(t: ScoreTime, ppq: Ppq) -> u64 {
    let whole = t.whole_notes();
    let (numer, denom) = if *whole.denom() < 0 {
        (-i128::from(*whole.numer()), -i128::from(*whole.denom()))
    } else {
        (i128::from(*whole.numer()), i128::from(*whole.denom()))
    };
    let scaled = numer * QUARTERS_PER_WHOLE * i128::from(ppq.get()) * 2 + denom;
    u64::try_from(scaled.div_euclid(denom * 2)).unwrap_or(u64::MAX)
}

fn low_seven(value: u32) -> u8 {
    u8::try_from(value & LOW_SEVEN).unwrap_or(0)
}

pub(crate) fn variable_length(value: u32) -> Vec<u8> {
    let mut bytes = vec![low_seven(value)];
    let mut rest = value >> SEVEN_BITS;
    while rest > 0 {
        bytes.push(low_seven(rest) | CONTINUES);
        rest >>= SEVEN_BITS;
    }
    bytes.reverse();
    bytes
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Placed {
    tick: u64,
    group: u8,
    seq: usize,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct KeyOnChannel {
    channel: u8,
    key: u8,
}

fn meta(kind: u8, data: &[u8]) -> Vec<u8> {
    let length = u32::try_from(data.len()).unwrap_or(MAX_DELTA);
    let mut bytes = vec![META, kind];
    bytes.extend(variable_length(length.min(MAX_DELTA)));
    bytes.extend(data);
    bytes
}

fn conductor_bytes(kind: ConductorKind) -> Vec<u8> {
    match kind {
        ConductorKind::Tempo { micros_per_quarter } => {
            let [_, high, middle, low] = micros_per_quarter.min(MAX_TEMPO).to_be_bytes();
            meta(TEMPO, &[high, middle, low])
        }
        ConductorKind::TimeSignature {
            numerator,
            denominator,
        } => {
            let power = u8::try_from(denominator.trailing_zeros()).unwrap_or(0);
            meta(
                TIME_SIGNATURE,
                &[
                    numerator,
                    power,
                    CLOCKS_PER_CLICK,
                    THIRTY_SECONDS_PER_QUARTER,
                ],
            )
        }
        ConductorKind::KeySignature { accidentals, minor } => meta(
            KEY_SIGNATURE,
            &[accidentals.to_be_bytes()[0], u8::from(minor)],
        ),
    }
}

fn channel_messages(channel: u8, message: MidiMessage) -> Vec<Vec<u8>> {
    let control = |controller: u8, value: u8| vec![CONTROL | channel, controller, value];
    match message {
        MidiMessage::NoteOn { key, velocity } => vec![vec![NOTE_ON | channel, key, velocity]],
        MidiMessage::NoteOff { key, velocity } => vec![vec![NOTE_OFF | channel, key, velocity]],
        MidiMessage::Program { program } => vec![vec![PROGRAM | channel, program]],
        MidiMessage::Bank { bank } => {
            vec![control(BANK_MSB, bank.msb()), control(BANK_LSB, bank.lsb())]
        }
        MidiMessage::Control { controller, value } => vec![control(controller, value)],
        MidiMessage::Rpn { parameter, value } => vec![
            control(RPN_MSB, low_seven(u32::from(parameter) >> SEVEN_BITS)),
            control(RPN_LSB, low_seven(u32::from(parameter))),
            control(DATA_ENTRY, value),
        ],
        MidiMessage::PitchBend { value } => vec![vec![
            PITCH_BEND | channel,
            low_seven(u32::from(value)),
            low_seven(u32::from(value) >> SEVEN_BITS),
        ]],
    }
}

fn placed_events(events: &[&TimedEvent], ppq: Ppq) -> Vec<Placed> {
    let mut open: BTreeMap<KeyOnChannel, VecDeque<u64>> = BTreeMap::new();
    let mut free: BTreeMap<KeyOnChannel, u64> = BTreeMap::new();
    let mut placed = Vec::new();
    for (seq, event) in events.iter().enumerate() {
        let channel = event.channel.get();
        let mut tick = to_ticks(event.at, ppq);
        match event.message {
            MidiMessage::NoteOn { key, .. } => {
                let slot = KeyOnChannel { channel, key };
                if let Some(after) = free.get(&slot) {
                    tick = tick.max(*after);
                }
                open.entry(slot).or_default().push_back(tick);
            }
            MidiMessage::NoteOff { key, .. } => {
                let slot = KeyOnChannel { channel, key };
                if let Some(start) = open.get_mut(&slot).and_then(VecDeque::pop_front) {
                    tick = tick.max(start.saturating_add(1));
                }
                free.insert(slot, tick);
            }
            _ => {}
        }
        for bytes in channel_messages(channel, event.message) {
            placed.push(Placed {
                tick,
                group: EVENT_GROUP,
                seq,
                bytes,
            });
        }
    }
    placed
}

fn delta(out: &mut Vec<u8>, ticks: u64) {
    let mut rest = ticks;
    let limit = u64::from(MAX_DELTA);
    while rest > limit {
        out.extend(variable_length(MAX_DELTA));
        out.extend(meta(TEXT, &[]));
        rest -= limit;
    }
    out.extend(variable_length(u32::try_from(rest).unwrap_or(MAX_DELTA)));
}

fn chunk(events: &[Placed], end: u64) -> Vec<u8> {
    let mut body = Vec::new();
    let mut now = 0;
    for event in events {
        delta(&mut body, event.tick.saturating_sub(now));
        now = now.max(event.tick);
        body.extend(&event.bytes);
    }
    delta(&mut body, end.saturating_sub(now));
    body.extend(meta(END_OF_TRACK, &[]));
    let mut out = TRACK.to_vec();
    out.extend(u32::try_from(body.len()).unwrap_or(u32::MAX).to_be_bytes());
    out.extend(body);
    out
}

fn conductor_track(conductor: &[ConductorEvent], keys: bool, ppq: Ppq) -> Vec<Placed> {
    conductor
        .iter()
        .filter(|event| keys || !matches!(event.kind, ConductorKind::KeySignature { .. }))
        .enumerate()
        .map(|(seq, event)| Placed {
            tick: to_ticks(event.at, ppq),
            group: EVENT_GROUP,
            seq,
            bytes: conductor_bytes(event.kind),
        })
        .collect()
}

fn note_track(name: &str, keys: &[Placed], events: &[&TimedEvent], ppq: Ppq) -> Vec<Placed> {
    let mut out = vec![Placed {
        tick: 0,
        group: NAME_GROUP,
        seq: 0,
        bytes: meta(TRACK_NAME, name.as_bytes()),
    }];
    out.extend(keys.iter().cloned());
    out.extend(placed_events(events, ppq));
    out.sort_by_key(|item| (item.tick, item.group, item.seq));
    out
}

pub fn write_smf(performance: &Performance, ppq: Ppq) -> Vec<u8> {
    let layout = performance.profile.track_layout();
    let keys: Vec<Placed> = match layout {
        TrackLayout::PerStaff => performance
            .conductor
            .iter()
            .filter(|event| matches!(event.kind, ConductorKind::KeySignature { .. }))
            .enumerate()
            .map(|(seq, event)| Placed {
                tick: to_ticks(event.at, ppq),
                group: KEY_GROUP,
                seq,
                bytes: conductor_bytes(event.kind),
            })
            .collect(),
        TrackLayout::PerTrack => Vec::new(),
    };
    let mut tracks = vec![conductor_track(
        &performance.conductor,
        layout == TrackLayout::PerTrack,
        ppq,
    )];
    for (index, setup) in performance.tracks.iter().enumerate() {
        let of_track = performance
            .events
            .iter()
            .filter(|event| event.track == index);
        match layout {
            TrackLayout::PerTrack => {
                let events: Vec<&TimedEvent> = of_track.collect();
                tracks.push(note_track(&setup.name, &keys, &events, ppq));
            }
            TrackLayout::PerStaff => {
                for staff in 0..setup.staves {
                    let events: Vec<&TimedEvent> = of_track
                        .clone()
                        .filter(|event| event.staff == staff)
                        .collect();
                    tracks.push(note_track(&setup.name, &keys, &events, ppq));
                }
            }
        }
    }
    let last = tracks
        .iter()
        .flatten()
        .map(|event| event.tick)
        .max()
        .unwrap_or(0);
    let end = last.max(to_ticks(performance.order.end(), ppq));
    let mut out = HEADER.to_vec();
    out.extend(HEADER_LENGTH.to_be_bytes());
    out.extend(FORMAT_ONE.to_be_bytes());
    out.extend(
        u16::try_from(tracks.len())
            .unwrap_or(u16::MAX)
            .to_be_bytes(),
    );
    out.extend(ppq.get().to_be_bytes());
    for track in &tracks {
        out.extend(chunk(track, end));
    }
    out
}

pub fn export_midi(score: &Score, profile: Profile) -> Result<Vec<u8>, PlaybackError> {
    let performance = perform(score, &PlaybackOptions { profile })?;
    Ok(write_smf(&performance, profile.ppq()))
}

#[cfg(test)]
mod tests {
    use opentab_model::Fraction;

    use super::{Ppq, to_ticks, variable_length};
    use crate::ScoreTime;

    #[test]
    fn variable_length_quantities_split_seven_bits_per_byte() {
        let cases: [(u32, &[u8]); 9] = [
            (0, &[0x00]),
            (0x40, &[0x40]),
            (0x7f, &[0x7f]),
            (0x80, &[0x81, 0x00]),
            (0x2000, &[0xc0, 0x00]),
            (0x3fff, &[0xff, 0x7f]),
            (0x4000, &[0x81, 0x80, 0x00]),
            (0x001f_ffff, &[0xff, 0xff, 0x7f]),
            (0x0fff_ffff, &[0xff, 0xff, 0xff, 0x7f]),
        ];
        for (value, bytes) in cases {
            assert_eq!(variable_length(value), bytes.to_vec(), "{value:#x}");
        }
    }

    #[test]
    fn ticks_round_half_up_from_the_absolute_position() {
        let at =
            |numer: i64, denom: i64| ScoreTime::new(Fraction::new(numer, denom)).expect("time");
        let gp = Ppq::GP;
        assert_eq!(to_ticks(at(0, 1), gp), 0);
        assert_eq!(to_ticks(at(1, 4), gp), 480);
        assert_eq!(to_ticks(at(1, 3840), gp), 1);
        assert_eq!(to_ticks(at(1, 3841), gp), 0);
        assert_eq!(to_ticks(at(1, 7680), gp), 0);
        assert_eq!(to_ticks(at(5, 7680), gp), 1);
        assert_eq!(to_ticks(at(3, 3840), gp), 2);
        assert_eq!(to_ticks(at(1, 12), gp), 160);
        assert_eq!(to_ticks(at(1, 28), gp), 69);
        assert_eq!(to_ticks(at(1, 28), Ppq::MUSICAL), 137);
        assert_eq!(to_ticks(at(1000, 7), gp), 274_286);
        assert_eq!(
            to_ticks(at(i64::MAX, 1), Ppq::MUSICAL),
            u64::MAX,
            "a position beyond the tick range saturates"
        );
    }
}
