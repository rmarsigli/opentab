use crate::AudioError;

const HZ_44100: u32 = 44_100;
const HZ_48000: u32 = 48_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SampleRate(u32);

impl SampleRate {
    pub const CD: SampleRate = SampleRate(HZ_44100);
    pub const STUDIO: SampleRate = SampleRate(HZ_48000);
    pub const SUPPORTED: [SampleRate; 2] = [SampleRate::CD, SampleRate::STUDIO];

    pub fn new(hz: u32) -> Result<SampleRate, AudioError> {
        SampleRate::SUPPORTED
            .into_iter()
            .find(|rate| rate.0 == hz)
            .ok_or(AudioError::UnsupportedRate { hz })
    }

    pub fn hz(self) -> u32 {
        self.0
    }

    pub fn frames_of_millis(self, millis: u32) -> u64 {
        u64::from(self.0) * u64::from(millis) / 1000
    }
}

impl Default for SampleRate {
    fn default() -> SampleRate {
        SampleRate::STUDIO
    }
}
