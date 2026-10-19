pub const HEADER: &[u8; 4] = b"MThd";
pub const TRACK: &[u8; 4] = b"MTrk";
const META: u8 = 0xff;
const SYSEX: u8 = 0xf0;
const SYSEX_END: u8 = 0xf7;
const TEMPO: u8 = 0x51;
const STATUS_BIT: u8 = 0x80;
const KIND_MASK: u8 = 0xf0;
const CHANNEL_MASK: u8 = 0x0f;
const NOTE_OFF: u8 = 0x80;
const NOTE_ON: u8 = 0x90;
const CONTROL: u8 = 0xb0;
const PROGRAM: u8 = 0xc0;
const PRESSURE: u8 = 0xd0;
const BEND: u8 = 0xe0;
const SEVEN_BITS: u32 = 7;
const LOW_SEVEN: u8 = 0x7f;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SmfKind {
    NoteOn { key: u8, velocity: u8 },
    NoteOff { key: u8 },
    Control { controller: u8, value: u8 },
    Program { program: u8 },
    PitchBend { value: u16 },
    Tempo { micros: u32 },
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SmfEvent {
    pub tick: u64,
    pub track: usize,
    pub channel: u8,
    pub kind: SmfKind,
}

#[derive(Debug, Clone)]
pub struct Smf {
    pub division: u16,
    pub events: Vec<SmfEvent>,
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn byte(&mut self) -> u8 {
        let value = self.bytes[self.at];
        self.at += 1;
        value
    }

    fn take(&mut self, count: usize) -> &[u8] {
        let slice = &self.bytes[self.at..self.at + count];
        self.at += count;
        slice
    }

    fn quantity(&mut self) -> u64 {
        let mut value: u64 = 0;
        loop {
            let byte = self.byte();
            value = (value << SEVEN_BITS) | u64::from(byte & LOW_SEVEN);
            if byte & STATUS_BIT == 0 {
                return value;
            }
        }
    }
}

fn big(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(0, |value, byte| (value << 8) | u64::from(*byte))
}

pub fn read_smf(bytes: &[u8]) -> Smf {
    assert_eq!(&bytes[..4], HEADER);
    let header = usize::try_from(big(&bytes[4..8])).expect("header length");
    let tracks = big(&bytes[10..12]);
    let division = u16::try_from(big(&bytes[12..14])).expect("division");
    let mut cursor = Cursor {
        bytes,
        at: 8 + header,
    };
    let mut events = Vec::new();
    for track in 0..usize::try_from(tracks).expect("tracks") {
        assert_eq!(cursor.take(4), TRACK);
        let length = usize::try_from(big(cursor.take(4))).expect("length");
        let end = cursor.at + length;
        let mut tick = 0;
        let mut running = 0;
        while cursor.at < end {
            tick += cursor.quantity();
            let mut status = cursor.byte();
            if status == META {
                let kind = cursor.byte();
                let size = usize::try_from(cursor.quantity()).expect("size");
                let data = cursor.take(size);
                let event = if kind == TEMPO {
                    SmfKind::Tempo {
                        micros: u32::try_from(big(data)).expect("tempo"),
                    }
                } else {
                    SmfKind::Other
                };
                events.push(SmfEvent {
                    tick,
                    track,
                    channel: 0,
                    kind: event,
                });
                continue;
            }
            if status == SYSEX || status == SYSEX_END {
                let size = usize::try_from(cursor.quantity()).expect("size");
                cursor.take(size);
                continue;
            }
            let first = if status & STATUS_BIT == 0 {
                let data = status;
                status = running;
                data
            } else {
                running = status;
                cursor.byte()
            };
            let channel = status & CHANNEL_MASK;
            let kind = match status & KIND_MASK {
                NOTE_ON => {
                    let velocity = cursor.byte();
                    if velocity == 0 {
                        SmfKind::NoteOff { key: first }
                    } else {
                        SmfKind::NoteOn {
                            key: first,
                            velocity,
                        }
                    }
                }
                NOTE_OFF => {
                    cursor.byte();
                    SmfKind::NoteOff { key: first }
                }
                CONTROL => SmfKind::Control {
                    controller: first,
                    value: cursor.byte(),
                },
                PROGRAM => SmfKind::Program { program: first },
                PRESSURE => SmfKind::Other,
                BEND => {
                    let high = cursor.byte();
                    SmfKind::PitchBend {
                        value: (u16::from(high) << SEVEN_BITS) | u16::from(first),
                    }
                }
                _ => {
                    cursor.byte();
                    SmfKind::Other
                }
            };
            events.push(SmfEvent {
                tick,
                track,
                channel,
                kind,
            });
        }
    }
    Smf { division, events }
}
