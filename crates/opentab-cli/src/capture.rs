use opentab_audio::{
    RENDER_TAIL_MS, RenderedAudio, Session, TransportCommand, TransportState, frame_at,
};
use opentab_playback::ScoreTime;

use crate::error::CliError;
use crate::sound::Prepared;

const BLOCK_FRAMES: usize = 512;
pub const OFFLINE_LIMIT_SECONDS: u64 = 1200;

fn deliver(session: &mut Session, command: TransportCommand) {
    while session.handle.send(command).is_err() {
        session.transport.process(&mut [], &mut []);
    }
}

struct Buffers {
    left: Vec<f32>,
    right: Vec<f32>,
}

impl Buffers {
    fn step(&mut self, session: &mut Session, frames: usize) {
        let start = self.left.len();
        self.left.resize(start + frames, 0.0);
        self.right.resize(start + frames, 0.0);
        session
            .transport
            .process(&mut self.left[start..], &mut self.right[start..]);
    }

    fn frames(&self) -> u64 {
        self.left.len() as u64
    }
}

fn limit_frames(session: &Session) -> u64 {
    u64::from(session.transport.rate().hz()) * OFFLINE_LIMIT_SECONDS
}

fn too_long() -> CliError {
    CliError::InvalidInput {
        message: format!(
            "the render would last more than {} minutes; use --bars to render part of the score",
            OFFLINE_LIMIT_SECONDS / 60
        ),
    }
}

fn estimate(prepared: &Prepared) -> Option<u64> {
    let session = &prepared.session;
    let performance = session.transport.performance();
    let rate = session.transport.rate();
    let speed = prepared.speed;
    let (from, to) = match prepared.range {
        Some(range) => {
            let first = performance.order.bars.get(range.first)?;
            let last = performance.order.bars.get(range.last)?;
            let end = ScoreTime::new(last.start.whole_notes() + last.length).ok()?;
            (first.start, end)
        }
        None => (ScoreTime::new(0.into()).ok()?, performance.order.end()),
    };
    let start = frame_at(&performance.tempo, from, speed, rate).ok()?;
    let end = frame_at(&performance.tempo, to, speed, rate).ok()?;
    Some(end.saturating_sub(start))
}

pub fn capture(prepared: &mut Prepared) -> Result<RenderedAudio, CliError> {
    let limit = limit_frames(&prepared.session);
    if estimate(prepared).is_some_and(|frames| frames > limit) {
        return Err(too_long());
    }
    let rate = prepared.session.transport.rate();
    let range = prepared.range;
    let session = &mut prepared.session;
    if let Some(range) = range {
        deliver(session, TransportCommand::Seek(range.first));
    }
    if prepared.tracks.len() < prepared.names.len() {
        for track in &prepared.tracks {
            deliver(session, TransportCommand::Solo(*track, true));
        }
    }
    deliver(session, TransportCommand::Play);
    let mut buffers = Buffers {
        left: Vec::new(),
        right: Vec::new(),
    };
    let mut size = 1;
    loop {
        buffers.step(session, size);
        if buffers.frames() > limit {
            return Err(too_long());
        }
        let status = session.handle.status();
        if status.state == TransportState::Stopped {
            break;
        }
        if let Some(range) = range {
            if status.state == TransportState::Playing && status.played_bar > range.last {
                deliver(session, TransportCommand::Pause);
                break;
            }
            size = if status.played_bar + 1 >= range.last {
                1
            } else {
                BLOCK_FRAMES
            };
        } else {
            size = BLOCK_FRAMES;
        }
    }
    let mut tail = rate.frames_of_millis(RENDER_TAIL_MS);
    while tail > 0 {
        let size = usize::try_from(tail)
            .unwrap_or(BLOCK_FRAMES)
            .min(BLOCK_FRAMES);
        buffers.step(session, size);
        tail -= size as u64;
    }
    if buffers.frames() > limit {
        return Err(too_long());
    }
    Ok(RenderedAudio {
        rate,
        frames: buffers.frames(),
        left: buffers.left,
        right: buffers.right,
    })
}
