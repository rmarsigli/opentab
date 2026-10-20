use std::sync::atomic::{AtomicI64, AtomicU8, AtomicU64, Ordering, fence};

use opentab_model::Fraction;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum TransportState {
    #[default]
    Stopped,
    Playing,
    Paused,
    CountIn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransportStatus {
    pub state: TransportState,
    pub played_bar: usize,
    pub source_bar: usize,
    pub offset: Fraction,
    pub frame: u64,
}

impl Default for TransportStatus {
    fn default() -> TransportStatus {
        TransportStatus {
            state: TransportState::Stopped,
            played_bar: 0,
            source_bar: 0,
            offset: Fraction::new_raw(0, 1),
            frame: 0,
        }
    }
}

fn state_code(state: TransportState) -> u8 {
    match state {
        TransportState::Stopped => 0,
        TransportState::Playing => 1,
        TransportState::Paused => 2,
        TransportState::CountIn => 3,
    }
}

fn state_of(code: u8) -> TransportState {
    match code {
        1 => TransportState::Playing,
        2 => TransportState::Paused,
        3 => TransportState::CountIn,
        _ => TransportState::Stopped,
    }
}

fn wide(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn narrow(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

const YIELD_AFTER: u32 = 64;

#[derive(Debug, Default)]
pub(crate) struct StatusCell {
    version: AtomicU64,
    state: AtomicU8,
    played_bar: AtomicU64,
    source_bar: AtomicU64,
    offset_numer: AtomicI64,
    offset_denom: AtomicI64,
    frame: AtomicU64,
}

impl StatusCell {
    pub(crate) fn new() -> StatusCell {
        let cell = StatusCell::default();
        cell.offset_denom.store(1, Ordering::Relaxed);
        cell
    }

    pub(crate) fn publish(&self, status: &TransportStatus) {
        let version = self.version.load(Ordering::Relaxed);
        self.version
            .store(version.wrapping_add(1), Ordering::Relaxed);
        fence(Ordering::Release);
        self.state
            .store(state_code(status.state), Ordering::Relaxed);
        self.played_bar
            .store(wide(status.played_bar), Ordering::Relaxed);
        self.source_bar
            .store(wide(status.source_bar), Ordering::Relaxed);
        self.offset_numer
            .store(*status.offset.numer(), Ordering::Relaxed);
        self.offset_denom
            .store(*status.offset.denom(), Ordering::Relaxed);
        self.frame.store(status.frame, Ordering::Relaxed);
        self.version
            .store(version.wrapping_add(2), Ordering::Release);
    }

    pub(crate) fn read(&self) -> TransportStatus {
        let mut attempts: u32 = 0;
        loop {
            let before = self.version.load(Ordering::Acquire);
            if before.is_multiple_of(2) {
                let state = state_of(self.state.load(Ordering::Relaxed));
                let played_bar = narrow(self.played_bar.load(Ordering::Relaxed));
                let source_bar = narrow(self.source_bar.load(Ordering::Relaxed));
                let numer = self.offset_numer.load(Ordering::Relaxed);
                let denom = self.offset_denom.load(Ordering::Relaxed);
                let frame = self.frame.load(Ordering::Relaxed);
                fence(Ordering::Acquire);
                if self.version.load(Ordering::Relaxed) == before && denom > 0 {
                    return TransportStatus {
                        state,
                        played_bar,
                        source_bar,
                        offset: Fraction::new_raw(numer, denom),
                        frame,
                    };
                }
            }
            attempts = attempts.saturating_add(1);
            if attempts > YIELD_AFTER {
                std::thread::yield_now();
            } else {
                std::hint::spin_loop();
            }
        }
    }
}
