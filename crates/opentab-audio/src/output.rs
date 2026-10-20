use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::{BLOCK_FRAMES, SampleRate, Transport};

pub const NULL_TICK_MILLIS: u64 = 5;
const NANOS_PER_SECOND: u128 = 1_000_000_000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OutputError {
    #[error("the output is already playing")]
    AlreadyRunning,
    #[error("the transport runs at {transport} Hz and the output at {output} Hz")]
    RateMismatch { transport: u32, output: u32 },
    #[error("no audio output device was found")]
    NoDevice,
    #[error("the audio device offers no stereo format at 44100 or 48000 Hz: {message}")]
    UnsupportedDevice { message: String },
    #[error("the audio stream failed: {message}")]
    Stream { message: String },
    #[error("the output thread stopped unexpectedly")]
    Thread,
}

pub trait AudioOutput {
    fn rate(&self) -> SampleRate;
    fn start(&mut self, transport: Transport) -> Result<(), OutputError>;
    fn stop(&mut self) -> Option<Transport>;
}

pub(crate) fn check_rate(transport: &Transport, rate: SampleRate) -> Result<(), OutputError> {
    if transport.rate() == rate {
        Ok(())
    } else {
        Err(OutputError::RateMismatch {
            transport: transport.rate().hz(),
            output: rate.hz(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NullClock {
    Manual,
    RealTime,
}

struct ManualRun {
    transport: Transport,
    left: Vec<f32>,
    right: Vec<f32>,
}

struct RealTimeRun {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<Transport>,
}

enum NullRun {
    Idle,
    Manual(Box<ManualRun>),
    RealTime(RealTimeRun),
}

pub struct NullOutput {
    rate: SampleRate,
    clock: NullClock,
    run: NullRun,
}

fn real_time(mut transport: Transport, rate: SampleRate, stop: &AtomicBool) -> Transport {
    let mut left = vec![0.0f32; BLOCK_FRAMES];
    let mut right = vec![0.0f32; BLOCK_FRAMES];
    let started = Instant::now();
    let mut produced: u64 = 0;
    while !stop.load(Ordering::Acquire) {
        let due = started.elapsed().as_nanos() * u128::from(rate.hz()) / NANOS_PER_SECOND;
        let due = u64::try_from(due).unwrap_or(u64::MAX);
        while produced < due {
            let frames = usize::try_from(due - produced)
                .unwrap_or(BLOCK_FRAMES)
                .min(BLOCK_FRAMES);
            transport.process(&mut left[..frames], &mut right[..frames]);
            produced += frames as u64;
        }
        std::thread::sleep(Duration::from_millis(NULL_TICK_MILLIS));
    }
    transport
}

impl NullOutput {
    pub fn manual(rate: SampleRate) -> NullOutput {
        NullOutput {
            rate,
            clock: NullClock::Manual,
            run: NullRun::Idle,
        }
    }

    pub fn real_time(rate: SampleRate) -> NullOutput {
        NullOutput {
            rate,
            clock: NullClock::RealTime,
            run: NullRun::Idle,
        }
    }

    pub fn clock(&self) -> NullClock {
        self.clock
    }

    pub fn is_running(&self) -> bool {
        !matches!(self.run, NullRun::Idle)
    }

    pub fn step(&mut self, frames: u64) -> bool {
        let NullRun::Manual(run) = &mut self.run else {
            return false;
        };
        let mut left_over = frames;
        while left_over > 0 {
            let count = usize::try_from(left_over)
                .unwrap_or(BLOCK_FRAMES)
                .min(BLOCK_FRAMES);
            run.transport
                .process(&mut run.left[..count], &mut run.right[..count]);
            left_over -= count as u64;
        }
        true
    }

    pub fn last_block(&self) -> Option<(&[f32], &[f32])> {
        match &self.run {
            NullRun::Manual(run) => Some((&run.left, &run.right)),
            NullRun::Idle | NullRun::RealTime(_) => None,
        }
    }
}

impl AudioOutput for NullOutput {
    fn rate(&self) -> SampleRate {
        self.rate
    }

    fn start(&mut self, transport: Transport) -> Result<(), OutputError> {
        if self.is_running() {
            return Err(OutputError::AlreadyRunning);
        }
        check_rate(&transport, self.rate)?;
        self.run = match self.clock {
            NullClock::Manual => NullRun::Manual(Box::new(ManualRun {
                transport,
                left: vec![0.0; BLOCK_FRAMES],
                right: vec![0.0; BLOCK_FRAMES],
            })),
            NullClock::RealTime => {
                let stop = Arc::new(AtomicBool::new(false));
                let flag = Arc::clone(&stop);
                let rate = self.rate;
                let thread = std::thread::Builder::new()
                    .name("opentab-null-output".to_owned())
                    .spawn(move || real_time(transport, rate, &flag))
                    .map_err(|_| OutputError::Thread)?;
                NullRun::RealTime(RealTimeRun { stop, thread })
            }
        };
        Ok(())
    }

    fn stop(&mut self) -> Option<Transport> {
        match std::mem::replace(&mut self.run, NullRun::Idle) {
            NullRun::Idle => None,
            NullRun::Manual(run) => Some(run.transport),
            NullRun::RealTime(run) => {
                run.stop.store(true, Ordering::Release);
                run.thread.join().ok()
            }
        }
    }
}

impl Drop for NullOutput {
    fn drop(&mut self) {
        if let NullRun::RealTime(run) = std::mem::replace(&mut self.run, NullRun::Idle) {
            run.stop.store(true, Ordering::Release);
            let _ = run.thread.join();
        }
    }
}
