use opentab_audio::{
    BarRange, RENDER_TAIL_MS, RenderedAudio, SampleRate, Session, TransportCommand, TransportState,
    frame_at,
};
use opentab_playback::{ScoreTime, Speed};

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

    fn drop_last(&mut self) {
        self.left.pop();
        self.right.pop();
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

fn bar_end(session: &Session, bar: usize) -> ScoreTime {
    let order = &session.transport.performance().order;
    match order.bars.get(bar + 1) {
        Some(next) => next.start,
        None => order.end(),
    }
}

fn frames_to_end(session: &Session, range: BarRange, speed: Speed, rate: SampleRate) -> Vec<u64> {
    let tempo = &session.transport.performance().tempo;
    let Ok(end) = frame_at(tempo, bar_end(session, range.last), speed, rate) else {
        return Vec::new();
    };
    (0..=range.last)
        .map(|bar| {
            let finish = frame_at(tempo, bar_end(session, bar), speed, rate).unwrap_or(end);
            end.saturating_sub(finish)
        })
        .collect()
}

fn block_size(remaining: &[u64], played_bar: usize) -> usize {
    let frames = remaining.get(played_bar).copied().unwrap_or(1);
    usize::try_from(frames)
        .unwrap_or(BLOCK_FRAMES)
        .clamp(1, BLOCK_FRAMES)
}

pub fn capture(prepared: &mut Prepared) -> Result<RenderedAudio, CliError> {
    let limit = limit_frames(&prepared.session);
    if estimate(prepared).is_some_and(|frames| frames > limit) {
        return Err(too_long());
    }
    let rate = prepared.session.transport.rate();
    let range = prepared.range;
    let session = &mut prepared.session;
    let bars = session.transport.performance().order.bars.len();
    let scope = range.or_else(|| bars.checked_sub(1).map(|last| BarRange { first: 0, last }));
    let remaining = scope
        .map(|scope| frames_to_end(session, scope, prepared.speed, rate))
        .unwrap_or_default();
    if prepared.tracks.len() < prepared.names.len() {
        for track in &prepared.tracks {
            deliver(session, TransportCommand::Solo(*track, true));
        }
    }
    deliver(
        session,
        range.map_or(TransportCommand::Play, TransportCommand::PlayRange),
    );
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
            if size == 1 {
                buffers.drop_last();
            }
            break;
        }
        if status.state == TransportState::Paused {
            break;
        }
        size = block_size(&remaining, status.played_bar);
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
