use opentab_model::MidiBank;
use opentab_playback::{Channel, MidiMessage};

use crate::builtin::Engine;
use crate::gm::{DRUM_KIT_NAME, GM_NAMES};
use crate::{Bank, BankFormat, BankInstrument, SampleRate, Synth};

const CHUNK_FRAMES: usize = 256;
const FULL_SCALE: f32 = 8_388_608.0;
const DRUM_BANK_MSB: u8 = 120;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct BuiltInBank;

impl Bank for BuiltInBank {
    fn format(&self) -> BankFormat {
        BankFormat::BuiltIn
    }

    fn instruments(&self) -> Vec<BankInstrument> {
        let mut instruments: Vec<BankInstrument> = (0u8..)
            .zip(GM_NAMES)
            .map(|(program, name)| BankInstrument {
                bank: MidiBank::default(),
                program,
                name: name.to_owned(),
            })
            .collect();
        if let Ok(bank) = MidiBank::new(DRUM_BANK_MSB, 0) {
            instruments.push(BankInstrument {
                bank,
                program: 0,
                name: DRUM_KIT_NAME.to_owned(),
            });
        }
        instruments
    }

    fn instantiate(&self, rate: SampleRate) -> Box<dyn Synth> {
        Box::new(BuiltInSynth::new(rate))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltInSynth {
    engine: Engine,
}

impl BuiltInSynth {
    pub fn new(rate: SampleRate) -> BuiltInSynth {
        BuiltInSynth {
            engine: Engine::new(rate.hz()),
        }
    }

    pub fn sounding(&self) -> usize {
        self.engine.sounding()
    }

    pub fn render_fixed(&mut self, left: &mut [i32], right: &mut [i32]) {
        self.engine.render(left, right);
    }
}

impl Synth for BuiltInSynth {
    fn reset(&mut self) {
        self.engine.reset();
    }

    fn handle(&mut self, channel: Channel, message: &MidiMessage) {
        self.engine.handle(channel.get(), *message);
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let frames = left.len().min(right.len());
        let mut fixed_left = [0i32; CHUNK_FRAMES];
        let mut fixed_right = [0i32; CHUNK_FRAMES];
        let (left, left_rest) = left.split_at_mut(frames);
        let (right, right_rest) = right.split_at_mut(frames);
        left_rest.fill(0.0);
        right_rest.fill(0.0);
        for (out_left, out_right) in left
            .chunks_mut(CHUNK_FRAMES)
            .zip(right.chunks_mut(CHUNK_FRAMES))
        {
            let count = out_left.len();
            let (chunk_left, chunk_right) = (&mut fixed_left[..count], &mut fixed_right[..count]);
            self.engine.render(chunk_left, chunk_right);
            for (target, sample) in out_left.iter_mut().zip(chunk_left.iter()) {
                *target = to_float(*sample);
            }
            for (target, sample) in out_right.iter_mut().zip(chunk_right.iter()) {
                *target = to_float(*sample);
            }
        }
    }
}

fn to_float(sample: i32) -> f32 {
    sample as f32 / FULL_SCALE
}
