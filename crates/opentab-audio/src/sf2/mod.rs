#[deny(clippy::float_arithmetic)]
mod map;
#[deny(clippy::float_arithmetic)]
mod riff;

use std::any::Any;
use std::collections::BTreeMap;
use std::fmt;
use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use opentab_model::MidiBank;
use opentab_playback::{Channel, MidiMessage};
use rustysynth::{SoundFont, SoundFontError, Synthesizer, SynthesizerSettings};

use crate::error::SF3_UNSUPPORTED;
use crate::{Bank, BankError, BankFormat, BankInstrument, SampleRate, Synth, TRACK_HEADROOM};

const UNKNOWN_PANIC: &str = "the sound engine stopped without a message";
const BYTES_PER_POINT: usize = 2;
const MAX_PROGRAM: u8 = 127;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SynthStage {
    Instantiate,
    Reset,
    Handle,
    Render,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SynthFault {
    pub instance: u32,
    pub stage: SynthStage,
    pub message: String,
}

#[derive(Debug, Default)]
struct FaultLog {
    entries: Mutex<Vec<SynthFault>>,
}

impl FaultLog {
    fn push(&self, fault: SynthFault) {
        match self.entries.lock() {
            Ok(mut entries) => entries.push(fault),
            Err(poisoned) => poisoned.into_inner().push(fault),
        }
    }

    fn snapshot(&self) -> Vec<SynthFault> {
        match self.entries.lock() {
            Ok(entries) => entries.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct ListingKey {
    msb: u8,
    lsb: u8,
    program: u8,
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        UNKNOWN_PANIC.to_owned()
    }
}

fn load_error(error: SoundFontError) -> BankError {
    match error {
        SoundFontError::UnsupportedSampleFormat => BankError::Unsupported {
            message: SF3_UNSUPPORTED.to_owned(),
        },
        other => BankError::Invalid {
            reason: other.to_string(),
        },
    }
}

fn listing(font: &SoundFont) -> Vec<BankInstrument> {
    let mut found: BTreeMap<ListingKey, String> = BTreeMap::new();
    for preset in font.get_presets() {
        let Some(bank) = map::listed_bank(preset.get_bank_number()) else {
            continue;
        };
        let Ok(program) = u8::try_from(preset.get_patch_number()) else {
            continue;
        };
        if program > MAX_PROGRAM {
            continue;
        }
        let key = ListingKey {
            msb: bank.msb(),
            lsb: bank.lsb(),
            program,
        };
        found.insert(key, preset.get_name().to_owned());
    }
    found
        .into_iter()
        .filter_map(|(key, name)| {
            MidiBank::new(key.msb, key.lsb)
                .ok()
                .map(|bank| BankInstrument {
                    bank,
                    program: key.program,
                    name,
                })
        })
        .collect()
}

pub struct Sf2Bank {
    font: Arc<SoundFont>,
    instruments: Vec<BankInstrument>,
    faults: Arc<FaultLog>,
    next_instance: AtomicU32,
}

impl fmt::Debug for Sf2Bank {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Sf2Bank")
            .field("name", &self.name())
            .field("instruments", &self.instruments.len())
            .field("sample_bytes", &self.sample_bytes())
            .finish()
    }
}

impl Sf2Bank {
    pub fn load(bytes: &[u8]) -> Result<Sf2Bank, BankError> {
        riff::check(bytes)?;
        let loaded =
            catch_unwind(|| SoundFont::new(&mut Cursor::new(bytes))).map_err(|payload| {
                BankError::Panicked {
                    message: panic_message(payload.as_ref()),
                }
            })?;
        let font = loaded.map_err(load_error)?;
        let instruments = listing(&font);
        Ok(Sf2Bank {
            font: Arc::new(font),
            instruments,
            faults: Arc::new(FaultLog::default()),
            next_instance: AtomicU32::new(0),
        })
    }

    pub fn name(&self) -> &str {
        self.font.get_info().get_bank_name()
    }

    pub fn sample_bytes(&self) -> usize {
        self.font.get_wave_data().len() * BYTES_PER_POINT
    }

    pub fn shared_count(&self) -> usize {
        Arc::strong_count(&self.font)
    }

    pub fn faults(&self) -> Vec<SynthFault> {
        self.faults.snapshot()
    }

    pub fn synth(&self, rate: SampleRate) -> Sf2Synth {
        let instance = self.next_instance.fetch_add(1, Ordering::Relaxed);
        Sf2Synth::new(
            Arc::clone(&self.font),
            rate,
            instance,
            Arc::clone(&self.faults),
        )
    }
}

impl Bank for Sf2Bank {
    fn format(&self) -> BankFormat {
        BankFormat::Sf2
    }

    fn instruments(&self) -> Vec<BankInstrument> {
        self.instruments.clone()
    }

    fn instantiate(&self, rate: SampleRate) -> Box<dyn Synth> {
        Box::new(self.synth(rate))
    }
}

pub struct Sf2Synth {
    font: Arc<SoundFont>,
    rate: SampleRate,
    engine: Option<Synthesizer>,
    instance: u32,
    log: Arc<FaultLog>,
    fault: Option<SynthFault>,
}

impl fmt::Debug for Sf2Synth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Sf2Synth")
            .field("instance", &self.instance)
            .field("rate", &self.rate)
            .field("silenced", &self.is_silenced())
            .field("fault", &self.fault)
            .finish()
    }
}

impl Sf2Synth {
    fn new(font: Arc<SoundFont>, rate: SampleRate, instance: u32, log: Arc<FaultLog>) -> Sf2Synth {
        let mut synth = Sf2Synth {
            font,
            rate,
            engine: None,
            instance,
            log,
            fault: None,
        };
        synth.build();
        synth
    }

    pub fn fault(&self) -> Option<&SynthFault> {
        self.fault.as_ref()
    }

    pub fn is_silenced(&self) -> bool {
        self.engine.is_none()
    }

    fn build(&mut self) {
        let hz = i32::try_from(self.rate.hz()).unwrap_or(i32::MAX);
        let settings = SynthesizerSettings::new(hz);
        let font = &self.font;
        match catch_unwind(AssertUnwindSafe(|| Synthesizer::new(font, &settings))) {
            Ok(Ok(engine)) => {
                self.engine = Some(engine);
                self.apply_defaults();
            }
            Ok(Err(error)) => self.record(SynthStage::Instantiate, error.to_string()),
            Err(payload) => self.record(SynthStage::Instantiate, panic_message(payload.as_ref())),
        }
    }

    fn apply_defaults(&mut self) {
        let range = map::default_bend_range();
        for channel in map::channels() {
            self.send(channel, &range);
        }
    }

    fn record(&mut self, stage: SynthStage, message: String) {
        self.engine = None;
        let fault = SynthFault {
            instance: self.instance,
            stage,
            message,
        };
        self.log.push(fault.clone());
        self.fault = Some(fault);
    }

    fn guarded(&mut self, stage: SynthStage, action: impl FnOnce(&mut Synthesizer)) -> bool {
        let Some(engine) = self.engine.as_mut() else {
            return false;
        };
        match catch_unwind(AssertUnwindSafe(|| action(engine))) {
            Ok(()) => true,
            Err(payload) => {
                self.record(stage, panic_message(payload.as_ref()));
                false
            }
        }
    }

    fn send(&mut self, channel: u8, message: &MidiMessage) {
        let batch = map::translate(channel, message);
        let channel = i32::from(channel);
        self.guarded(SynthStage::Handle, |engine| {
            for raw in batch.messages() {
                engine.process_midi_message(channel, raw.command, raw.data1, raw.data2);
            }
        });
    }
}

fn sanitize(samples: &mut [f32]) {
    for sample in samples {
        *sample = if sample.is_finite() {
            sample.clamp(-TRACK_HEADROOM, TRACK_HEADROOM)
        } else {
            0.0
        };
    }
}

impl Synth for Sf2Synth {
    fn reset(&mut self) {
        if self.engine.is_none() {
            self.build();
            return;
        }
        if self.guarded(SynthStage::Reset, Synthesizer::reset) {
            self.apply_defaults();
        }
    }

    fn handle(&mut self, channel: Channel, message: &MidiMessage) {
        self.send(channel.get(), message);
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let frames = left.len().min(right.len());
        let (left, left_rest) = left.split_at_mut(frames);
        let (right, right_rest) = right.split_at_mut(frames);
        left_rest.fill(0.0);
        right_rest.fill(0.0);
        let rendered = self.guarded(SynthStage::Render, |engine| engine.render(left, right));
        if rendered {
            sanitize(left);
            sanitize(right);
        } else {
            left.fill(0.0);
            right.fill(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Sf2Bank, SynthStage};
    use crate::{SampleRate, Synth};

    #[test]
    fn a_panic_inside_the_engine_silences_the_voice_and_is_reported() {
        let bytes = opentab_testkit::banks::tiny_sf2();
        let bank = Sf2Bank::load(&bytes).expect("tiny bank loads");
        let mut synth = bank.synth(SampleRate::STUDIO);
        assert!(!synth.is_silenced());
        let survived = synth.guarded(SynthStage::Render, |_| panic!("engine exploded"));
        assert!(!survived);
        assert!(synth.is_silenced());
        let mut left = vec![1.0f32; 64];
        let mut right = vec![1.0f32; 64];
        synth.render(&mut left, &mut right);
        assert!(left.iter().chain(&right).all(|sample| *sample == 0.0));
        let faults = bank.faults();
        assert_eq!(faults.len(), 1);
        assert_eq!(faults[0].stage, SynthStage::Render);
        assert_eq!(faults[0].message, "engine exploded");
        assert_eq!(synth.fault(), Some(&faults[0]));
        synth.reset();
        assert!(!synth.is_silenced());
    }
}
