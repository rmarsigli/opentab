use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use opentab_playback::Speed;

pub const COMMAND_CAPACITY: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BarRange {
    pub first: usize,
    pub last: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransportCommand {
    Play,
    Pause,
    Stop,
    Seek(usize),
    Loop(BarRange),
    ClearLoop,
    Speed(Speed),
    Mute(usize, bool),
    Solo(usize, bool),
    Metronome(bool),
    CountIn(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, thiserror::Error)]
#[error(
    "the transport has {capacity} commands waiting and takes no more until it plays the next block"
)]
pub struct QueueFull {
    pub capacity: usize,
}

const TAG_BITS: u32 = 8;
const PAYLOAD_BITS: u32 = 64 - TAG_BITS;
const PAYLOAD_MAX: u64 = (1 << PAYLOAD_BITS) - 1;
const HALF_BITS: u32 = PAYLOAD_BITS / 2;
const HALF_MAX: u64 = (1 << HALF_BITS) - 1;
const PLAY: u64 = 0;
const PAUSE: u64 = 1;
const STOP: u64 = 2;
const SEEK: u64 = 3;
const LOOP: u64 = 4;
const CLEAR_LOOP: u64 = 5;
const SPEED: u64 = 6;
const MUTE: u64 = 7;
const SOLO: u64 = 8;
const METRONOME: u64 = 9;
const COUNT_IN: u64 = 10;

fn saturate(value: usize, max: u64) -> u64 {
    u64::try_from(value).map_or(max, |value| value.min(max))
}

fn widen(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

fn flagged(index: usize, flag: bool) -> u64 {
    (saturate(index, PAYLOAD_MAX >> 1) << 1) | u64::from(flag)
}

fn pack(tag: u64, payload: u64) -> u64 {
    (tag << PAYLOAD_BITS) | (payload & PAYLOAD_MAX)
}

pub(crate) fn encode(command: TransportCommand) -> u64 {
    match command {
        TransportCommand::Play => pack(PLAY, 0),
        TransportCommand::Pause => pack(PAUSE, 0),
        TransportCommand::Stop => pack(STOP, 0),
        TransportCommand::Seek(bar) => pack(SEEK, saturate(bar, PAYLOAD_MAX)),
        TransportCommand::Loop(range) => pack(
            LOOP,
            (saturate(range.first, HALF_MAX) << HALF_BITS) | saturate(range.last, HALF_MAX),
        ),
        TransportCommand::ClearLoop => pack(CLEAR_LOOP, 0),
        TransportCommand::Speed(speed) => pack(SPEED, u64::from(speed.percent())),
        TransportCommand::Mute(track, muted) => pack(MUTE, flagged(track, muted)),
        TransportCommand::Solo(track, soloed) => pack(SOLO, flagged(track, soloed)),
        TransportCommand::Metronome(on) => pack(METRONOME, u64::from(on)),
        TransportCommand::CountIn(on) => pack(COUNT_IN, u64::from(on)),
    }
}

pub(crate) fn decode(word: u64) -> Option<TransportCommand> {
    let payload = word & PAYLOAD_MAX;
    let command = match word >> PAYLOAD_BITS {
        PLAY => TransportCommand::Play,
        PAUSE => TransportCommand::Pause,
        STOP => TransportCommand::Stop,
        SEEK => TransportCommand::Seek(widen(payload)),
        LOOP => TransportCommand::Loop(BarRange {
            first: widen(payload >> HALF_BITS),
            last: widen(payload & HALF_MAX),
        }),
        CLEAR_LOOP => TransportCommand::ClearLoop,
        SPEED => TransportCommand::Speed(Speed::new(u16::try_from(payload).ok()?).ok()?),
        MUTE => TransportCommand::Mute(widen(payload >> 1), payload & 1 == 1),
        SOLO => TransportCommand::Solo(widen(payload >> 1), payload & 1 == 1),
        METRONOME => TransportCommand::Metronome(payload & 1 == 1),
        COUNT_IN => TransportCommand::CountIn(payload & 1 == 1),
        _ => return None,
    };
    Some(command)
}

#[derive(Debug)]
struct Ring {
    slots: Box<[AtomicU64]>,
    written: AtomicUsize,
    read: AtomicUsize,
}

#[derive(Debug)]
pub(crate) struct CommandSender {
    ring: Arc<Ring>,
}

#[derive(Debug)]
pub(crate) struct CommandReceiver {
    ring: Arc<Ring>,
}

pub(crate) fn command_queue() -> (CommandSender, CommandReceiver) {
    let slots: Vec<AtomicU64> = (0..COMMAND_CAPACITY).map(|_| AtomicU64::new(0)).collect();
    let ring = Arc::new(Ring {
        slots: slots.into_boxed_slice(),
        written: AtomicUsize::new(0),
        read: AtomicUsize::new(0),
    });
    (
        CommandSender {
            ring: Arc::clone(&ring),
        },
        CommandReceiver { ring },
    )
}

impl CommandSender {
    pub(crate) fn send(&mut self, command: TransportCommand) -> Result<(), QueueFull> {
        let ring = &self.ring;
        let written = ring.written.load(Ordering::Relaxed);
        let read = ring.read.load(Ordering::Acquire);
        if written.wrapping_sub(read) >= ring.slots.len() {
            return Err(QueueFull {
                capacity: ring.slots.len(),
            });
        }
        if let Some(slot) = ring.slots.get(written % ring.slots.len()) {
            slot.store(encode(command), Ordering::Relaxed);
        }
        ring.written
            .store(written.wrapping_add(1), Ordering::Release);
        Ok(())
    }
}

impl CommandReceiver {
    pub(crate) fn receive(&mut self) -> Option<Option<TransportCommand>> {
        let ring = &self.ring;
        let read = ring.read.load(Ordering::Relaxed);
        let written = ring.written.load(Ordering::Acquire);
        if read == written {
            return None;
        }
        let word = ring
            .slots
            .get(read % ring.slots.len())
            .map(|slot| slot.load(Ordering::Relaxed));
        ring.read.store(read.wrapping_add(1), Ordering::Release);
        Some(word.and_then(decode))
    }
}
