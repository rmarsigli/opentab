use opentab_model::Fraction;
use opentab_playback::{Performance, PlaybackWarning, UnplayedFeature};

use crate::command::TransportCommand;
use crate::transport::{SessionConfig, Transport, TransportHandle};
use crate::{AudioError, BankFormat, BuiltInSynth, MAX_FRAMES, RenderedAudio, Synth};

pub const BUILT_IN_BANK_NAME: &str = "built-in";

pub struct Session {
    pub transport: Transport,
    pub handle: TransportHandle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScriptStep {
    pub frame: u64,
    pub command: TransportCommand,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TrackBackend {
    pub track: usize,
    pub format: BankFormat,
    pub bank: String,
    pub fallback: Option<String>,
}

impl TrackBackend {
    pub fn built_in(track: usize) -> TrackBackend {
        TrackBackend {
            track,
            format: BankFormat::BuiltIn,
            bank: BUILT_IN_BANK_NAME.to_owned(),
            fallback: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderReport {
    pub seconds: Fraction,
    pub played_bars: Vec<usize>,
    pub backends: Vec<TrackBackend>,
    pub unplayed: Vec<UnplayedFeature>,
    pub warnings: Vec<PlaybackWarning>,
}

impl Session {
    pub fn new(
        performance: Performance,
        voices: Vec<Box<dyn Synth>>,
        config: SessionConfig,
    ) -> Result<Session, AudioError> {
        let click = Box::new(BuiltInSynth::new(config.rate));
        Session::with_click(performance, voices, click, config)
    }

    pub fn with_click(
        performance: Performance,
        voices: Vec<Box<dyn Synth>>,
        click: Box<dyn Synth>,
        config: SessionConfig,
    ) -> Result<Session, AudioError> {
        let (transport, handle) = Transport::create(performance, voices, click, config)?;
        Ok(Session { transport, handle })
    }

    pub fn split(self) -> (Transport, TransportHandle) {
        (self.transport, self.handle)
    }

    pub fn report(&self, backends: Vec<TrackBackend>) -> RenderReport {
        let performance = self.transport.performance();
        let rate = i64::from(self.transport.rate().hz());
        let frames = i64::try_from(self.transport.clock()).unwrap_or(i64::MAX);
        RenderReport {
            seconds: Fraction::new(frames, rate),
            played_bars: self.transport.played_bars(),
            backends,
            unplayed: performance.unplayed.clone(),
            warnings: performance.warnings.clone(),
        }
    }

    fn deliver(&mut self, command: TransportCommand) {
        while self.handle.send(command).is_err() {
            self.transport.process(&mut [], &mut []);
        }
    }
}

pub fn render_session(
    session: &mut Session,
    script: &[ScriptStep],
    frames: u64,
) -> Result<RenderedAudio, AudioError> {
    if frames > MAX_FRAMES {
        return Err(AudioError::TooLong { frames });
    }
    let length = usize::try_from(frames).map_err(|_| AudioError::TooLong { frames })?;
    let mut steps = script.to_vec();
    steps.sort_by_key(|step| step.frame);
    let mut left = vec![0.0; length];
    let mut right = vec![0.0; length];
    session.deliver(TransportCommand::Play);
    let mut pending = steps.iter().peekable();
    let mut cursor: usize = 0;
    loop {
        while let Some(step) = pending.next_if(|step| step.frame <= cursor as u64) {
            session.deliver(step.command);
        }
        if cursor >= length {
            break;
        }
        let until = pending
            .peek()
            .and_then(|step| usize::try_from(step.frame).ok())
            .map_or(length, |frame| frame.min(length));
        session
            .transport
            .process(&mut left[cursor..until], &mut right[cursor..until]);
        cursor = until;
    }
    Ok(RenderedAudio {
        rate: session.transport.rate(),
        frames,
        left,
        right,
    })
}
