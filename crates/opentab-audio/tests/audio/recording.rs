use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use num_rational::Ratio;
use opentab_audio::{SampleRate, Synth};
use opentab_playback::{
    Channel, ChannelState, MidiMessage, Performance, ScoreTime, Speed, TempoMap,
};

const CHANNELS: usize = 16;
const PERCENT: i128 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Heard {
    pub frame: u64,
    pub channel: Channel,
    pub message: MidiMessage,
}

#[derive(Debug, Default)]
pub struct Log {
    pub rendered: u64,
    pub heard: Vec<Heard>,
}

pub type SharedLog = Arc<Mutex<Log>>;

pub struct RecordingSynth {
    log: SharedLog,
}

impl Synth for RecordingSynth {
    fn reset(&mut self) {
        let mut log = self.log.lock().expect("log");
        log.rendered = 0;
        log.heard.clear();
    }

    fn handle(&mut self, channel: Channel, message: &MidiMessage) {
        let mut log = self.log.lock().expect("log");
        let frame = log.rendered;
        log.heard.push(Heard {
            frame,
            channel,
            message: *message,
        });
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        right.fill(0.0);
        self.log.lock().expect("log").rendered += left.len() as u64;
    }
}

pub fn recording() -> (SharedLog, Box<dyn Synth>) {
    let log: SharedLog = Arc::new(Mutex::new(Log::default()));
    let synth = Box::new(RecordingSynth {
        log: Arc::clone(&log),
    });
    (log, synth)
}

pub struct Recorders {
    pub logs: Vec<SharedLog>,
    pub voices: Vec<Box<dyn Synth>>,
}

pub fn recorders(performance: &Performance) -> Recorders {
    let (logs, voices) = performance.tracks.iter().map(|_| recording()).unzip();
    Recorders { logs, voices }
}

pub fn heard(log: &SharedLog) -> Vec<Heard> {
    log.lock().expect("log").heard.clone()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SynthModel {
    pub states: [ChannelState; CHANNELS],
    pub sounding: BTreeMap<(u8, u8), u32>,
}

impl Default for SynthModel {
    fn default() -> SynthModel {
        SynthModel {
            states: [ChannelState::default(); CHANNELS],
            sounding: BTreeMap::new(),
        }
    }
}

impl SynthModel {
    pub fn apply(&mut self, channel: Channel, message: MidiMessage) {
        let index = usize::from(channel.get());
        self.states[index].apply(message);
        let key = |key: u8| (channel.get(), key);
        match message {
            MidiMessage::NoteOn {
                key: note,
                velocity,
            } if velocity > 0 => {
                *self.sounding.entry(key(note)).or_insert(0) += 1;
            }
            MidiMessage::NoteOn { key: note, .. } | MidiMessage::NoteOff { key: note, .. } => {
                if let Some(count) = self.sounding.get_mut(&key(note)) {
                    *count -= 1;
                    if *count == 0 {
                        self.sounding.remove(&key(note));
                    }
                }
            }
            _ => {}
        }
    }

    pub fn of_log(heard: &[Heard], through: u64) -> SynthModel {
        let mut model = SynthModel::default();
        for entry in heard.iter().filter(|entry| entry.frame <= through) {
            model.apply(entry.channel, entry.message);
        }
        model
    }
}

pub fn unit_of(tempo: &TempoMap, at: ScoreTime, rate: SampleRate) -> u64 {
    let seconds = tempo.seconds_at(at, Speed::NORMAL).expect("seconds");
    let exact = Ratio::new(seconds.numer(), seconds.denom())
        * Ratio::from_integer(i128::from(rate.hz()) * PERCENT);
    u64::try_from(exact.floor().to_integer()).expect("unit")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Note {
    pub channel: u8,
    pub key: u8,
    pub on: u64,
    pub off: Option<u64>,
}

pub fn notes(heard: &[Heard]) -> Vec<Note> {
    let mut open: BTreeMap<(u8, u8), Vec<usize>> = BTreeMap::new();
    let mut notes: Vec<Note> = Vec::new();
    for entry in heard {
        let channel = entry.channel.get();
        match entry.message {
            MidiMessage::NoteOn { key, velocity } if velocity > 0 => {
                open.entry((channel, key)).or_default().push(notes.len());
                notes.push(Note {
                    channel,
                    key,
                    on: entry.frame,
                    off: None,
                });
            }
            MidiMessage::NoteOn { key, .. } | MidiMessage::NoteOff { key, .. } => {
                let queue = open.entry((channel, key)).or_default();
                assert!(
                    !queue.is_empty(),
                    "a note-off on channel {channel} key {key} at frame {} closes no note",
                    entry.frame
                );
                let index = queue.remove(0);
                notes[index].off = Some(entry.frame);
            }
            _ => {}
        }
    }
    notes
}
