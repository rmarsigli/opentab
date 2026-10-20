use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig, SupportedStreamConfig};

use crate::output::{AudioOutput, OutputError, check_rate};
use crate::{BLOCK_FRAMES, SampleRate, Transport};

const STEREO: u16 = 2;

pub struct CpalOutput {
    device: cpal::Device,
    config: SupportedStreamConfig,
    rate: SampleRate,
    stream: Option<cpal::Stream>,
    failed: Arc<AtomicBool>,
}

fn unsupported(message: impl ToString) -> OutputError {
    OutputError::UnsupportedDevice {
        message: message.to_string(),
    }
}

fn stream_error(error: impl ToString) -> OutputError {
    OutputError::Stream {
        message: error.to_string(),
    }
}

fn choose(device: &cpal::Device) -> Result<(SupportedStreamConfig, SampleRate), OutputError> {
    let default = device.default_output_config().map_err(unsupported)?;
    if let Ok(rate) = SampleRate::new(default.sample_rate()) {
        return Ok((default, rate));
    }
    let ranges = device.supported_output_configs().map_err(unsupported)?;
    let mut fallback: Option<(SupportedStreamConfig, SampleRate)> = None;
    for range in ranges {
        for rate in SampleRate::SUPPORTED.iter().rev() {
            if let Some(config) = range.try_with_sample_rate(rate.hz()) {
                let stereo = config.channels() >= STEREO;
                if stereo && config.sample_format() == SampleFormat::F32 {
                    return Ok((config, *rate));
                }
                if fallback.is_none() {
                    fallback = Some((config, *rate));
                }
            }
        }
    }
    fallback.ok_or_else(|| unsupported("no supported rate"))
}

struct Callback {
    transport: Transport,
    left: Vec<f32>,
    right: Vec<f32>,
    channels: usize,
}

impl Callback {
    fn fill<T>(&mut self, data: &mut [T])
    where
        T: SizedSample + FromSample<f32>,
    {
        let channels = self.channels.max(1);
        for chunk in data.chunks_mut(channels * BLOCK_FRAMES) {
            let frames = chunk.len() / channels;
            let (Some(left), Some(right)) =
                (self.left.get_mut(..frames), self.right.get_mut(..frames))
            else {
                return;
            };
            self.transport.process(left, right);
            for ((frame, left), right) in chunk
                .chunks_mut(channels)
                .zip(left.iter())
                .zip(right.iter())
            {
                for (index, sample) in frame.iter_mut().enumerate() {
                    let value = match (channels, index) {
                        (1, _) => (*left + *right) * 0.5,
                        (_, 0) => *left,
                        (_, 1) => *right,
                        _ => 0.0,
                    };
                    *sample = T::from_sample(value);
                }
            }
        }
    }
}

impl CpalOutput {
    pub fn open() -> Result<CpalOutput, OutputError> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or(OutputError::NoDevice)?;
        let (config, rate) = choose(&device)?;
        Ok(CpalOutput {
            device,
            config,
            rate,
            stream: None,
            failed: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn has_failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }

    fn build<T>(&self, transport: Transport) -> Result<cpal::Stream, OutputError>
    where
        T: SizedSample + FromSample<f32>,
    {
        let config: StreamConfig = self.config.config();
        let mut callback = Callback {
            transport,
            left: vec![0.0; BLOCK_FRAMES],
            right: vec![0.0; BLOCK_FRAMES],
            channels: usize::from(config.channels),
        };
        let failed = Arc::clone(&self.failed);
        self.device
            .build_output_stream::<T, _, _>(
                config,
                move |data: &mut [T], _: &cpal::OutputCallbackInfo| callback.fill(data),
                move |_| failed.store(true, Ordering::Release),
                None,
            )
            .map_err(stream_error)
    }
}

impl AudioOutput for CpalOutput {
    fn rate(&self) -> SampleRate {
        self.rate
    }

    fn start(&mut self, transport: Transport) -> Result<(), OutputError> {
        if self.stream.is_some() {
            return Err(OutputError::AlreadyRunning);
        }
        check_rate(&transport, self.rate)?;
        let stream = match self.config.sample_format() {
            SampleFormat::F32 => self.build::<f32>(transport),
            SampleFormat::I16 => self.build::<i16>(transport),
            SampleFormat::U16 => self.build::<u16>(transport),
            SampleFormat::I32 => self.build::<i32>(transport),
            other => Err(unsupported(format!("sample format {other:?}"))),
        }?;
        stream.play().map_err(stream_error)?;
        self.stream = Some(stream);
        Ok(())
    }

    fn stop(&mut self) -> Option<Transport> {
        self.stream = None;
        None
    }
}
