use opentab_model::MidiBank;
use opentab_playback::{BEND_RANGE_SEMITONES, MidiMessage, PITCH_BEND_RANGE_RPN};

pub(crate) const DRUM_BANK_MSB: u8 = 120;
const DRUM_CHANNEL: u8 = 9;
const CHANNELS: u8 = 16;
pub(crate) const PERCUSSION_BANK: i32 = 128;
const MELODIC_BANKS: i32 = 128;
const NOTE_OFF: i32 = 0x80;
const NOTE_ON: i32 = 0x90;
const CONTROL: i32 = 0xB0;
const PROGRAM: i32 = 0xC0;
const PITCH_BEND: i32 = 0xE0;
const BANK_SELECT: u8 = 0;
const BANK_SELECT_LSB: u8 = 32;
const DATA_ENTRY: i32 = 6;
const DATA_ENTRY_LSB: i32 = 38;
const RPN_LSB: i32 = 100;
const RPN_MSB: i32 = 101;
const RPN_NULL: i32 = 127;
const SEVEN_BITS: i32 = 0x7F;
const MAX_MESSAGES: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct RawMessage {
    pub(crate) command: i32,
    pub(crate) data1: i32,
    pub(crate) data2: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct RawBatch {
    messages: [RawMessage; MAX_MESSAGES],
    count: usize,
}

impl RawBatch {
    fn push(&mut self, command: i32, data1: i32, data2: i32) {
        if let Some(slot) = self.messages.get_mut(self.count) {
            *slot = RawMessage {
                command,
                data1,
                data2,
            };
            self.count += 1;
        }
    }

    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&RawMessage) -> bool) {
        let mut kept = RawBatch::default();
        for raw in self.messages() {
            if keep(raw) {
                kept.push(raw.command, raw.data1, raw.data2);
            }
        }
        *self = kept;
    }

    pub(crate) fn messages(&self) -> &[RawMessage] {
        &self.messages[..self.count]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NoteStart {
    pub(crate) bank: i32,
    pub(crate) patch: i32,
    pub(crate) key: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChannelPresets {
    banks: [i32; CHANNELS as usize],
    patches: [i32; CHANNELS as usize],
}

impl Default for ChannelPresets {
    fn default() -> ChannelPresets {
        let mut banks = [0; CHANNELS as usize];
        banks[usize::from(DRUM_CHANNEL)] = PERCUSSION_BANK;
        ChannelPresets {
            banks,
            patches: [0; CHANNELS as usize],
        }
    }
}

impl ChannelPresets {
    pub(crate) fn observe(&mut self, channel: u8, raw: &RawMessage) -> Option<NoteStart> {
        let slot = usize::from(channel);
        match raw.command {
            CONTROL if raw.data1 == i32::from(BANK_SELECT) => {
                let offset = if channel == DRUM_CHANNEL {
                    PERCUSSION_BANK
                } else {
                    0
                };
                if let Some(bank) = self.banks.get_mut(slot) {
                    *bank = raw.data2 + offset;
                }
                None
            }
            PROGRAM => {
                if let Some(patch) = self.patches.get_mut(slot) {
                    *patch = raw.data1;
                }
                None
            }
            NOTE_ON if raw.data2 > 0 => Some(NoteStart {
                bank: self.banks.get(slot).copied().unwrap_or(0),
                patch: self.patches.get(slot).copied().unwrap_or(0),
                key: u8::try_from(raw.data1).unwrap_or(u8::MAX),
            }),
            _ => None,
        }
    }
}

pub(crate) fn bank_select(channel: u8, msb: u8) -> i32 {
    if channel == DRUM_CHANNEL {
        0
    } else if msb == DRUM_BANK_MSB {
        PERCUSSION_BANK
    } else {
        i32::from(msb)
    }
}

pub(crate) fn listed_bank(sf2_bank: i32) -> Option<MidiBank> {
    if sf2_bank == PERCUSSION_BANK {
        return MidiBank::new(DRUM_BANK_MSB, 0).ok();
    }
    let msb = u8::try_from(sf2_bank)
        .ok()
        .filter(|msb| i32::from(*msb) < MELODIC_BANKS && *msb != DRUM_BANK_MSB)?;
    MidiBank::new(msb, 0).ok()
}

pub(crate) fn translate(channel: u8, message: &MidiMessage) -> RawBatch {
    let mut batch = RawBatch::default();
    match *message {
        MidiMessage::NoteOn { key, velocity } => {
            batch.push(NOTE_ON, i32::from(key), i32::from(velocity));
        }
        MidiMessage::NoteOff { key, velocity } => {
            batch.push(NOTE_OFF, i32::from(key), i32::from(velocity));
        }
        MidiMessage::Program { program } => batch.push(PROGRAM, i32::from(program), 0),
        MidiMessage::Bank { bank } => {
            batch.push(
                CONTROL,
                i32::from(BANK_SELECT),
                bank_select(channel, bank.msb()),
            );
        }
        MidiMessage::Control {
            controller: BANK_SELECT,
            value,
        } => batch.push(CONTROL, i32::from(BANK_SELECT), bank_select(channel, value)),
        MidiMessage::Control {
            controller: BANK_SELECT_LSB,
            ..
        } => {}
        MidiMessage::Control { controller, value } => {
            batch.push(CONTROL, i32::from(controller), i32::from(value));
        }
        MidiMessage::Rpn { parameter, value } => {
            let parameter = i32::from(parameter);
            batch.push(CONTROL, RPN_MSB, (parameter >> 7) & SEVEN_BITS);
            batch.push(CONTROL, RPN_LSB, parameter & SEVEN_BITS);
            batch.push(CONTROL, DATA_ENTRY, i32::from(value));
            batch.push(CONTROL, DATA_ENTRY_LSB, 0);
            batch.push(CONTROL, RPN_MSB, RPN_NULL);
            batch.push(CONTROL, RPN_LSB, RPN_NULL);
        }
        MidiMessage::PitchBend { value } => {
            let value = i32::from(value);
            batch.push(PITCH_BEND, value & SEVEN_BITS, (value >> 7) & SEVEN_BITS);
        }
    }
    batch
}

pub(crate) fn default_bend_range() -> MidiMessage {
    MidiMessage::Rpn {
        parameter: PITCH_BEND_RANGE_RPN,
        value: BEND_RANGE_SEMITONES,
    }
}

pub(crate) fn channels() -> impl Iterator<Item = u8> {
    0..CHANNELS
}
