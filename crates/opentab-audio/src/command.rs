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

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{BarRange, COMMAND_CAPACITY, TransportCommand, command_queue, decode, encode};
    use opentab_playback::Speed;

    const SENT: usize = 200_000;

    fn arb_command() -> impl Strategy<Value = TransportCommand> {
        let index = prop_oneof![0usize..4096, any::<usize>()];
        prop_oneof![
            Just(TransportCommand::Play),
            Just(TransportCommand::Pause),
            Just(TransportCommand::Stop),
            Just(TransportCommand::ClearLoop),
            index.clone().prop_map(TransportCommand::Seek),
            (0usize..1 << 20, 0usize..1 << 20)
                .prop_map(|(first, last)| TransportCommand::Loop(BarRange { first, last })),
            (25u16..=200)
                .prop_map(|percent| TransportCommand::Speed(Speed::new(percent).expect("speed"))),
            (index.clone(), any::<bool>())
                .prop_map(|(track, on)| TransportCommand::Mute(track, on)),
            (index, any::<bool>()).prop_map(|(track, on)| TransportCommand::Solo(track, on)),
            any::<bool>().prop_map(TransportCommand::Metronome),
            any::<bool>().prop_map(TransportCommand::CountIn),
        ]
    }

    fn huge(index: usize) -> bool {
        index >= 1 << 28
    }

    proptest! {
        #[test]
        fn every_command_survives_the_queue_word(command in arb_command()) {
            let decoded = decode(encode(command)).expect("decodes");
            match (command, decoded) {
                (TransportCommand::Seek(sent), TransportCommand::Seek(got))
                | (TransportCommand::Mute(sent, _), TransportCommand::Mute(got, _))
                | (TransportCommand::Solo(sent, _), TransportCommand::Solo(got, _))
                    if huge(sent) =>
                {
                    prop_assert!(huge(got));
                }
                _ => prop_assert_eq!(decoded, command),
            }
        }
    }

    #[test]
    fn the_queue_refuses_beyond_its_capacity_and_keeps_order_across_threads() {
        let (mut sender, mut receiver) = command_queue();
        for bar in 0..COMMAND_CAPACITY {
            sender.send(TransportCommand::Seek(bar)).expect("room");
        }
        assert!(sender.send(TransportCommand::Play).is_err());
        assert_eq!(receiver.receive(), Some(Some(TransportCommand::Seek(0))));
        sender.send(TransportCommand::Play).expect("room again");
        while receiver.receive().is_some() {}
        let producer = std::thread::spawn(move || {
            for bar in 0..SENT {
                while sender.send(TransportCommand::Seek(bar)).is_err() {
                    std::thread::yield_now();
                }
            }
        });
        let mut next = 0;
        while next < SENT {
            match receiver.receive() {
                Some(Some(TransportCommand::Seek(bar))) => {
                    assert_eq!(bar, next);
                    next += 1;
                }
                Some(other) => panic!("unexpected {other:?}"),
                None => std::thread::yield_now(),
            }
        }
        producer.join().expect("producer");
        assert_eq!(receiver.receive(), None);
    }
}
