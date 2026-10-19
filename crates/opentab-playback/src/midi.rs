use opentab_model::{Dynamic, MidiBank, MidiChannel};

use crate::ScoreTime;

pub const CHANNELS: u8 = 16;
pub const DRUM_CHANNEL: u8 = 9;
pub const VOLUME_CONTROLLER: u8 = 7;
pub const PAN_CONTROLLER: u8 = 10;
pub const PITCH_BEND_RANGE_RPN: u16 = 0;
pub const BEND_CENTRE: u16 = 8192;
pub const BEND_MAX: u16 = 16383;
pub const DEFAULT_VOLUME: u8 = 100;
pub const DEFAULT_PAN: u8 = 64;
pub const NOTE_OFF_VELOCITY: u8 = 64;
pub const BEND_RANGE_SEMITONES: u8 = 6;
const VELOCITIES: [u8; 8] = [25, 38, 51, 64, 76, 89, 102, 114];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Channel(u8);

impl Channel {
    pub const DRUMS: Channel = Channel(DRUM_CHANNEL);

    pub fn new(channel: u8) -> Option<Channel> {
        (channel < CHANNELS).then_some(Channel(channel))
    }

    pub fn get(self) -> u8 {
        self.0
    }
}

impl From<MidiChannel> for Channel {
    fn from(channel: MidiChannel) -> Channel {
        Channel(channel.get())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MidiMessage {
    NoteOn { key: u8, velocity: u8 },
    NoteOff { key: u8, velocity: u8 },
    Program { program: u8 },
    Bank { bank: MidiBank },
    Control { controller: u8, value: u8 },
    Rpn { parameter: u16, value: u8 },
    PitchBend { value: u16 },
}

impl MidiMessage {
    pub(crate) fn rank(self) -> u8 {
        match self {
            MidiMessage::NoteOff { .. } => 0,
            MidiMessage::Bank { .. } => 1,
            MidiMessage::Program { .. } => 2,
            MidiMessage::Control { .. } => 3,
            MidiMessage::Rpn { .. } => 4,
            MidiMessage::PitchBend { .. } => 5,
            MidiMessage::NoteOn { .. } => 6,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimedEvent {
    pub at: ScoreTime,
    pub seq: u32,
    pub track: usize,
    pub channel: Channel,
    pub message: MidiMessage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChannelState {
    pub program: u8,
    pub bank: MidiBank,
    pub volume: u8,
    pub pan: u8,
    pub bend_range: u8,
    pub bend: u16,
}

impl Default for ChannelState {
    fn default() -> ChannelState {
        ChannelState {
            program: 0,
            bank: MidiBank::default(),
            volume: DEFAULT_VOLUME,
            pan: DEFAULT_PAN,
            bend_range: BEND_RANGE_SEMITONES,
            bend: BEND_CENTRE,
        }
    }
}

impl ChannelState {
    pub fn apply(&mut self, message: MidiMessage) {
        match message {
            MidiMessage::Program { program } => self.program = program,
            MidiMessage::Bank { bank } => self.bank = bank,
            MidiMessage::Control {
                controller: VOLUME_CONTROLLER,
                value,
            } => self.volume = value,
            MidiMessage::Control {
                controller: PAN_CONTROLLER,
                value,
            } => self.pan = value,
            MidiMessage::Rpn {
                parameter: PITCH_BEND_RANGE_RPN,
                value,
            } => self.bend_range = value,
            MidiMessage::PitchBend { value } => self.bend = value,
            MidiMessage::NoteOn { .. }
            | MidiMessage::NoteOff { .. }
            | MidiMessage::Control { .. }
            | MidiMessage::Rpn { .. } => {}
        }
    }

    pub fn messages(&self) -> [MidiMessage; 6] {
        [
            MidiMessage::Bank { bank: self.bank },
            MidiMessage::Program {
                program: self.program,
            },
            MidiMessage::Control {
                controller: VOLUME_CONTROLLER,
                value: self.volume,
            },
            MidiMessage::Control {
                controller: PAN_CONTROLLER,
                value: self.pan,
            },
            MidiMessage::Rpn {
                parameter: PITCH_BEND_RANGE_RPN,
                value: self.bend_range,
            },
            MidiMessage::PitchBend { value: self.bend },
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChannelSetup {
    pub channel: Channel,
    pub state: ChannelState,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TrackSetup {
    pub name: String,
    pub channel: Channel,
    pub channels: Vec<ChannelSetup>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConductorKind {
    Tempo { micros_per_quarter: u32 },
    TimeSignature { numerator: u8, denominator: u8 },
    KeySignature { accidentals: i8, minor: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConductorEvent {
    pub at: ScoreTime,
    pub kind: ConductorKind,
}

pub fn velocity_of(dynamic: Dynamic) -> u8 {
    let step = match dynamic {
        Dynamic::Ppp => 0,
        Dynamic::Pp => 1,
        Dynamic::P => 2,
        Dynamic::Mp => 3,
        Dynamic::Mf => 4,
        Dynamic::F => 5,
        Dynamic::Ff => 6,
        Dynamic::Fff => 7,
    };
    VELOCITIES[step]
}
