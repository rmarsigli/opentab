use opentab_model::MidiBank;
use opentab_playback::{Channel, MidiMessage};

use crate::SampleRate;

pub trait Synth: Send {
    fn reset(&mut self);
    fn handle(&mut self, channel: Channel, message: &MidiMessage);
    fn render(&mut self, left: &mut [f32], right: &mut [f32]);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BankFormat {
    BuiltIn,
    Sf2,
    Sfz,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BankInstrument {
    pub bank: MidiBank,
    pub program: u8,
    pub name: String,
}

pub trait Bank {
    fn format(&self) -> BankFormat;
    fn instruments(&self) -> Vec<BankInstrument>;
    fn instantiate(&self, rate: SampleRate) -> Box<dyn Synth>;
}
